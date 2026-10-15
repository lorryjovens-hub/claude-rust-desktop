use claude_core::{
    runtime::RuntimePaths,
    services::{session_import, team, EnvironmentCredentials, ServiceContext},
};
use serde_json::json;
use std::sync::Arc;
#[test]
fn session_import_previews_backs_up_preserves_and_rolls_back_conflicts() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("project");
    std::fs::create_dir(&root).unwrap();
    let source = dir.path().join("sessions");
    std::fs::create_dir(&source).unwrap();
    let context = ServiceContext::open(
        RuntimePaths::new(dir.path().join("data")).unwrap(),
        Arc::new(EnvironmentCredentials),
    )
    .unwrap();
    let file = source.join("a.json");
    let raw=json!({"id":"old-session","name":"Prior work","created_at":"2025-01-01T00:00:00Z","updated_at":"2025-01-01T00:01:00Z","messages":[{"role":"user","content":"question","timestamp":"2025-01-01T00:00:00Z"},{"role":"assistant","content":"answer","timestamp":"2025-01-01T00:01:00Z"}],"custom_metadata":{"preserved":true}}).to_string();
    std::fs::write(&file, &raw).unwrap();
    assert_eq!(
        session_import::import(&context, &source, &root, true)
            .unwrap()
            .imported,
        1
    );
    assert!(context
        .db
        .with_conn(claude_core::db::conversation_repo::list_conversations)
        .unwrap()
        .unwrap()
        .is_empty());
    assert!(context
        .db
        .with_conn(claude_core::db::project_repo::list_projects)
        .unwrap()
        .unwrap()
        .is_empty());
    let report = session_import::import(&context, &source, &root, false).unwrap();
    assert!(std::path::Path::new(&report.backup.unwrap()).exists());
    let store = claude_core::runtime::SessionStore::new(context.db.clone());
    assert_eq!(
        store.history("old-session").unwrap()[1]["content"],
        "answer"
    );
    assert!(store.resume("old-session").is_ok());
    assert_eq!(
        session_import::import(&context, &source, &root, false)
            .unwrap()
            .skipped,
        1
    );
    std::fs::write(
        source.join("b.json"),
        raw.replace("old-session", "new-session"),
    )
    .unwrap();
    std::fs::write(source.join("z.json"), raw.replace("Prior work", "conflict")).unwrap();
    assert!(session_import::import(&context, &source, &root, false).is_err());
    assert!(store.resume("new-session").is_err());
    assert_eq!(std::fs::read_to_string(file).unwrap(), raw);
}
#[test]
fn team_model_context_is_project_and_team_scoped() {
    let dir = tempfile::tempdir().unwrap();
    let a = dir.path().join("a");
    let b = dir.path().join("b");
    std::fs::create_dir(&a).unwrap();
    std::fs::create_dir(&b).unwrap();
    let context = ServiceContext::open(
        RuntimePaths::new(dir.path().join("data")).unwrap(),
        Arc::new(EnvironmentCredentials),
    )
    .unwrap();
    let mut config = team::TeamConfig {
        team: "team-a".into(),
        endpoint: "http://127.0.0.1:1234".into(),
        credential: "SECRET_ENV_NAME".into(),
    };
    context.save_setting("team-sync", &config).unwrap();
    context.db.with_conn(|c|c.execute("INSERT INTO team_memories(team,id,title,content,tags,deleted,revision,dirty,mutation) VALUES('team-a','m','Shared','Team fact','[]',0,1,0,'x')",[])).unwrap().unwrap();
    assert!(team::model_context(&context, &a).unwrap().is_empty());
    team::configure_model_context(&context, &a, true).unwrap();
    let result = team::model_context(&context, &a).unwrap();
    assert!(result.contains("Team fact"));
    assert!(!result.contains("SECRET_ENV_NAME"));
    assert!(team::model_context(&context, &b).unwrap().is_empty());
    config.team = "team-b".into();
    context.save_setting("team-sync", &config).unwrap();
    assert!(team::model_context(&context, &a).unwrap().is_empty());
    config.team = "team-a".into();
    context.save_setting("team-sync", &config).unwrap();
    team::configure_model_context(&context, &a, false).unwrap();
    assert!(team::model_context(&context, &a).unwrap().is_empty());
}

#[test]
fn concurrent_hosts_initialize_and_write_shared_schema() {
    let dir = tempfile::tempdir().unwrap();
    let paths = RuntimePaths::new(dir.path().join("data")).unwrap();
    let barrier = Arc::new(std::sync::Barrier::new(6));
    let threads: Vec<_> = (0..6)
        .map(|i| {
            let paths = paths.clone();
            let barrier = barrier.clone();
            std::thread::spawn(move || {
                barrier.wait();
                let context =
                    ServiceContext::open(paths, Arc::new(EnvironmentCredentials)).unwrap();
                context.save_setting(&format!("host-{i}"), &i).unwrap();
            })
        })
        .collect();
    for thread in threads {
        thread.join().unwrap();
    }
    let context = ServiceContext::open(paths, Arc::new(EnvironmentCredentials)).unwrap();
    for i in 0..6 {
        assert_eq!(
            context.setting::<i32>(&format!("host-{i}")).unwrap(),
            Some(i)
        );
    }
}

#[test]
fn desktop_import_preserves_exact_selected_project() {
    use claude_core::db::project_repo;
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("project");
    std::fs::create_dir(&root).unwrap();
    let root = std::fs::canonicalize(root).unwrap();
    let context = ServiceContext::open(
        RuntimePaths::new(dir.path().join("data")).unwrap(),
        Arc::new(EnvironmentCredentials),
    )
    .unwrap();
    for id in ["first", "selected"] {
        context
            .db
            .with_conn(|c| -> anyhow::Result<()> {
                let path = root.to_string_lossy().into_owned();
                project_repo::insert_project(
                    c,
                    id,
                    id,
                    None,
                    None,
                    Some(&path),
                    false,
                    "2025-01-01T00:00:00Z",
                    "2025-01-01T00:00:00Z",
                )?;
                project_repo::set_folders(c, id, &[path.clone()])?;
                project_repo::trust_folders(c, id, &[path])?;
                Ok(())
            })
            .unwrap()
            .unwrap();
    }
    let file = dir.path().join("old.json");
    std::fs::write(&file,json!({"id":"exact","name":"Exact project","created_at":"2025-01-01T00:00:00Z","updated_at":"2025-01-01T00:00:00Z","messages":[]}).to_string()).unwrap();
    assert!(
        session_import::import_into_project(&context, &file, &root, true, Some("missing")).is_err()
    );
    let result =
        session_import::import_into_project(&context, &file, &root, false, Some("selected"))
            .unwrap();
    assert_eq!(result.project_id, "selected");
    assert_eq!(
        claude_core::runtime::SessionStore::new(context.db.clone())
            .resume("exact")
            .unwrap()
            .project_id
            .as_deref(),
        Some("selected")
    );
}
