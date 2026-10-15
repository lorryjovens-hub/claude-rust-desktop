#[tokio::test]
async fn cancelling_a_tool_stops_its_shell_before_later_writes() {
    claude_core::config::OrchestrationConfig::init_global().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let cwd = dir.path().to_string_lossy().into_owned();
    let (shell, command) = if cfg!(windows) {
        ("powershell", "Set-Content started.txt ready; Start-Sleep -Milliseconds 1500; Set-Content finished.txt unexpected")
    } else {
        (
            "sh",
            "echo ready > started.txt; sleep 1.5; echo unexpected > finished.txt",
        )
    };
    let task = tokio::spawn(async move {
        claude_core::tools::execute_tool_async(
            "Bash",
            serde_json::json!({"shell":shell,"command":command,"timeout":10}),
            &cwd,
        )
        .await
    });
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        while !dir.path().join("started.txt").exists() {
            assert!(!task.is_finished(), "Shell exited before starting");
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
    task.abort();
    assert!(task.await.unwrap_err().is_cancelled());
    tokio::time::sleep(std::time::Duration::from_secs(2)).await;
    assert!(!dir.path().join("finished.txt").exists());
}
