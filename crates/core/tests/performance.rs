use claude_core::performance;

#[test]
fn persisted_tool_output_is_complete_and_unicode_preview_is_bounded() {
    let dir = tempfile::tempdir().unwrap();
    let text = "中文🙂".repeat(5000);
    let preview = performance::retain_tool_output(dir.path(), &text, &text);
    assert!(preview.len() < 17_000);
    assert!(preview.contains("Full tool output"));
    let files: Vec<_> = std::fs::read_dir(dir.path().join("tool-outputs"))
        .unwrap()
        .collect();
    assert_eq!(files.len(), 1);
    assert_eq!(
        std::fs::read_to_string(files[0].as_ref().unwrap().path()).unwrap(),
        text
    );
    let blocked = dir.path().join("file");
    std::fs::write(&blocked, "x").unwrap();
    assert_eq!(
        performance::retain_tool_output(&blocked, &text, "preview"),
        text
    );
}

#[tokio::test]
async fn actual_http_rate_limit_retries_and_auth_failure_does_not() {
    use axum::{response::IntoResponse, routing::get, Router};
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    };
    let count = Arc::new(AtomicUsize::new(0));
    let requests = count.clone();
    let app = Router::new()
        .route(
            "/limited",
            get(move || {
                let requests = requests.clone();
                async move {
                    if requests.fetch_add(1, Ordering::SeqCst) == 0 {
                        (
                            axum::http::StatusCode::TOO_MANY_REQUESTS,
                            [("retry-after", "0")],
                            "limited",
                        )
                            .into_response()
                    } else {
                        "ok".into_response()
                    }
                }
            }),
        )
        .route(
            "/auth",
            get(|| async { (axum::http::StatusCode::UNAUTHORIZED, "invalid token 5") }),
        );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    let client = reqwest::Client::new();
    let response = performance::retry_with_backoff(|| async {
        performance::checked_response(client.get(format!("http://{addr}/limited")).send().await?)
            .await
    })
    .await
    .unwrap();
    assert_eq!(response.text().await.unwrap(), "ok");
    assert_eq!(count.load(Ordering::SeqCst), 2);
    let error = performance::checked_response(
        client
            .get(format!("http://{addr}/auth"))
            .send()
            .await
            .unwrap(),
    )
    .await
    .unwrap_err();
    assert_eq!(
        performance::classify(&error),
        performance::ErrorClass::Authentication
    );
    server.abort();
}

#[test]
fn router_preserves_complex_tasks() {
    assert!(performance::simple_turn("Hello there"));
    for prompt in [
        "review code",
        "删除文件",
        "https://example.com",
        "hi\nthere",
        "debug model 5",
    ] {
        assert!(!performance::simple_turn(prompt));
    }
}

#[tokio::test]
async fn cancelling_retry_delay_prevents_another_request() {
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    };
    let count = Arc::new(AtomicUsize::new(0));
    let attempts = count.clone();
    let (sent, started) = tokio::sync::oneshot::channel();
    let task = tokio::spawn(async move {
        let mut sent = Some(sent);
        performance::retry_with_backoff(|| {
            attempts.fetch_add(1, Ordering::SeqCst);
            if let Some(sent) = sent.take() {
                let _ = sent.send(());
            }
            async {
                Err::<(), _>(
                    performance::ApiError {
                        status: 429,
                        retry_after: Some(std::time::Duration::from_secs(300)),
                        message: "limited".into(),
                    }
                    .into(),
                )
            }
        })
        .await
    });
    started.await.unwrap();
    task.abort();
    assert!(task.await.unwrap_err().is_cancelled());
    assert_eq!(count.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn shared_engine_routes_opt_in_and_preserves_explicit_provider() {
    use claude_core::{
        native_engine::{
            engine_core::ChatRequest,
            provider_manager::{Provider, ProviderManager},
        },
        runtime::{self, RuntimePaths, SessionStore},
        services::{EnvironmentCredentials, ServiceContext},
    };
    use serde_json::json;
    use std::sync::Arc;
    claude_core::config::OrchestrationConfig::init_global().unwrap();
    let seen = Arc::new(std::sync::Mutex::new(Vec::<serde_json::Value>::new()));
    let captured = seen.clone();
    let app = axum::Router::new().route("/v1/chat/completions", axum::routing::post(move |axum::Json(body): axum::Json<serde_json::Value>| { let captured = captured.clone(); async move {
        captured.lock().unwrap().push(body.clone());
        ([ ("content-type", "text/event-stream") ], "data: {\"id\":\"r\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"hello\"},\"finish_reason\":null}]}\n\ndata: {\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"stop\"}]}\n\ndata: [DONE]\n\n")
    }}));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    let dir = tempfile::tempdir().unwrap();
    let paths = RuntimePaths::new(dir.path().into()).unwrap();
    let ctx = ServiceContext::open(paths.clone(), Arc::new(EnvironmentCredentials)).unwrap();
    let provider: Provider = serde_json::from_value(json!({"id":"local","name":"Local","base_url":format!("http://{addr}"),"api_key":"fixture","api_format":"openai","enabled":true,"models":[{"id":"primary","name":"Primary","enabled":true,"max_tokens":100,"supports_vision":false,"supports_web_search":false},{"id":"cheap","name":"Cheap","enabled":true,"max_tokens":100,"supports_vision":false,"supports_web_search":false}]})).unwrap();
    std::fs::write(
        dir.path().join("providers.json"),
        serde_json::to_vec(&vec![provider.clone()]).unwrap(),
    )
    .unwrap();
    performance::configure_router(
        &ctx,
        &performance::RouterConfig {
            enabled: true,
            provider_id: "local".into(),
            model_id: "cheap".into(),
        },
    )
    .unwrap();
    ctx.save_setting("learning.enabled", &true).unwrap();
    let manager = Arc::new(tokio::sync::Mutex::new(ProviderManager::in_memory(
        vec![provider],
        dir.path().join("providers.json"),
    )));
    let skills = claude_core::skills::SkillsManager::with_path(dir.path().join("skills"));
    skills
        .create_skill(
            "Test Skill",
            "fixture",
            "---\nname: Test Skill\ndescription: fixture\n---\nUnique skill marker 7931",
        )
        .unwrap();
    let engine = runtime::create_engine(
        &paths,
        ctx.db.clone(),
        manager,
        runtime::default_permissions(),
        Arc::new(claude_core::cache::FileCache::new()),
    )
    .with_skills_manager(Arc::new(tokio::sync::Mutex::new(skills)));
    let sessions = SessionStore::new(ctx.db.clone());
    for (provider_id, prompt) in [
        (None, "Hello"),
        (Some("local".to_owned()), "Hello"),
        (Some("local".to_owned()), "/test-skill inspect this"),
    ] {
        let conversation_id = sessions.create(dir.path(), "primary", "local").unwrap();
        let mut events = engine
            .send_message(ChatRequest {
                conversation_id,
                messages: vec![json!({"role":"user","content":prompt})],
                model: "primary".into(),
                provider_id,
                effort: None,
                system_prompt: None,
                max_tokens: Some(100),
                workspace_path: Some(dir.path().to_string_lossy().into()),
                temperature: None,
                top_p: None,
                reasoning_mode: None,
            })
            .await
            .unwrap();
        tokio::time::timeout(std::time::Duration::from_secs(10), async {
            while events.recv().await.is_some() {}
        })
        .await
        .unwrap();
    }
    let requests = seen.lock().unwrap();
    assert_eq!(
        requests
            .iter()
            .map(|r| r["model"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["cheap", "primary", "primary"]
    );
    assert!(requests[2]["messages"][0]["content"]
        .as_str()
        .unwrap()
        .contains("Unique skill marker 7931"));
    let observations = claude_core::learning::status(&ctx).unwrap();
    assert_eq!(observations.experiences.len(), 3);
    assert!(observations
        .experiences
        .iter()
        .all(|e| e.success && e.context.is_empty()));
    server.abort();
}
