use serde_json::{json, Value};
use std::process::Command;

#[test]
fn imported_python_plugin_runs_in_standalone_cli_with_permission() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("source");
    std::fs::create_dir(&source).unwrap();
    std::fs::write(source.join("plugin.json"), json!({"name":"old-python","version":"1.0.0","description":null,"main":"main.py","commands":[{"name":"echo","description":"echo","usage":null,"examples":[]}],"hooks":[],"dependencies":{},"permissions":[],"enabled":true}).to_string()).unwrap();
    std::fs::write(
        source.join("main.py"),
        "import sys,json\nprint(json.dumps(sys.argv[1:]))",
    )
    .unwrap();
    let invoke = |args: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_claude-code"))
            .arg("--data-dir")
            .arg(temp.path().join("data"))
            .args(args)
            .output()
            .unwrap()
    };
    let result = invoke(&[
        "plugin",
        "import-legacy",
        source.to_str().unwrap(),
        "--preview",
    ]);
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert_eq!(
        serde_json::from_slice::<Value>(&result.stdout).unwrap()["imported"],
        false
    );
    assert!(
        invoke(&["plugin", "import-legacy", source.to_str().unwrap()])
            .status
            .success()
    );
    assert!(!invoke(&["plugin", "run", "old-python", "echo"])
        .status
        .success());
    let result = invoke(&[
        "plugin",
        "run",
        "old-python",
        "echo",
        "--input",
        r#"{"hello":"world"}"#,
        "--yes",
    ]);
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let result: Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(
        serde_json::from_str::<Value>(result["stdout"].as_str().unwrap()).unwrap(),
        json!(["--hello=world"])
    );
    assert!(invoke(&["plugin", "disable", "old-python"])
        .status
        .success());
    assert!(!invoke(&["plugin", "run", "old-python", "echo", "--yes"])
        .status
        .success());
}
