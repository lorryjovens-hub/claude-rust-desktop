use claude_core::mcp::McpServerManager;
use serde_json::json;

fn entry(name: &str) -> serde_json::Value {
    json!({"name":name,"command":"node","args":[],"env":{"TEST_SECRET":"fixture-only"},"cwd":null,"auto_start":false,"status":"Running","capabilities":["old-cache"]})
}

#[tokio::test]
async fn preview_backup_idempotence_conflict_and_restart() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("old.json");
    let path = dir.path().join("mcp.json");
    let original = serde_json::to_vec(&json!({"mcp_servers":[entry("old")]})).unwrap();
    std::fs::write(&source, &original).unwrap();
    let manager = McpServerManager::new(path.clone());
    assert_eq!(
        manager.import_legacy(&source, true).await.unwrap().imported,
        ["old"]
    );
    assert!(!path.exists());
    assert!(manager.list_servers().await.is_empty());
    manager.initialize().await.unwrap();
    let before = std::fs::read(&path).unwrap();
    let report = manager.import_legacy(&source, false).await.unwrap();
    assert_eq!(std::fs::read(report.backup.unwrap()).unwrap(), before);
    assert!(!serde_json::to_string(&report.imported)
        .unwrap()
        .contains("fixture-only"));
    let status = manager.get_server("old").await.unwrap();
    assert!(!status.running && !status.enabled);
    assert_eq!(
        manager.import_legacy(&source, false).await.unwrap().skipped,
        ["old"]
    );
    let saved = std::fs::read(&path).unwrap();
    let mut conflicting = entry("old");
    conflicting["command"] = json!("different");
    std::fs::write(
        &source,
        serde_json::to_vec(&json!({"mcp_servers":[entry("new"), conflicting]})).unwrap(),
    )
    .unwrap();
    assert!(manager.import_legacy(&source, false).await.is_err());
    assert_eq!(std::fs::read(&path).unwrap(), saved);
    assert!(manager.get_server("new").await.is_none());
    std::fs::write(&source, &original).unwrap();
    let reopened = McpServerManager::new(path);
    assert_eq!(
        reopened
            .import_legacy(&source, false)
            .await
            .unwrap()
            .skipped,
        ["old"]
    );
    assert_eq!(std::fs::read(&source).unwrap(), original);
    manager.shutdown_all().await.unwrap();
}

#[tokio::test]
async fn stale_host_cannot_overwrite_an_import() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("mcp.json");
    let source = dir.path().join("old.json");
    let first = McpServerManager::new(path.clone());
    first.initialize().await.unwrap();
    let second = McpServerManager::new(path.clone());
    second.initialize().await.unwrap();
    std::fs::write(&source, json!({"mcp_servers":[entry("first")]}).to_string()).unwrap();
    first.import_legacy(&source, false).await.unwrap();
    let saved = std::fs::read(&path).unwrap();
    std::fs::write(
        &source,
        json!({"mcp_servers":[entry("second")]}).to_string(),
    )
    .unwrap();
    assert!(second
        .import_legacy(&source, false)
        .await
        .unwrap_err()
        .to_string()
        .contains("another process"));
    assert!(second.get_server("second").await.is_none());
    assert_eq!(std::fs::read(&path).unwrap(), saved);
    assert!(tokio::time::timeout(
        std::time::Duration::from_secs(2),
        second.set_server_enabled("filesystem", true)
    )
    .await
    .unwrap()
    .is_err());
    assert!(!second.get_server("filesystem").await.unwrap().enabled);
    assert_eq!(std::fs::read(&path).unwrap(), saved);
    first.shutdown_all().await.unwrap();
    second.shutdown_all().await.unwrap();
}

#[tokio::test]
async fn enabled_edits_complete_and_persist_without_recursive_locking() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("mcp.json");
    let manager = McpServerManager::new(path.clone());
    manager.initialize().await.unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(2), async {
        manager
            .set_server_enabled("filesystem", true)
            .await
            .unwrap();
        assert!(manager.get_server("filesystem").await.unwrap().enabled);
        manager.toggle_server("filesystem").await.unwrap();
        assert!(!manager.get_server("filesystem").await.unwrap().enabled);
    })
    .await
    .unwrap();
    let saved: serde_json::Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    assert_eq!(saved["mcpServers"]["filesystem"]["enabled"], false);
    manager.shutdown_all().await.unwrap();
}

#[tokio::test]
async fn imported_working_directory_is_used_by_real_mcp_process() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("old.json");
    let mut config = entry("cwd-fixture");
    config["cwd"] = json!(dir.path());
    config["auto_start"] = json!(true);
    config["args"] = json!(["-e", "require('fs').writeFileSync('started.txt',process.env.TEST_SECRET);require('readline').createInterface({input:process.stdin}).on('line',line=>{const r=JSON.parse(line);console.log(JSON.stringify({jsonrpc:'2.0',id:r.id,result:r.method==='tools/list'?{tools:[]}:r.method==='resources/list'?{resources:[]}:{protocolVersion:'2024-11-05',capabilities:{},serverInfo:{name:'fixture',version:'1'}}}));});"]);
    std::fs::write(&source, json!({"mcp_servers":[config]}).to_string()).unwrap();
    let manager = McpServerManager::new(dir.path().join("mcp.json"));
    manager.import_legacy(&source, false).await.unwrap();
    assert!(!dir.path().join("started.txt").exists());
    manager.start_server("cwd-fixture").await.unwrap();
    assert_eq!(
        std::fs::read_to_string(dir.path().join("started.txt")).unwrap(),
        "fixture-only"
    );
    manager.stop_server("cwd-fixture").await.unwrap();
}

#[tokio::test]
async fn invalid_and_duplicate_definitions_leave_destination_untouched() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("old.json");
    let path = dir.path().join("mcp.json");
    let manager = McpServerManager::new(path.clone());
    let mut relative = entry("relative");
    relative["cwd"] = json!("relative/path");
    for entries in [
        json!([entry("same"), entry("same")]),
        json!([relative]),
        json!([{"name":"missing-fields"}]),
    ] {
        std::fs::write(&source, json!({"mcp_servers":entries}).to_string()).unwrap();
        assert!(manager.import_legacy(&source, false).await.is_err());
        assert!(!path.exists());
    }
}
