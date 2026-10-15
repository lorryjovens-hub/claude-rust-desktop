use claude_core::{
    runtime::RuntimePaths,
    services::{agent_import, agents, EnvironmentCredentials, ServiceContext},
};
use serde_json::json;
use std::sync::Arc;

fn fixture() -> serde_json::Value {
    json!({"agent_type":"Custom","name":"Old Reader","description":"Read code","when_to_use":"Code review","tools":["file_read","search","list_files"],"model":"sonnet","system_prompt":"Inspect carefully.","source":"user","base_dir":"old/project"})
}
fn context(path: &std::path::Path) -> ServiceContext {
    ServiceContext::open(
        RuntimePaths::new(path.to_path_buf()).unwrap(),
        Arc::new(EnvironmentCredentials),
    )
    .unwrap()
}

#[test]
fn legacy_mapping_preview_backup_idempotence_conflicts_and_reopen() {
    let dir = tempfile::tempdir().unwrap();
    let ctx = context(dir.path());
    let source = dir.path().join("old.json");
    let raw = fixture().to_string();
    std::fs::write(&source, &raw).unwrap();
    let preview = agent_import::import(&ctx, &source, "OldReader", true).unwrap();
    assert_eq!(
        preview.definition.tools.unwrap(),
        ["Read", "Grep", "ListDir"]
    );
    assert!(agents::resolve(&ctx, "OldReader").is_err());
    assert!(!dir.path().join("backups").exists());
    let report = agent_import::import(&ctx, &source, "OldReader", false).unwrap();
    assert!(report.imported && !report.skipped);
    let backup = rusqlite::Connection::open(report.backup.unwrap()).unwrap();
    assert_eq!(
        backup
            .query_row(
                "SELECT COUNT(*) FROM peripheral_settings WHERE key='agent-definitions'",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
        0
    );
    let saved = agents::resolve(&context(dir.path()), "OldReader").unwrap();
    assert_eq!(
        serde_json::to_value(saved.legacy.unwrap()).unwrap(),
        fixture()
    );
    assert!(
        agent_import::import(&ctx, &source, "OldReader", false)
            .unwrap()
            .skipped
    );
    let mut changed = fixture();
    changed["system_prompt"] = json!("Changed");
    std::fs::write(&source, changed.to_string()).unwrap();
    assert!(agent_import::import(&ctx, &source, "OldReader", false).is_err());
    assert_eq!(
        agents::resolve(&ctx, "OldReader").unwrap().system_prompt,
        "Inspect carefully."
    );
    std::fs::write(&source, &raw).unwrap();
    agent_import::import(&ctx, &source, "OldReader", true).unwrap();
    assert_eq!(std::fs::read_to_string(source).unwrap(), raw);
}

#[test]
fn invalid_tools_schema_and_reserved_names_never_register() {
    let dir = tempfile::tempdir().unwrap();
    let ctx = context(dir.path());
    let source = dir.path().join("old.json");
    let mut unknown = fixture();
    unknown["tools"] = json!(["all"]);
    let mut extra = fixture();
    extra["permissions"] = json!("bypass");
    for raw in [unknown, extra, json!({"name":"incomplete"})] {
        std::fs::write(&source, raw.to_string()).unwrap();
        assert!(agent_import::import(&ctx, &source, "bad", false).is_err());
    }
    std::fs::write(&source, fixture().to_string()).unwrap();
    for id in ["guide", "GUIDE", "GENERAL-PURPOSE", "bad id"] {
        assert!(agent_import::import(&ctx, &source, id, false).is_err());
    }
    assert_eq!(agents::list(&ctx).unwrap().len(), 5);
    let mut empty = fixture();
    empty["tools"] = json!([]);
    std::fs::write(&source, empty.to_string()).unwrap();
    assert_eq!(
        agent_import::import(&ctx, &source, "empty", false)
            .unwrap()
            .definition
            .tools,
        Some(vec![])
    );
}

#[test]
fn concurrent_imports_do_not_lose_definitions() {
    let dir = tempfile::tempdir().unwrap();
    let ctx = context(dir.path());
    let source = dir.path().join("old.json");
    std::fs::write(&source, fixture().to_string()).unwrap();
    let barrier = Arc::new(std::sync::Barrier::new(2));
    let handles: Vec<_> = (0..2)
        .map(|i| {
            let ctx = context(dir.path());
            let source = source.clone();
            let barrier = barrier.clone();
            std::thread::spawn(move || {
                barrier.wait();
                agent_import::import(&ctx, &source, &format!("custom-{i}"), false).unwrap();
            })
        })
        .collect();
    for handle in handles {
        handle.join().unwrap();
    }
    assert!(agents::resolve(&ctx, "custom-0").is_ok());
    assert!(agents::resolve(&ctx, "custom-1").is_ok());
}
