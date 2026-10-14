"""Exercise project persistence against a running `tauri dev` instance.

Uses Vite's authenticated local proxy. Creates and removes only its own test
projects, empty conversations, and temporary source folders. No model calls.
"""
import json
import tempfile
from pathlib import Path
from urllib.error import HTTPError
from urllib.request import Request, urlopen

BASE = "http://127.0.0.1:5200/api"


def request(path, method="GET", body=None, status=200):
    data = json.dumps(body).encode("utf-8") if body is not None else None
    req = Request(BASE + path, data=data, method=method, headers={"Content-Type": "application/json"})
    try:
        response = urlopen(req, timeout=15)
    except HTTPError as error:
        response = error
    with response:
        actual = response.code
        result = json.load(response)
    assert actual == status, f"{method} {path}: expected {status}, got {actual}: {result}"
    return result


def main():
    project_id = None
    conversation_id = None
    with tempfile.TemporaryDirectory(prefix="claude-project-test-") as temporary:
        folders = [str(Path(temporary) / name) for name in ["first", "second", "third"]]
        for folder in folders:
            Path(folder).mkdir()
        try:
            request("/projects", "POST", {"name": "", "folders": folders[:1]}, status=400)
            request("/projects", "POST", {"name": "Missing folder", "folders": []}, status=400)
            project = request("/projects", "POST", {"name": "Project workflow test", "folders": folders[:2]})
            project_id = project["id"]
            route = f"/projects/{project_id}"
            assert len(project["folders"]) == 2 and not any(f["trusted"] for f in project["folders"])
            request(route + "/conversations", "POST", {}, status=400)
            print("PASS: required fields, multiple folders, and untrusted chat rejection")

            request(route + "/trust", "POST", {"folders": folders[:1]})
            request(route + "/conversations", "POST", {}, status=400)
            request(route + "/trust", "POST", {"folders": folders[1:2]})
            reloaded = request(route)
            assert all(f["trusted"] for f in reloaded["folders"])
            conversation = request(route + "/conversations", "POST", {"title": "Project workflow test"})
            conversation_id = conversation["id"]
            assert conversation["project_id"] == project_id
            assert conversation["workspace_path"] == reloaded["folders"][0]["path"]
            print("PASS: explicit per-folder trust persists and new chat uses the project workspace")

            updated = request(route, "PATCH", {"folders": folders})
            assert updated["folders"][0]["trusted"] and not updated["folders"][2]["trusted"]
            rejected = request(f"/conversations/{conversation_id}", "POST", {"project_id": project_id})
            assert rejected["ok"] is False
            request(route, "PATCH", {"name": "Should roll back", "folders": []}, status=400)
            assert request(route)["name"] == "Project workflow test"
            updated = request(route, "PATCH", {"folders": folders[1:2]})
            request(route, "PATCH", {"folders": []}, status=400)
            request(route + "/trust", "POST", {"folders": folders[2:]}, status=400)
            chat = request(f"/conversations/{conversation_id}")
            assert chat["workspace_path"] == updated["folders"][0]["path"]
            assert len(request(route)["folders"]) == 1
            print("PASS: add/remove, trust reset, last-folder guard, atomic validation, linked workspace update")
        finally:
            if conversation_id:
                request(f"/conversations/{conversation_id}", "DELETE")
            if project_id:
                request(f"/projects/{project_id}", "DELETE")
            print("Test data removed")


if __name__ == "__main__":
    main()
