#[tokio::test]
async fn legacy_skill_parser_cannot_execute_tools_without_shared_permissions() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("should-not-exist.txt");
    let markdown = format!(
        "---\nname: test\n---\nexecute Write({})",
        serde_json::json!({"file_path":file,"content":"unexpected"})
    );
    let context = claude_core::skills::SkillExecutionContext {
        workspace_path: Some(dir.path().to_string_lossy().into()),
        ..Default::default()
    };
    let error = claude_core::skills::SkillExecutionEngine::execute(&markdown, &context)
        .await
        .unwrap_err();
    assert!(error.to_string().contains("NativeEngine"));
    assert!(!file.exists());
}
