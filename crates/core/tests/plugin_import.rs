use claude_core::{
    runtime::RuntimePaths,
    services::{plugins, EnvironmentCredentials, ServiceContext},
};
use serde_json::json;
use std::sync::Arc;

#[tokio::test]
async fn legacy_preview_backup_execution_idempotence_conflict_and_disable() {
    let temp = tempfile::tempdir().unwrap();
    let context = ServiceContext::open(
        RuntimePaths::new(temp.path().join("data")).unwrap(),
        Arc::new(EnvironmentCredentials),
    )
    .unwrap();
    let source = temp.path().join("legacy");
    std::fs::create_dir(&source).unwrap();
    let manifest = json!({"name":"legacy-echo","version":"1.0.0","description":"old package","author":"author","license":null,"repository":null,"main":"main.cjs","commands":[{"name":"echo","description":"echo","usage":"echo","examples":[]}],"hooks":[],"dependencies":{},"permissions":[],"enabled":true});
    std::fs::write(source.join("plugin.json"), manifest.to_string()).unwrap();
    std::fs::write(
        source.join("main.cjs"),
        "console.log(JSON.stringify(process.argv.slice(2)))",
    )
    .unwrap();
    let preview = plugins::import_legacy(&context, &source, true).unwrap();
    assert!(preview.preview && preview.backup.is_none());
    assert!(plugins::list(&context).unwrap().is_empty());
    assert!(!context.paths.data_dir.join("backups").exists());
    let imported = plugins::import_legacy(&context, &source, false).unwrap();
    assert!(imported.imported);
    assert!(std::path::Path::new(&imported.backup.unwrap()).is_file());
    let output = plugins::run(
        &context,
        "legacy-echo",
        "echo",
        json!({"message":"hello 世界"}),
        temp.path(),
    )
    .await
    .unwrap();
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(output["stdout"].as_str().unwrap()).unwrap(),
        json!(["--message=hello 世界"])
    );
    assert!(plugins::run(
        &context,
        "legacy-echo",
        "echo",
        json!({"message":42}),
        temp.path()
    )
    .await
    .is_err());
    plugins::enable(&context, "legacy-echo", false).unwrap();
    let repeated = plugins::import_legacy(&context, &source, false).unwrap();
    assert!(repeated.skipped);
    assert!(!repeated.enabled);
    assert!(!plugins::list(&context).unwrap()[0].enabled);
    std::fs::write(source.join("main.cjs"), "console.log('changed')").unwrap();
    assert!(plugins::import_legacy(&context, &source, false).is_err());
    assert_eq!(plugins::list(&context).unwrap().len(), 1);
    assert_eq!(
        std::fs::read_to_string(source.join("plugin.json")).unwrap(),
        manifest.to_string()
    );
}

#[test]
fn unsupported_legacy_contracts_fail_without_installing() {
    let temp = tempfile::tempdir().unwrap();
    let context = ServiceContext::open(
        RuntimePaths::new(temp.path().join("data")).unwrap(),
        Arc::new(EnvironmentCredentials),
    )
    .unwrap();
    let source = temp.path().join("legacy");
    std::fs::create_dir(&source).unwrap();
    let base = json!({"name":"x","version":"1.0.0","description":null,"main":"main.py","commands":[{"name":"run","description":"","usage":null,"examples":[]}],"hooks":[],"dependencies":{},"permissions":[],"enabled":false});
    std::fs::write(source.join("main.py"), "print('hello')").unwrap();
    for (key, value) in [
        ("hooks", json!(["on_plugin_load"])),
        ("dependencies", json!({"other":"1"})),
        ("permissions", json!(["read"])),
        ("main", json!("../escape.py")),
        ("main", json!("main.wasm")),
        ("unknown", json!(true)),
    ] {
        let mut manifest = base.clone();
        manifest[key] = value;
        std::fs::write(source.join("plugin.json"), manifest.to_string()).unwrap();
        assert!(
            plugins::import_legacy(&context, &source, false).is_err(),
            "{key}"
        );
        assert!(plugins::list(&context).unwrap().is_empty());
    }
    assert!(!context.paths.data_dir.join("backups").exists());
}
