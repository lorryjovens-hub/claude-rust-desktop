use axum::{extract::State, routing::post, Json, Router};
use claude_core::{
    memory::ContextManager,
    native_engine::provider_manager::Provider,
    runtime::RuntimePaths,
    services::{
        dream::{self, DreamConfig},
        generation::Generator,
        lifecycle, EnvironmentCredentials, ServiceContext,
    },
};
use serde_json::{json, Value};
use std::{
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    time::Duration,
};

#[derive(Clone)]
struct Fixture {
    calls: Arc<AtomicUsize>,
    release: Arc<tokio::sync::Semaphore>,
    finish: &'static str,
}
async fn respond(State(f): State<Fixture>, Json(body): Json<Value>) -> Json<Value> {
    assert!(body.get("tools").is_none());
    assert!(body["messages"][1]["content"]
        .as_str()
        .unwrap()
        .contains("PostgreSQL"));
    f.calls.fetch_add(1, Ordering::SeqCst);
    let _permit = f.release.acquire().await.unwrap();
    Json(
        json!({"id":"r", "object":"chat.completion", "created":0, "model":"m", "choices":[{"index":0,"message":{"role":"assistant","content":"Remember PostgreSQL architecture and Chinese replies."},"finish_reason":f.finish}]}),
    )
}
async fn fixture(
    finish: &'static str,
    permits: usize,
) -> (
    tempfile::TempDir,
    ServiceContext,
    Generator,
    Fixture,
    tokio::task::JoinHandle<()>,
) {
    let dir = tempfile::tempdir().unwrap();
    let paths = RuntimePaths::new(dir.path().to_path_buf()).unwrap();
    let context = ServiceContext::open(paths.clone(), Arc::new(EnvironmentCredentials)).unwrap();
    let f = Fixture {
        calls: Arc::new(AtomicUsize::new(0)),
        release: Arc::new(tokio::sync::Semaphore::new(permits)),
        finish,
    };
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let provider: Provider = serde_json::from_value(json!({"id":"p","name":"p","base_url":format!("http://{}",listener.local_addr().unwrap()),"api_key":"fixture","api_format":"openai","enabled":true,"models":[{"id":"m","name":"m","enabled":true,"supports_vision":false,"supports_web_search":false}]})).unwrap();
    std::fs::write(
        paths.data_dir.join("providers.json"),
        serde_json::to_vec(&vec![provider.clone()]).unwrap(),
    )
    .unwrap();
    let app = Router::new()
        .route("/v1/chat/completions", post(respond))
        .with_state(f.clone());
    let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let generator = Generator::new(&paths, vec![provider], "p", "m").unwrap();
    dream::client(&context)
        .ingest("PostgreSQL architecture", Some(0.8), None)
        .await
        .unwrap();
    (dir, context, generator, f, server)
}
async fn wait_calls(f: &Fixture) {
    tokio::time::timeout(Duration::from_secs(5), async {
        while f.calls.load(Ordering::SeqCst) == 0 {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn consolidation_is_durable_recalled_once_and_respects_gates() {
    let (_dir, context, generator, f, server) = fixture("stop", 1).await;
    let config = DreamConfig {
        enabled: true,
        provider: "p".into(),
        model: "m".into(),
        min_hours: 24,
        min_memories: 2,
    };
    context.save_setting("auto-dream", &config).unwrap();
    assert_eq!(
        dream::run(&context, &generator, true)
            .await
            .unwrap()
            .consolidated,
        0
    );
    assert_eq!(f.calls.load(Ordering::SeqCst), 0);
    assert_eq!(
        dream::run(&context, &generator, false)
            .await
            .unwrap()
            .consolidated,
        1
    );
    assert_eq!(
        dream::run(&context, &generator, false)
            .await
            .unwrap()
            .consolidated,
        0
    );
    let reopened = Arc::new(dream::client(&context));
    assert_eq!(reopened.stats().await.unwrap().total_memories, 2);
    assert_eq!(reopened.dream_status().await.unwrap().runs, 1);
    let manager = ContextManager::new(reopened.clone());
    assert!(manager
        .before_api_call("next", "Chinese replies")
        .await
        .unwrap()
        .contains("Remember PostgreSQL"));
    reopened
        .ingest("New PostgreSQL configuration", Some(0.8), None)
        .await
        .unwrap();
    context
        .save_setting(
            "auto-dream",
            &DreamConfig {
                min_memories: 1,
                ..config
            },
        )
        .unwrap();
    assert_eq!(
        dream::run(&context, &generator, true)
            .await
            .unwrap()
            .skipped
            .as_deref(),
        Some("time gate")
    );
    assert_eq!(f.calls.load(Ordering::SeqCst), 1);
    reopened.clear().await.unwrap();
    assert_eq!(reopened.dream_status().await.unwrap().runs, 0);
    server.abort();
}

#[tokio::test]
async fn stopped_request_and_concurrent_clear_never_write_late_summary() {
    let (_dir, context, generator, f, server) = fixture("stop", 0).await;
    let clone = context.clone();
    let task = tokio::spawn(async move { dream::run(&clone, &generator, false).await });
    wait_calls(&f).await;
    assert!(lifecycle::Lease::acquire(&context, "memory-dream").is_err());
    lifecycle::stop(&context, "memory-dream").unwrap();
    assert!(tokio::time::timeout(Duration::from_secs(3), task)
        .await
        .unwrap()
        .unwrap()
        .is_err());
    f.release.add_permits(1);
    assert_eq!(
        dream::client(&context)
            .stats()
            .await
            .unwrap()
            .total_memories,
        1
    );
    server.abort();

    let (_dir, context, generator, f, server) = fixture("stop", 0).await;
    let clone = context.clone();
    let task = tokio::spawn(async move { dream::run(&clone, &generator, false).await });
    wait_calls(&f).await;
    dream::client(&context).clear().await.unwrap();
    f.release.add_permits(1);
    assert!(task.await.unwrap().is_err());
    assert_eq!(
        dream::client(&context)
            .stats()
            .await
            .unwrap()
            .total_memories,
        0
    );
    server.abort();
}

#[tokio::test]
async fn incomplete_generation_preserves_sources_and_reports_failure() {
    let (_dir, context, generator, _f, server) = fixture("length", 1).await;
    assert!(dream::run(&context, &generator, false).await.is_err());
    assert_eq!(
        dream::client(&context)
            .dream_status()
            .await
            .unwrap()
            .pending,
        1
    );
    assert!(lifecycle::status(&context).unwrap()[0].error.is_some());
    server.abort();
}
