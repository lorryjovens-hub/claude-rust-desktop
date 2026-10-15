use serde_json::{json, Value};
use std::process::{Command, Output};

fn invoke(root: &std::path::Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_claude-code"))
        .arg("--data-dir")
        .arg(root.join("data"))
        .args(args)
        .output()
        .unwrap()
}

fn success(output: Output) -> Value {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

#[test]
fn migration_settings_persist_and_project_preview_does_not_write() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    assert_eq!(
        success(invoke(root, &["learning", "status"]))["enabled"],
        false
    );
    success(invoke(root, &["learning", "configure", "--enabled"]));
    assert_eq!(
        success(invoke(root, &["learning", "status"]))["enabled"],
        true
    );
    success(invoke(root, &["learning", "configure"]));
    assert_eq!(
        success(invoke(root, &["learning", "status"]))["enabled"],
        false
    );
    assert!(!invoke(
        root,
        &[
            "performance",
            "configure",
            "--enabled",
            "--provider",
            "missing",
            "--model",
            "missing"
        ]
    )
    .status
    .success());
    assert_eq!(
        success(invoke(root, &["performance", "status"]))["enabled"],
        false
    );
    let parent = root.to_str().unwrap();
    success(invoke(
        root,
        &["project", "preview", "sample", "--parent", parent],
    ));
    assert!(!root.join("sample").exists());
    success(invoke(
        root,
        &["project", "create", "sample", "--parent", parent],
    ));
    assert!(std::fs::read_to_string(root.join("sample/README.md"))
        .unwrap()
        .contains("sample"));
    assert!(
        !invoke(root, &["project", "create", "sample", "--parent", parent])
            .status
            .success()
    );
}

#[test]
fn project_commands_and_remote_mutations_require_explicit_consent() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    let template = root.join("template.json");
    std::fs::write(&template, json!({"name":"custom","files":[{"path":"README.md","content":"sample"}],"commands":["echo accepted"]}).to_string()).unwrap();
    let output = invoke(
        root,
        &[
            "project",
            "create",
            "sample",
            "--parent",
            root.to_str().unwrap(),
            "--template-file",
            template.to_str().unwrap(),
        ],
    );
    assert!(!output.status.success());
    assert!(!root.join("sample").exists());
    let request = root.join("request.json");
    std::fs::write(
        &request,
        json!({"endpoint":"http://127.0.0.1:1/","method":"POST"}).to_string(),
    )
    .unwrap();
    let output = invoke(root, &["remote", request.to_str().unwrap()]);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("--yes"));
}

#[test]
fn legacy_skill_import_is_isolated_previewed_and_discoverable() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    let source = root.join("old-skills");
    std::fs::create_dir_all(source.join("example")).unwrap();
    let markdown = "---\nname: Legacy Fixture\ndescription: Imported fixture\n---\nUse the normal tool permissions.\n";
    std::fs::write(source.join("example/SKILL.md"), markdown).unwrap();
    success(invoke(
        root,
        &[
            "skills",
            "import-legacy",
            source.to_str().unwrap(),
            "--preview",
        ],
    ));
    assert!(!root.join("data/skills").exists());
    success(invoke(
        root,
        &["skills", "import-legacy", source.to_str().unwrap()],
    ));
    let skills = success(invoke(root, &["skills", "list"]));
    assert!(skills
        .as_array()
        .unwrap()
        .iter()
        .any(|skill| skill["name"] == "Legacy Fixture"));
    assert_eq!(
        std::fs::read_to_string(source.join("example/SKILL.md")).unwrap(),
        markdown
    );
    assert_eq!(
        success(invoke(
            root,
            &["skills", "import-legacy", source.to_str().unwrap()]
        ))["skipped"],
        true
    );
}
