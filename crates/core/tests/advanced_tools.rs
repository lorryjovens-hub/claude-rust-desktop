use claude_core::{
    permissions::{
        AuditLogger, PermissionContext, PermissionManager, PermissionMode, PermissionResult,
    },
    tools::{execute_tool_async, get_tool_definitions},
};
use serde_json::json;

#[test]
fn advanced_tools_respect_read_edit_and_execution_modes() {
    let manager = PermissionManager::new(std::sync::Arc::new(AuditLogger::new(100)));
    let context = |name: &str| PermissionContext {
        tool_name: name.into(),
        tool_input: json!({}),
        conversation_id: "fixture".into(),
        user_id: None,
        workspace_path: None,
    };
    manager.set_mode(PermissionMode::PlanMode);
    manager.set_always_allow_checker(Box::new(|_, _| true));
    for name in [
        "NotebookEdit",
        "TaskCreate",
        "TaskUpdate",
        "TodoWrite",
        "LSP",
        "worktree",
    ] {
        assert!(
            matches!(
                manager.check_permission(&context(name)),
                PermissionResult::Denied(_)
            ),
            "{name}"
        );
    }
    for name in ["TaskGet", "TaskList", "TodoRead", "ToolSearch"] {
        assert_eq!(
            manager.check_permission(&context(name)),
            PermissionResult::Granted,
            "{name}"
        );
    }
    manager.set_always_allow_checker(Box::new(|_, _| false));
    manager.set_mode(PermissionMode::AcceptEdits);
    for name in ["NotebookEdit", "TaskCreate", "TaskUpdate", "TodoWrite"] {
        assert_eq!(
            manager.check_permission(&context(name)),
            PermissionResult::Granted,
            "{name}"
        );
    }
    for name in ["LSP", "worktree"] {
        assert!(
            matches!(
                manager.check_permission(&context(name)),
                PermissionResult::RequiresConfirmation(_)
            ),
            "{name}"
        );
    }
}

#[tokio::test]
async fn registered_tools_dispatch_and_project_reads_have_no_side_effects() {
    let dir = tempfile::tempdir().unwrap();
    let cwd = dir.path().to_str().unwrap();
    for name in ["TaskList", "TodoRead"] {
        execute_tool_async(name, json!({}), cwd).await.unwrap();
    }
    execute_tool_async("cron", json!({"operation":"list"}), cwd)
        .await
        .unwrap();
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
    let search = execute_tool_async("ToolSearch", json!({"query":"notebook"}), cwd)
        .await
        .unwrap();
    assert!(search["tools"]
        .as_array()
        .unwrap()
        .iter()
        .any(|t| t["name"] == "NotebookEdit"));
    let registered = get_tool_definitions();
    for name in [
        "NotebookEdit",
        "TaskCreate",
        "TaskUpdate",
        "TodoWrite",
        "LSP",
        "worktree",
    ] {
        assert_eq!(registered.iter().filter(|t| t.name == name).count(), 1);
    }
    execute_tool_async(
        "NotebookEdit",
        json!({"path":"fixture.ipynb","operation":"create"}),
        cwd,
    )
    .await
    .unwrap();
    let task = execute_tool_async(
        "TaskCreate",
        json!({"subject":"inspect","description":"inspect notebook"}),
        cwd,
    )
    .await
    .unwrap();
    let loaded = execute_tool_async("TaskGet", json!({"taskId":task["id"]}), cwd)
        .await
        .unwrap();
    assert_eq!(loaded, task);
}
