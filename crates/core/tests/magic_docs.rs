use axum::{routing::post, Json, Router};
use claude_core::{
    native_engine::provider_manager::Provider,
    permissions::PermissionMode,
    runtime::{self, RuntimePaths},
    services::{generation::Generator, magic_docs as docs, EnvironmentCredentials, ServiceContext},
};
use serde_json::{json, Value};
use std::sync::Arc;

#[tokio::test]
async fn previews_are_persistent_scoped_and_apply_checks_permissions_and_changes() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("project");
    std::fs::create_dir(&root).unwrap();
    let paths = RuntimePaths::new(dir.path().join("data")).unwrap();
    let context = ServiceContext::open(paths.clone(), Arc::new(EnvironmentCredentials)).unwrap();
    let file = root.join("notes.md");
    let original = "# MAGIC DOC: Architecture\n\nOriginal facts.\n";
    std::fs::write(&file, original).unwrap();
    let outside = dir.path().join("outside.md");
    std::fs::write(&outside, original).unwrap();
    assert!(docs::check(&context, &root, &outside).is_err());
    let document = docs::check(&context, &root, &file).unwrap();
    assert_eq!(docs::check(&context, &root, &file).unwrap().id, document.id);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let provider: Provider = serde_json::from_value(json!({"id":"p","name":"p","base_url":format!("http://{}",listener.local_addr().unwrap()),"api_key":"fixture","api_format":"openai","enabled":true,"models":[{"id":"m","name":"m","enabled":true,"supports_vision":false,"supports_web_search":false}]})).unwrap();
    let app = Router::new().route("/v1/chat/completions",post(|Json(body):Json<Value>|async move {
        assert!(body.get("tools").is_none());
        Json(json!({"id":"r","object":"chat.completion","created":0,"model":"m","choices":[{"index":0,"message":{"role":"assistant","content":"# MAGIC DOC: Architecture\n\nOriginal facts. New facts.\n"},"finish_reason":"stop"}]}))
    }));
    let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let generator = Generator::new(&paths, vec![provider], "p", "m").unwrap();
    let preview = docs::update(&context, &root, &document.id, "New facts.", &generator)
        .await
        .unwrap();
    assert!(preview.diff.contains("-Original facts."));
    assert!(preview.diff.contains("+Original facts. New facts."));
    assert_eq!(std::fs::read_to_string(&file).unwrap(), original);
    let reopened = ServiceContext::open(paths.clone(), Arc::new(EnvironmentCredentials)).unwrap();
    assert_eq!(docs::previews(&reopened, &root).unwrap()[0].id, preview.id);
    let permissions = runtime::default_permissions();
    permissions.set_mode(PermissionMode::PlanMode);
    assert!(docs::apply(&context, &root, &preview.id, &permissions).is_err());
    permissions.set_mode(PermissionMode::AcceptEdits);
    std::fs::write(&file, "Changed by user").unwrap();
    assert!(docs::apply(&context, &root, &preview.id, &permissions).is_err());
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "Changed by user");
    std::fs::write(&file, original).unwrap();
    docs::apply(&context, &root, &preview.id, &permissions).unwrap();
    assert!(std::fs::read_to_string(&file)
        .unwrap()
        .contains("New facts."));
    assert_eq!(docs::list(&context, &root).unwrap()[0].update_count, 1);
    assert!(docs::apply(&context, &root, &preview.id, &permissions).is_err());
    docs::set_auto(&context, &root, &document.id, true).unwrap();
    assert!(docs::list(&reopened, &root).unwrap()[0].auto_update);
    assert_eq!(docs::clear(&context, &root).unwrap(), 1);
    assert!(file.exists());
    assert!(docs::previews(&context, &root).unwrap().is_empty());
    assert_eq!(std::fs::read_to_string(outside).unwrap(), original);
    server.abort();
}

#[test]
fn legacy_document_import_is_previewed_scoped_and_idempotent() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("project");
    std::fs::create_dir(&root).unwrap();
    let context = ServiceContext::open(
        RuntimePaths::new(dir.path().join("data")).unwrap(),
        Arc::new(EnvironmentCredentials),
    )
    .unwrap();
    let file = root.join("notes.md");
    let text = "#MAGIC  DOC: Architecture\nOriginal text";
    std::fs::write(&file, text).unwrap();
    let source = dir.path().join("magic_docs_state.json");
    std::fs::write(&source,json!([{"path":file,"title":"Architecture","instructions":"retain","last_updated":"2025-01-01T00:00:00Z","update_count":3}]).to_string()).unwrap();
    assert_eq!(
        docs::import_legacy(&context, &root, &source, true)
            .unwrap()
            .imported,
        1
    );
    assert!(docs::list(&context, &root).unwrap().is_empty());
    let report = docs::import_legacy(&context, &root, &source, false).unwrap();
    assert!(std::path::Path::new(&report.backup.unwrap()).exists());
    assert_eq!(
        docs::import_legacy(&context, &root, &source, false)
            .unwrap()
            .skipped,
        1
    );
    let documents = docs::list(&context, &root).unwrap();
    assert!(!documents[0].auto_update);
    assert_eq!(documents[0].update_count, 3);
    assert_eq!(std::fs::read_to_string(file).unwrap(), text);
}
