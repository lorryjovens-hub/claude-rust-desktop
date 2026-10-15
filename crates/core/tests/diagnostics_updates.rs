use axum::{routing::get, Json, Router};
use claude_core::services::{diagnostics, updates};
use serde_json::json;
#[tokio::test]
async fn diagnostics_use_real_requests_and_validate_limits() {
    let result = diagnostics::stress(2, 3, None, false).await.unwrap();
    assert_eq!(result.requests, 6);
    assert_eq!(result.succeeded, 6);
    assert_eq!(result.failed, 0);
    assert!(diagnostics::stress(0, 1, None, false).await.is_err());
    assert!(
        diagnostics::stress(1, 1, Some("https://example.com".into()), false)
            .await
            .is_err()
    );
}
#[tokio::test]
async fn updates_distinguish_current_newer_and_invalid_responses() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let root = format!("http://{}", listener.local_addr().unwrap());
    let server = tokio::spawn(async move {
        axum::serve(
            listener,
            Router::new()
                .route(
                    "/manifest",
                    get(|| async { Json(json!({"version":"3.2.0"})) }),
                )
                .route("/invalid", get(|| async { "not json" })),
        )
        .await
        .unwrap()
    });
    assert!(updates::check(&format!("{root}/manifest"), "3.1.0")
        .await
        .unwrap()
        .is_some());
    assert!(updates::check(&format!("{root}/manifest"), "3.2.0")
        .await
        .unwrap()
        .is_none());
    assert!(updates::check(&format!("{root}/manifest"), "3.2.0-beta.1")
        .await
        .unwrap()
        .is_some());
    assert!(updates::check(&format!("{root}/invalid"), "3.1.0")
        .await
        .is_err());
    assert!(updates::check(&format!("{root}/missing"), "3.1.0")
        .await
        .is_err());
    server.abort();
}
