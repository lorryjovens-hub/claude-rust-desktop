use claude_core::{
    runtime::RuntimePaths,
    services::{plugins, EnvironmentCredentials, ServiceContext},
};
use serde_json::json;
use std::{sync::Arc, time::Duration};
#[tokio::test]
async fn installation_execution_upgrade_and_revocation() {
    let dir = tempfile::tempdir().unwrap();
    let context = ServiceContext::open(
        RuntimePaths::new(dir.path().join("data")).unwrap(),
        Arc::new(EnvironmentCredentials),
    )
    .unwrap();
    let source = dir.path().join("plugin.json");
    let mut bundle = json!({"id":"echo","version":"1.0.0","description":"fixture","system_prompt":"Use echo for checks","commands":{"echo":{"description":"echo","interpreter":"node","script":"echo.mjs"},"wait":{"description":"wait","interpreter":"node","script":"wait.mjs"}},"files":{"echo.mjs":"let s='';for await(const c of process.stdin)s+=c;console.log(JSON.stringify({input:JSON.parse(s),cwd:process.cwd()}));","wait.mjs":"setTimeout(()=>{require('fs').writeFileSync('late.txt','bad')},2000)"}});
    // ESM script uses a valid long-lived process, with a delayed write we must prevent.
    bundle["files"]["wait.mjs"] =
        json!("import fs from 'node:fs';setTimeout(()=>fs.writeFileSync('late.txt','bad'),2000)");
    std::fs::write(&source, bundle.to_string()).unwrap();
    let source = source.to_str().unwrap();
    plugins::install(&context, source, None).await.unwrap();
    assert!(plugins::install(&context, source, None).await.is_err());
    let result = plugins::run(&context, "echo", "echo", json!({"test":42}), dir.path())
        .await
        .unwrap();
    let output: serde_json::Value =
        serde_json::from_str(result["stdout"].as_str().unwrap().trim()).unwrap();
    assert_eq!(output["input"]["test"], 42);
    assert!(plugins::prompt_hooks(&context)
        .unwrap()
        .contains("Use echo"));
    let stop = async {
        tokio::time::sleep(Duration::from_millis(350)).await;
        plugins::enable(&context, "echo", false).unwrap();
    };
    let (result, ()) = tokio::join!(
        plugins::run(&context, "echo", "wait", json!(null), dir.path()),
        stop
    );
    assert!(result.unwrap_err().to_string().contains("disabled"));
    tokio::time::sleep(Duration::from_secs(2)).await;
    assert!(!dir.path().join("late.txt").exists());
    assert!(plugins::prompt_hooks(&context).unwrap().is_empty());
    assert!(
        plugins::run(&context, "echo", "echo", json!(null), dir.path())
            .await
            .is_err()
    );
    assert!(plugins::install(&context, source, Some("echo"))
        .await
        .is_err());
    bundle["version"] = json!("1.1.0");
    std::fs::write(source, bundle.to_string()).unwrap();
    let updated = plugins::install(&context, source, Some("echo"))
        .await
        .unwrap();
    assert!(!updated.enabled);
    plugins::remove(&context, "echo").unwrap();
    assert!(plugins::list(&context).unwrap().is_empty());
    assert!(std::path::Path::new(source).exists());
    assert_eq!(
        std::fs::read_dir(context.paths.data_dir.join("plugin-runs"))
            .unwrap()
            .count(),
        0
    );
}
#[test]
fn bundle_paths_and_permissions_are_validated() {
    let bundle:plugins::Bundle=serde_json::from_value(json!({"id":"x","version":"1.0.0","description":"","commands":{},"files":{"../escape":"bad"}})).unwrap();
    assert!(bundle.validate().is_err());
    let permissions = claude_core::runtime::default_permissions();
    let context = claude_core::permissions::PermissionContext {
        tool_name: "PluginRun".into(),
        tool_input: json!({"plugin":"x","command":"run"}),
        conversation_id: "test".into(),
        user_id: None,
        workspace_path: None,
    };
    permissions.set_mode(claude_core::permissions::PermissionMode::PlanMode);
    assert!(matches!(
        permissions.check_permission(&context),
        claude_core::permissions::PermissionResult::Denied(_)
    ));
    permissions.set_mode(claude_core::permissions::PermissionMode::AcceptEdits);
    assert!(matches!(
        permissions.check_permission(&context),
        claude_core::permissions::PermissionResult::RequiresConfirmation(_)
    ));
}
