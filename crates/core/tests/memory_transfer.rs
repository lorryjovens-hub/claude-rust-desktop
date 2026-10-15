use claude_core::memory::{ContextManager, MemExClient};
use serde_json::json;
use std::sync::Arc;

#[tokio::test]
async fn legacy_preview_backup_roundtrip_recall_and_conflict_rollback() {
    let dir = tempfile::tempdir().unwrap();
    let client = Arc::new(MemExClient::with_path(
        dir.path().join("memory.sqlite3"),
        None,
    ));
    let legacy = json!([{"id":"old", "memory_type":"Knowledge", "content":"Project uses PostgreSQL", "importance":0.8, "timestamp":"2026-10-15T00:00:00Z", "tags":["architecture"], "metadata":{"nested":{"a":1}}, "embedding":[0.1]}]);
    let preview = client.import_archive(legacy.clone(), true).await.unwrap();
    assert_eq!(preview.imported, 1);
    assert!(preview.backup.is_none());
    assert_eq!(client.stats().await.unwrap().total_memories, 0);
    let report = client.import_archive(legacy.clone(), false).await.unwrap();
    let backup = MemExClient::with_path(report.backup.unwrap().into(), None);
    assert_eq!(backup.stats().await.unwrap().total_memories, 0);
    assert_eq!(
        client
            .import_archive(legacy.clone(), false)
            .await
            .unwrap()
            .skipped,
        1
    );
    let archive = client.export_archive().await.unwrap();
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(
            &archive.memories[0].metadata.as_ref().unwrap()["legacy_source"]
        )
        .unwrap(),
        legacy[0]
    );
    let target = MemExClient::with_path(dir.path().join("target.sqlite3"), None);
    let value = serde_json::to_value(archive).unwrap();
    target.import_archive(value.clone(), false).await.unwrap();
    assert_eq!(
        serde_json::to_value(target.export_archive().await.unwrap()).unwrap(),
        value
    );
    let mut conflicting = legacy.clone();
    conflicting[0]["id"] = json!("new");
    conflicting[0]["content"] = json!("A new entry");
    let mut conflict = legacy[0].clone();
    conflict["content"] = json!("Changed PostgreSQL");
    conflicting.as_array_mut().unwrap().push(conflict);
    assert!(client.import_archive(conflicting, false).await.is_err());
    assert_eq!(client.stats().await.unwrap().total_memories, 1);
    let context = ContextManager::new(client.clone());
    assert!(context
        .before_api_call("new-session", "PostgreSQL")
        .await
        .unwrap()
        .contains("Project uses PostgreSQL"));
    let mut config = client.get_config().await;
    config.backend_url = "http://127.0.0.1:1".into();
    client.update_config(config).await.unwrap();
    assert!(client.export_archive().await.is_err());
    assert!(client.import_archive(legacy, false).await.is_err());
}

#[tokio::test]
async fn export_is_complete_even_when_memory_disabled() {
    let dir = tempfile::tempdir().unwrap();
    let client = MemExClient::with_path(dir.path().join("memory.sqlite3"), None);
    for n in 0..105 {
        client
            .ingest(&format!("Memory {n}"), Some(0.8), None)
            .await
            .unwrap();
    }
    let mut config = client.get_config().await;
    config.enabled = false;
    client.update_config(config).await.unwrap();
    assert_eq!(client.export_archive().await.unwrap().memories.len(), 105);
}
