use claude_desktop_tauri_lib::tools::execute_tool;
use serde_json::json;

#[test]
fn grep_honors_include_and_resolves_relative_paths_in_project() {
    let temp = tempfile::tempdir().unwrap();
    std::fs::create_dir(temp.path().join("src")).unwrap();
    std::fs::write(temp.path().join("src/main.rs"), "// 中文 needle\n").unwrap();
    std::fs::write(temp.path().join("src/notes.txt"), "needle\n").unwrap();
    let cwd = temp.path().to_str().unwrap();
    for include in ["*.rs", "**/*.rs"] {
        let result = execute_tool("Grep", json!({"pattern":"needle", "path":"src", "include":include}), cwd).unwrap();
        assert_eq!(result["count"], 1);
        assert_eq!(result["matches"][0]["content"], "// 中文 needle");
    }
    let file = execute_tool("Grep", json!({"pattern":"needle", "path":"src/main.rs", "include":"*.rs"}), cwd).unwrap();
    assert_eq!(file["count"], 1);
    assert!(execute_tool("Grep", json!({"pattern":"needle", "include":"["}), cwd).is_err());
}

#[test]
fn glob_matches_project_relative_patterns_and_only_returns_files() {
    let temp = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(temp.path().join("src/nested")).unwrap();
    std::fs::create_dir(temp.path().join("src/directory.rs")).unwrap();
    std::fs::write(temp.path().join("src/main.rs"), "").unwrap();
    std::fs::write(temp.path().join("src/nested/lib.rs"), "").unwrap();
    let cwd = temp.path().to_str().unwrap();
    let result = execute_tool("Glob", json!({"pattern":"src/**/*.rs"}), cwd).unwrap();
    assert_eq!(result["count"], 2);
    let scoped = execute_tool("Glob", json!({"pattern":"**/*.rs", "path":"src"}), cwd).unwrap();
    assert_eq!(scoped["count"], 2);
    assert!(execute_tool("Glob", json!({"pattern":"["}), cwd).is_err());
}
