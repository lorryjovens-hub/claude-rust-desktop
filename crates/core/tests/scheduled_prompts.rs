use axum::{routing::post, Json, Router};
use claude_core::{
    runtime::RuntimePaths,
    services::{lifecycle, scheduled_prompts, EnvironmentCredentials, ServiceContext},
    tools::scheduled,
};
use serde_json::{json, Value};
use std::{
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    time::Duration,
};

#[tokio::test]
async fn host_generates_real_response_prevents_duplicates_and_cancels_inflight_on_stop() {
    let dir = tempfile::tempdir().unwrap();
    let project = dir.path().join("project");
    std::fs::create_dir(&project).unwrap();
    let cwd = project.to_string_lossy().into_owned();
    let paths = RuntimePaths::new(dir.path().join("data")).unwrap();
    let context = ServiceContext::open(paths.clone(), Arc::new(EnvironmentCredentials)).unwrap();
    let calls = Arc::new(AtomicUsize::new(0));
    let fixture_calls = calls.clone();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    std::fs::write(paths.data_dir.join("providers.json"),json!([{"id":"p","name":"p","base_url":format!("http://{}",listener.local_addr().unwrap()),"api_key":"fixture","api_format":"openai","enabled":true,"models":[{"id":"m","name":"m","enabled":true,"supports_vision":false,"supports_web_search":false}]}]).to_string()).unwrap();
    let app=Router::new().route("/v1/chat/completions",post(move |Json(body):Json<Value>|{let calls=fixture_calls.clone();async move{
        assert!(body.get("tools").is_none());
        if calls.fetch_add(1,Ordering::SeqCst)>0{std::future::pending::<()>().await;}
        Json(json!({"id":"fixture","object":"chat.completion","created":0,"model":"m","choices":[{"index":0,"message":{"role":"assistant","content":"scheduled result"},"finish_reason":"stop"}]}))
    }}));
    let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let record = scheduled::execute(
        json!({"operation":"create","schedule":"* * * * *","prompt":"Produce a summary"}),
        &cwd,
    )
    .unwrap();
    let id = record["cron_id"].as_str().unwrap();
    let force_due = || {
        rusqlite::Connection::open(project.join(".claude/schedules.sqlite3"))
            .unwrap()
            .execute("UPDATE schedules SET next_run=0", [])
            .unwrap();
    };
    force_due();
    let host = tokio::spawn(scheduled_prompts::run(
        context.clone(),
        cwd.clone(),
        "p".into(),
        "m".into(),
    ));
    tokio::time::timeout(Duration::from_secs(8), async {
        loop {
            if scheduled::execute(json!({"operation":"get","cron_id":id}), &cwd).unwrap()["state"]
                == "completed"
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
    assert_eq!(
        scheduled::execute(json!({"operation":"get","cron_id":id}), &cwd).unwrap()["output"],
        "scheduled result"
    );
    assert!(
        scheduled_prompts::run(context.clone(), cwd.clone(), "p".into(), "m".into())
            .await
            .is_err()
    );
    force_due();
    tokio::time::timeout(Duration::from_secs(5), async {
        while calls.load(Ordering::SeqCst) < 2 {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
    lifecycle::stop(&context, "cron").unwrap();
    tokio::time::timeout(Duration::from_secs(3), host)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    lifecycle::wait_stopped(&context, "cron").await.unwrap();
    assert_eq!(
        scheduled::execute(json!({"operation":"get","cron_id":id}), &cwd).unwrap()["state"],
        "failed"
    );
    assert!(
        scheduled::execute(json!({"operation":"get","cron_id":id}), &cwd).unwrap()["error"]
            .as_str()
            .unwrap()
            .contains("cancelled")
    );
    server.abort();
}
