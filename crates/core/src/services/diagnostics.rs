use anyhow::{ensure, Result};
use serde::Serialize;
use std::time::{Duration, Instant};

#[derive(Debug, Serialize)]
pub struct StressReport {
    pub endpoint: String,
    pub requests: usize,
    pub concurrency: usize,
    pub succeeded: usize,
    pub failed: usize,
    pub elapsed_ms: u128,
    pub min_ms: u128,
    pub max_ms: u128,
    pub mean_ms: f64,
}
/// Real bounded HTTP requests. The default endpoint is a temporary local server.
pub async fn stress(
    concurrency: usize,
    iterations: usize,
    endpoint: Option<String>,
    allow_remote: bool,
) -> Result<StressReport> {
    ensure!(
        (1..=64).contains(&concurrency)
            && (1..=1000).contains(&iterations)
            && concurrency * iterations <= 10_000,
        "Use concurrency 1..64, iterations 1..1000, total at most 10000"
    );
    let mut local_server = None;
    let endpoint = if let Some(endpoint) = endpoint {
        let url = super::validate_endpoint(&endpoint)?;
        ensure!(
            allow_remote
                || matches!(
                    url.host_str(),
                    Some("localhost" | "127.0.0.1" | "[::1]" | "::1")
                ),
            "Remote diagnostics require --allow-remote"
        );
        endpoint
    } else {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
        let endpoint = format!("http://{}/health", listener.local_addr()?);
        let mut tasks = tokio::task::JoinSet::new();
        tasks.spawn(async move {
            axum::serve(
                listener,
                axum::Router::new().route("/health", axum::routing::get(|| async { "ok" })),
            )
            .await
        });
        local_server = Some(tasks);
        endpoint
    };
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .redirect(reqwest::redirect::Policy::none())
        .build()?;
    let start = Instant::now();
    let mut tasks = tokio::task::JoinSet::new();
    for _ in 0..concurrency {
        let client = client.clone();
        let endpoint = endpoint.clone();
        tasks.spawn(async move {
            let mut results = Vec::new();
            for _ in 0..iterations {
                let started = Instant::now();
                let ok = client
                    .get(&endpoint)
                    .send()
                    .await
                    .is_ok_and(|r| r.status().is_success());
                results.push((ok, started.elapsed().as_millis()));
            }
            results
        });
    }
    let mut results = Vec::new();
    while let Some(result) = tasks.join_next().await {
        results.extend(result?);
    }
    let succeeded = results.iter().filter(|(ok, _)| *ok).count();
    let report = StressReport {
        endpoint,
        requests: results.len(),
        concurrency,
        succeeded,
        failed: results.len() - succeeded,
        elapsed_ms: start.elapsed().as_millis(),
        min_ms: results.iter().map(|(_, ms)| *ms).min().unwrap_or(0),
        max_ms: results.iter().map(|(_, ms)| *ms).max().unwrap_or(0),
        mean_ms: results.iter().map(|(_, ms)| *ms as f64).sum::<f64>() / results.len() as f64,
    };
    if let Some(mut server) = local_server {
        server.abort_all();
        while server.join_next().await.is_some() {}
    }
    Ok(report)
}
