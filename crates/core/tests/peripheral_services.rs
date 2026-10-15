use anyhow::{Context, Result};
use axum::{
    extract::{Multipart, State},
    http::{HeaderMap, StatusCode},
    routing::post,
    Json, Router,
};
use claude_core::{
    runtime::RuntimePaths,
    services::{
        self,
        lifecycle::{self, Lease},
        team::{self, TeamConfig},
        team_server::TeamServer,
        voice::{self, VoiceConfig},
        Credentials, ServiceContext,
    },
};
use std::{collections::HashMap, sync::Arc, time::Duration};

struct Secrets(HashMap<String, String>);
impl Credentials for Secrets {
    fn get(&self, name: &str) -> Result<String> {
        self.0.get(name).cloned().context("No credential")
    }
}
fn context(path: &std::path::Path, token: &str) -> ServiceContext {
    ServiceContext::open(
        RuntimePaths::new(path.to_path_buf()).unwrap(),
        Arc::new(Secrets(HashMap::from([("token".into(), token.into())]))),
    )
    .unwrap()
}
async fn server(path: &std::path::Path) -> (String, tokio::task::JoinHandle<()>) {
    let server = TeamServer::new(
        path.join("server.sqlite3"),
        HashMap::from([
            ("test-alpha-token-long".into(), "alpha".into()),
            ("test-beta-token-long".into(), "beta".into()),
        ]),
    )
    .unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let task = tokio::spawn(async move {
        axum::serve(listener, server.router()).await.unwrap();
    });
    (format!("http://{addr}"), task)
}
async fn auth(c: &ServiceContext, url: &str, team: &str) -> Result<()> {
    team::auth(
        c,
        TeamConfig {
            endpoint: url.into(),
            team: team.into(),
            credential: "token".into(),
        },
    )
    .await
}

#[tokio::test]
async fn independent_clients_auth_crud_conflicts_tombstones_and_restart() {
    let dir = tempfile::tempdir().unwrap();
    let (url, server) = server(dir.path()).await;
    let a = context(&dir.path().join("a"), "test-alpha-token-long");
    let b = context(&dir.path().join("b"), "test-alpha-token-long");
    let beta = context(&dir.path().join("beta"), "test-beta-token-long");
    let bad = context(&dir.path().join("bad"), "invalid-token");
    assert!(auth(&bad, &url, "alpha").await.is_err());
    assert!(bad.setting::<TeamConfig>("team-sync").unwrap().is_none());
    assert!(auth(&beta, &url, "alpha").await.is_err());
    auth(&beta, &url, "beta").await.unwrap();
    auth(&a, &url, "alpha").await.unwrap();
    auth(&b, &url, "alpha").await.unwrap();
    let created = team::put(
        &a,
        None,
        "Architecture".into(),
        "Use Rust".into(),
        vec!["decision".into()],
    )
    .unwrap();
    team::sync(&a).await.unwrap();
    team::sync(&b).await.unwrap();
    team::sync(&beta).await.unwrap();
    assert_eq!(team::list(&b, false).unwrap()[0].content, "Use Rust");
    assert!(team::list(&beta, false).unwrap().is_empty());
    assert!(team::sync(&a).await.unwrap().accepted.is_empty());
    assert_eq!(team::list(&a, false).unwrap().len(), 1);
    team::put(
        &a,
        Some(created.id.clone()),
        "A".into(),
        "Local A".into(),
        vec![],
    )
    .unwrap();
    team::put(
        &b,
        Some(created.id.clone()),
        "B".into(),
        "Local B".into(),
        vec![],
    )
    .unwrap();
    team::sync(&a).await.unwrap();
    team::sync(&b).await.unwrap();
    let conflicts = team::conflicts(&b).unwrap();
    assert_eq!(conflicts.len(), 1);
    assert_eq!(conflicts[0].local.content, "Local B");
    assert_eq!(conflicts[0].remote.content, "Local A");
    team::resolve(&b, &created.id, true).unwrap();
    team::sync(&b).await.unwrap();
    team::sync(&a).await.unwrap();
    assert_eq!(team::list(&a, false).unwrap()[0].content, "Local B");
    team::delete(&a, &created.id).unwrap();
    team::sync(&a).await.unwrap();
    team::sync(&b).await.unwrap();
    assert!(team::list(&b, false).unwrap().is_empty());
    assert!(team::list(&b, true).unwrap()[0].deleted);
    let reopened = context(&dir.path().join("b"), "test-alpha-token-long");
    assert!(team::list(&reopened, true).unwrap()[0].deleted);
    let transport = serde_json::to_string(&team::sync(&a).await.unwrap()).unwrap();
    assert!(!transport.contains("token"));
    assert!(!transport.contains("api_key"));
    server.abort();
    let _ = server.await;
}

#[tokio::test]
async fn offline_queue_recovers_and_retry_of_accepted_mutation_is_idempotent() {
    let dir = tempfile::tempdir().unwrap();
    let (url, server) = server(dir.path()).await;
    let a = context(&dir.path().join("a"), "test-alpha-token-long");
    auth(&a, &url, "alpha").await.unwrap();
    let first = team::put(&a, None, "one".into(), "offline content".into(), vec![]).unwrap();
    let mutation =
        a.db.with_conn(|c| {
            c.query_row(
                "SELECT mutation FROM team_memories WHERE id=?1",
                [&first.id],
                |r| r.get::<_, String>(0),
            )
        })
        .unwrap()
        .unwrap();
    let req = team::SyncRequest {
        cursor: 0,
        changes: vec![team::Mutation {
            memory: first.clone(),
            mutation: mutation.clone(),
        }],
    };
    let http = reqwest::Client::new();
    let endpoint = format!("{url}/v1/teams/alpha/sync");
    let response = http
        .post(&endpoint)
        .bearer_auth("test-alpha-token-long")
        .json(&req)
        .send()
        .await
        .unwrap()
        .json::<team::SyncResponse>()
        .await
        .unwrap();
    let repeated = http
        .post(&endpoint)
        .bearer_auth("test-alpha-token-long")
        .json(&req)
        .send()
        .await
        .unwrap()
        .json::<team::SyncResponse>()
        .await
        .unwrap();
    assert_eq!(response.cursor, repeated.cursor);
    assert_eq!(response.accepted[0].memory, repeated.accepted[0].memory);
    team::sync(&a).await.unwrap();
    assert!(team::conflicts(&a).unwrap().is_empty());
    assert_eq!(team::list(&a, false).unwrap().len(), 1);
    server.abort();
    let _ = server.await;
    team::put(
        &a,
        Some(first.id.clone()),
        "one".into(),
        "queued offline edit".into(),
        vec![],
    )
    .unwrap();
    assert!(team::sync(&a).await.is_err());
    assert_eq!(
        team::list(&a, false).unwrap()[0].content,
        "queued offline edit"
    );
    let listener = tokio::net::TcpListener::bind(url.trim_start_matches("http://"))
        .await
        .unwrap();
    let router = TeamServer::new(
        dir.path().join("server.sqlite3"),
        HashMap::from([("test-alpha-token-long".into(), "alpha".into())]),
    )
    .unwrap()
    .router();
    let restored = tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });
    team::sync(&a).await.unwrap();
    assert_eq!(
        team::list(&a, false).unwrap()[0].content,
        "queued offline edit"
    );
    restored.abort();
    let _ = restored.await;
}

#[tokio::test]
async fn cross_connection_lease_and_foreground_host_actually_work_and_stop() {
    let dir = tempfile::tempdir().unwrap();
    let (url, server) = server(dir.path()).await;
    let a = context(&dir.path().join("a"), "test-alpha-token-long");
    let other = context(&dir.path().join("a"), "test-alpha-token-long");
    auth(&a, &url, "alpha").await.unwrap();
    let lease = Lease::acquire(&a, "test").unwrap();
    assert!(Lease::acquire(&other, "test").is_err());
    lifecycle::stop(&other, "test").unwrap();
    assert!(!lease.renew().unwrap());
    drop(lease);
    assert!(Lease::acquire(&other, "test").is_ok());
    team::put(&a, None, "host".into(), "host pushes this".into(), vec![]).unwrap();
    let host = tokio::spawn(lifecycle::run_team_sync(a.clone(), 1));
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if lifecycle::status(&other)
                .unwrap()
                .iter()
                .any(|s| s.name == "team-sync" && s.completed > 0)
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .unwrap();
    assert!(lifecycle::run_team_sync(other.clone(), 1).await.is_err());
    lifecycle::stop(&other, "team-sync").unwrap();
    lifecycle::wait_stopped(&other, "team-sync").await.unwrap();
    host.await.unwrap().unwrap();
    let b = context(&dir.path().join("b"), "test-alpha-token-long");
    auth(&b, &url, "alpha").await.unwrap();
    team::sync(&b).await.unwrap();
    assert_eq!(team::list(&b, false).unwrap().len(), 1);
    team::put(
        &other,
        None,
        "after stop".into(),
        "must stay local".into(),
        vec![],
    )
    .unwrap();
    tokio::time::sleep(Duration::from_millis(1200)).await;
    team::sync(&b).await.unwrap();
    assert_eq!(team::list(&b, false).unwrap().len(), 1);
    server.abort();
    let _ = server.await;
}

async fn audio_endpoint(
    State(received): State<Arc<tokio::sync::Mutex<Vec<u8>>>>,
    headers: HeaderMap,
    mut body: Multipart,
) -> Result<Json<serde_json::Value>, StatusCode> {
    if headers.get("authorization").and_then(|v| v.to_str().ok()) != Some("Bearer audio-token") {
        return Err(StatusCode::UNAUTHORIZED);
    }
    let mut model = false;
    let mut audio = false;
    while let Some(field) = body
        .next_field()
        .await
        .map_err(|_| StatusCode::BAD_REQUEST)?
    {
        match field.name() {
            Some("file") => {
                let bytes = field.bytes().await.map_err(|_| StatusCode::BAD_REQUEST)?;
                audio = bytes.starts_with(b"RIFF");
                *received.lock().await = bytes.to_vec();
            }
            Some("model") => {
                model = field.text().await.map_err(|_| StatusCode::BAD_REQUEST)? == "test-stt";
            }
            _ => {}
        }
    }
    if !model || !audio {
        return Err(StatusCode::BAD_REQUEST);
    }
    Ok(Json(
        serde_json::json!({"text":"Transcribed test recording"}),
    ))
}
#[tokio::test]
async fn audio_bytes_are_uploaded_to_audio_api_and_errors_are_real() {
    let dir = tempfile::tempdir().unwrap();
    let c = context(dir.path(), "audio-token");
    let received = Arc::new(tokio::sync::Mutex::new(Vec::new()));
    let router = Router::new()
        .route("/v1/audio/transcriptions", post(audio_endpoint))
        .with_state(received.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });
    let audio = b"RIFF test audio bytes".to_vec();
    assert!(voice::transcribe(&c, audio.clone(), "audio/wav")
        .await
        .is_err());
    c.save_setting(
        "voice",
        &VoiceConfig {
            endpoint: format!("http://{addr}/v1/audio/transcriptions"),
            model: "test-stt".into(),
            credential: "token".into(),
            language: None,
        },
    )
    .unwrap();
    assert_eq!(
        voice::transcribe(&c, audio.clone(), "audio/wav")
            .await
            .unwrap(),
        "Transcribed test recording"
    );
    assert_eq!(*received.lock().await, audio);
    let bad = context(dir.path(), "wrong-key");
    assert!(voice::transcribe(&bad, audio.clone(), "audio/wav")
        .await
        .unwrap_err()
        .to_string()
        .contains("401"));
    assert!(voice::transcribe(&c, vec![], "audio/wav").await.is_err());
    assert!(voice::transcribe(&c, audio, "text/plain").await.is_err());
    assert!(services::validate_endpoint("http://remote.example/api").is_err());
    server.abort();
    let _ = server.await;
}

#[tokio::test]
async fn legacy_import_is_explicit_backed_up_idempotent_and_atomic() {
    let dir = tempfile::tempdir().unwrap();
    let (url, server) = server(dir.path()).await;
    let a = context(&dir.path().join("a"), "test-alpha-token-long");
    auth(&a, &url, "alpha").await.unwrap();
    let source = dir.path().join("legacy-team");
    std::fs::create_dir(&source).unwrap();
    let old=serde_json::json!({"id":"legacy-one","title":"legacy","content":"legacy content","author":"user","tags":["old"],"etag":"untrusted-etag","created_at":"2026-01-01T00:00:00Z","updated_at":"2026-01-01T00:00:00Z"}).to_string();
    std::fs::write(source.join("one.json"), &old).unwrap();
    std::fs::write(source.join(".team_token"), "fake-old-token").unwrap();
    let preview = team::import_legacy(&a, &source, true).unwrap();
    assert_eq!(preview.imported, 1);
    assert!(preview.backup.is_none());
    assert!(team::list(&a, false).unwrap().is_empty());
    let report = team::import_legacy(&a, &source, false).unwrap();
    assert_eq!(report.imported, 1);
    assert!(std::path::Path::new(&report.backup.unwrap()).exists());
    assert_eq!(team::import_legacy(&a, &source, false).unwrap().skipped, 1);
    assert_eq!(
        std::fs::read_to_string(source.join("one.json")).unwrap(),
        old
    );
    assert_eq!(team::list(&a, false).unwrap()[0].revision, 0);
    team::sync(&a).await.unwrap();
    assert!(team::conflicts(&a).unwrap().is_empty());
    std::fs::write(
        source.join("two.json"),
        serde_json::json!({"id":"legacy-two","title":"two","content":"two"}).to_string(),
    )
    .unwrap();
    std::fs::write(
        source.join("one.json"),
        serde_json::json!({"id":"legacy-one","title":"conflicting","content":"different"})
            .to_string(),
    )
    .unwrap();
    assert!(team::import_legacy(&a, &source, false).is_err());
    assert_eq!(team::list(&a, false).unwrap().len(), 1);
    assert_eq!(
        std::fs::read_to_string(source.join(".team_token")).unwrap(),
        "fake-old-token"
    );
    server.abort();
    let _ = server.await;
}

#[tokio::test]
async fn stopping_host_cancels_a_stalled_request_and_releases_both_leases() {
    let dir = tempfile::tempdir().unwrap();
    let started = Arc::new(tokio::sync::Notify::new());
    let signal = started.clone();
    let router = Router::new().route(
        "/v1/teams/alpha/sync",
        post(move || {
            let signal = signal.clone();
            async move {
                signal.notify_one();
                std::future::pending::<()>().await;
                Json(serde_json::json!({}))
            }
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });
    let a = context(&dir.path().join("a"), "test-alpha-token-long");
    a.save_setting(
        "team-sync",
        &TeamConfig {
            team: "alpha".into(),
            endpoint: format!("http://{addr}"),
            credential: "token".into(),
        },
    )
    .unwrap();
    let host = tokio::spawn(lifecycle::run_team_sync(a.clone(), 1));
    tokio::time::timeout(Duration::from_secs(3), started.notified())
        .await
        .unwrap();
    lifecycle::stop(&a, "team-sync").unwrap();
    tokio::time::timeout(Duration::from_secs(2), host)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert!(lifecycle::status(&a)
        .unwrap()
        .iter()
        .all(|s| s.state == "stopped"));
    assert!(Lease::acquire(&a, "team-sync").is_ok());
    assert!(Lease::acquire(&a, "team-request:alpha").is_ok());
    server.abort();
    let _ = server.await;
}
