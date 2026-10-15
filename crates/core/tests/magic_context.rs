use claude_core::{
    runtime::RuntimePaths,
    services::{magic_context, session_import, EnvironmentCredentials, ServiceContext},
};
use serde_json::json;
use std::sync::Arc;

#[test]
fn conversation_context_is_opt_in_scoped_revocable_and_bounded() {
    let dir = tempfile::tempdir().unwrap();
    let context = ServiceContext::open(
        RuntimePaths::new(dir.path().join("data")).unwrap(),
        Arc::new(EnvironmentCredentials),
    )
    .unwrap();
    let mut projects = Vec::new();
    for index in 0..2 {
        let root = dir.path().join(format!("project-{index}"));
        std::fs::create_dir(&root).unwrap();
        let source = dir.path().join(format!("session-{index}.json"));
        std::fs::write(&source,json!({"id":format!("session-{index}"),"name":"Notes","created_at":"2026-10-15T00:00:00Z","updated_at":"2026-10-15T00:00:00Z","messages":[{"role":"user","content":format!("PROJECT_{index}_SENTINEL"),"timestamp":"2026-10-15T00:00:00Z"}]}).to_string()).unwrap();
        let report = session_import::import(&context, &source, &root, false).unwrap();
        projects.push((root, report.project_id));
    }
    let (root, id) = &projects[0];
    assert!(magic_context::collect(&context, root, Some(id), vec![])
        .unwrap()
        .is_none());
    magic_context::configure(&context, root, id, true).unwrap();
    let content = magic_context::collect(&context, root, Some(id), vec![])
        .unwrap()
        .unwrap();
    assert!(content.contains("PROJECT_0_SENTINEL"));
    assert!(!content.contains("PROJECT_1_SENTINEL"));
    assert!(magic_context::configure(&context, root, &projects[1].1, true).is_err());
    assert!(
        magic_context::collect(&context, root, Some(&projects[1].1), vec![])
            .unwrap()
            .is_none()
    );
    magic_context::configure(&context, root, id, false).unwrap();
    assert!(magic_context::collect(&context, root, None, vec![])
        .unwrap()
        .is_none());
    let bounded = magic_context::collect(
        &context,
        root,
        None,
        vec!["\u{0001}".repeat(12000), "中文".repeat(10000)],
    )
    .unwrap();
    if let Some(content) = bounded {
        assert!(content.len() <= 32000);
        serde_json::from_str::<serde_json::Value>(&content).unwrap();
    }
}
