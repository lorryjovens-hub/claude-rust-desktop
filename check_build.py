import os, subprocess, sys

os.chdir(os.path.dirname(os.path.abspath(__file__)))

release_dir = "src-tauri/target/release"
if os.path.exists(release_dir):
    exes = [f for f in os.listdir(release_dir) if f.endswith('.exe')]
    if exes:
        for exe in exes:
            path = os.path.join(release_dir, exe)
            size_mb = os.path.getsize(path) / (1024*1024)
            mtime = os.path.getmtime(path)
            import datetime
            dt = datetime.datetime.fromtimestamp(mtime)
            print(f"Found: {exe} ({size_mb:.1f} MB, built {dt})")
    else:
        print("No release exe found, starting build...")
        os.chdir("src-tauri")
        result = subprocess.run(["cargo", "build", "--release"], capture_output=True, text=True, timeout=600)
        print(result.stdout[-3000:] if len(result.stdout) > 3000 else result.stdout)
        if result.returncode != 0:
            print("BUILD ERRORS:", result.stderr[-3000:])
            sys.exit(1)
else:
    print("No target/release directory, starting build...")
    os.chdir("src-tauri")
    result = subprocess.run(["cargo", "build", "--release"], capture_output=True, text=True, timeout=600)
    print(result.stdout[-3000:] if len(result.stdout) > 3000 else result.stdout)
    if result.returncode != 0:
        print("BUILD ERRORS:", result.stderr[-3000:])
        sys.exit(1)
