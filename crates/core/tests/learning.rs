use claude_core::{
    learning::{self, Experience},
    runtime::RuntimePaths,
    services::{EnvironmentCredentials, ServiceContext},
};
use std::sync::Arc;

#[tokio::test]
async fn learning_persists_deduplicates_and_feedback_changes_ranking() {
    let dir = tempfile::tempdir().unwrap();
    let open = || {
        ServiceContext::open(
            RuntimePaths::new(dir.path().into()).unwrap(),
            Arc::new(EnvironmentCredentials),
        )
        .unwrap()
    };
    let ctx = open();
    for n in 0..4 {
        let exp = Experience {
            id: format!("e{n}"),
            session_id: "s1".into(),
            context: "read code".into(),
            intent: "review".into(),
            tools_used: vec!["Read".into()],
            success: n != 3,
            duration_ms: 100,
            feedback: None,
        };
        learning::record(&ctx, &exp).unwrap();
        learning::record(&ctx, &exp).unwrap();
        let mut conflict = exp.clone();
        conflict.success = !exp.success;
        assert!(learning::record(&ctx, &conflict).is_err());
    }
    let before = learning::status(&ctx).unwrap();
    assert_eq!(before.experiences.len(), 4);
    assert_eq!(before.suggestions[0].occurrences, 4);
    assert_eq!(before.suggestions[0].success_rate, 0.75);
    assert_eq!(
        learning::status(&ctx).unwrap().suggestions,
        before.suggestions
    );
    learning::feedback(&ctx, "e0", 0.0).unwrap();
    assert!(learning::feedback(&ctx, "e0", f64::NAN).is_err());
    assert!(learning::feedback(&ctx, "missing", 1.0).is_err());
    learning::accept(&ctx, &before.suggestions[0].id).unwrap();
    let after = learning::status(&open()).unwrap();
    learning::accept(&ctx, &before.suggestions[0].id).unwrap();
    let manager = claude_core::skills::SkillsManager::with_path(dir.path().join("skills"));
    let installed = manager.list_enabled_skills().await.unwrap();
    let skill = installed
        .iter()
        .find(|s| s.name.starts_with("learned-"))
        .unwrap();
    assert_eq!(
        skill.content.as_deref(),
        Some(after.suggestions[0].skill_markdown.as_str())
    );
    let path = dir.path().join("skills").join(&skill.name).join("SKILL.md");
    std::fs::write(&path, "user edited content").unwrap();
    assert!(learning::accept(&ctx, &before.suggestions[0].id).is_err());
    assert_eq!(
        std::fs::read_to_string(path).unwrap(),
        "user edited content"
    );
    assert!(after.suggestions[0].accepted);
    assert_eq!(after.suggestions[0].score, 0.5);
    assert!(after.suggestions[0]
        .skill_markdown
        .contains("normal tool permission"));
}

#[tokio::test]
async fn cancelled_turn_is_recorded_without_prompt_or_tool_content() {
    let dir = tempfile::tempdir().unwrap();
    let ctx = ServiceContext::open(
        RuntimePaths::new(dir.path().into()).unwrap(),
        Arc::new(EnvironmentCredentials),
    )
    .unwrap();
    let observation = learning::TurnObservation::new(ctx.clone(), "cancelled-session".into());
    observation.tools.lock().unwrap().push("Read".into());
    let (started, received) = tokio::sync::oneshot::channel();
    let task = tokio::spawn(async move {
        let _observation = observation;
        started.send(()).unwrap();
        std::future::pending::<()>().await;
    });
    received.await.unwrap();
    task.abort();
    let _ = task.await;
    let state = learning::status(&ctx).unwrap();
    assert_eq!(state.experiences.len(), 1);
    assert!(!state.experiences[0].success);
    assert!(state.experiences[0].context.is_empty());
    assert_eq!(state.experiences[0].tools_used, ["Read"]);
}
