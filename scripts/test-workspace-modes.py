"""Check persisted workspace modes against tauri dev without calling a model."""
import tempfile
import runpy
from pathlib import Path

request = runpy.run_path(str(Path(__file__).with_name("test-project-workflow.py")))["request"]


def main():
    conversations = []
    project_id = None
    try:
        for mode in ("chat", "cowork", "code"):
            chat = request("/conversations", "POST", {"title": "Mode verification", "workspace_mode": mode})
            conversations.append(chat["id"])
            assert chat["workspace_mode"] == mode
            assert request(f"/conversations/{chat['id']}")["workspace_mode"] == mode
        listed = request("/conversations")["conversations"]
        assert {c["workspace_mode"] for c in listed if c["id"] in conversations} == {"chat", "cowork", "code"}
        legacy = request("/conversations", "POST", {"title": "Default mode verification"})
        conversations.append(legacy["id"])
        assert legacy["workspace_mode"] == "chat"
        branch = request(f"/conversations/{conversations[2]}/branch", "POST", {})
        conversations.append(branch["new_conversation_id"])
        assert request(f"/conversations/{conversations[-1]}")["workspace_mode"] == "code"
        print("PASS: create/list/detail preserve all modes; legacy defaults to Chat; branch retains Code")
        with tempfile.TemporaryDirectory(prefix="claude-mode-project-") as folder:
            project = request("/projects", "POST", {"name": "Mode verification", "folders": [folder]})
            project_id = project["id"]
            request(f"/projects/{project_id}/trust", "POST", {"folders": [folder]})
            chat = request(f"/projects/{project_id}/conversations", "POST", {"workspace_mode": "cowork"})
            conversations.append(chat["id"])
            assert request(f"/conversations/{chat['id']}")["workspace_mode"] == "cowork"
            print("PASS: project conversation retains Cowork mode")
    finally:
        for conversation_id in conversations:
            request(f"/conversations/{conversation_id}", "DELETE")
        if project_id:
            request(f"/projects/{project_id}", "DELETE")
        print("Test records removed")


if __name__ == "__main__":
    main()
