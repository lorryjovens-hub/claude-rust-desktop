use super::*;

fn search_provider(base: &str, id: &str) -> ResolvedProvider {
    serde_json::from_value::<crate::native_engine::provider_manager::Provider>(json!({
        "id": id, "name": id, "base_url": base, "api_key": format!("key-{id}"),
        "api_format": "openai", "models": [], "enabled": true,
        "web_search_strategy": "openai_native"
    }))
    .map(|provider| ResolvedProvider {
        provider,
        model: serde_json::from_value(json!({
            "id": id, "name": id, "enabled": true,
            "supports_vision": false, "supports_web_search": true
        })).unwrap(),
    }).unwrap()
}

#[tokio::test]
async fn parallel_searches_keep_provider_credentials_and_usage_separate() {
    use axum::{http::HeaderMap, routing::post, Json, Router};
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let app = Router::new().route("/v1/responses", post(|headers: HeaderMap, Json(body): Json<Value>| async move {
        let model = body["model"].as_str().unwrap();
        assert_eq!(headers["authorization"], format!("Bearer key-{model}"));
        assert_eq!(body["tools"][0]["type"], "web_search");
        Json(json!({"id": format!("response-{model}"), "model": model, "status": "completed",
            "output": [{"type":"web_search_call", "status":"completed", "action":{"sources":[{"url":format!("https://{model}.example"), "title":model}]}}],
            "usage":{"input_tokens":100,"output_tokens":20,"total_tokens":120,"input_tokens_details":{"cached_tokens":40}}}))
    }));
    let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap(); });
    let (tx, mut rx) = mpsc::channel(32);
    let counter = Arc::new(AtomicUsize::new(0));
    let a = execute_tool_standalone("WebSearch".into(), json!({"query":"Rust release notes"}), "tool-a".into(), tx.clone(), None,
        ".".into(), None, None, counter.clone(), 2, search_provider(&base, "model-a"), None);
    let b = execute_tool_standalone("WebSearch".into(), json!({"query":"Rust release notes"}), "tool-b".into(), tx.clone(), None,
        ".".into(), None, None, counter.clone(), 2, search_provider(&base, "model-b"), None);
    let (a, b) = tokio::join!(a, b);
    assert!(!a.4 && !b.4);
    assert!(a.3.contains("model-a.example") && !a.3.contains("model-b.example"));
    assert!(b.3.contains("model-b.example") && !b.3.contains("model-a.example"));
    assert_eq!(counter.load(Ordering::SeqCst), 2);
    let mut completed = Vec::new();
    while let Ok(event) = rx.try_recv() {
        if let EngineEvent::RequestSnapshot(snapshot) = event {
            assert_eq!(snapshot["source"], "web_search");
            assert!(!snapshot.to_string().contains("key-"));
            if snapshot["status"] == "completed" { completed.push(snapshot); }
        }
    }
    assert_eq!(completed.len(), 2);
    for snapshot in completed {
        assert_eq!(snapshot["providerId"], snapshot["model"]);
        assert_eq!(snapshot["totalTokens"], 120);
        assert_eq!(snapshot["cacheReadTokens"], 40);
        assert_eq!(snapshot["apiFormat"], "openai");
        assert!(snapshot["ttftMs"].is_null()); // Nonstreaming search has no measured first token.
    }
    server.abort();
}

#[tokio::test]
async fn search_failure_still_records_provider_usage() {
    use axum::{routing::post, Json, Router};
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let provider = search_provider(&format!("http://{}", listener.local_addr().unwrap()), "failed");
    let app = Router::new().route("/v1/responses", post(|| async {
        Json(json!({"model":"failed", "output":[{"type":"message"}], "usage":{"input_tokens":50,"output_tokens":10,"total_tokens":60}}))
    }));
    let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap(); });
    let (tx, mut rx) = mpsc::channel(8);
    assert!(execute_builtin_tool("WebSearch", json!({"query":"Rust"}), ".", &provider, &tx, None).await.is_err());
    let mut final_snapshot = None;
    while let Ok(event) = rx.try_recv() {
        if let EngineEvent::RequestSnapshot(snapshot) = event { final_snapshot = Some(snapshot); }
    }
    let snapshot = final_snapshot.unwrap();
    assert_eq!(snapshot["status"], "error");
    assert_eq!(snapshot["totalTokens"], 60);
    server.abort();
}

#[tokio::test]
async fn deepseek_search_requires_separate_settings_instead_of_chat_credentials() {
    let mut provider = search_provider("http://127.0.0.1:1", "deepseek-v4.1-flash");
    provider.provider.web_search_strategy = None;
    let (tx, mut rx) = mpsc::channel(8);
    let error = execute_builtin_tool("WebSearch", json!({"query":"Rust"}), ".", &provider, &tx, None)
        .await.unwrap_err().to_string();
    assert!(error.contains("DeepSeek") && error.contains("API Key"));
    assert!(error.contains("联网搜索"));
    assert!(!error.contains("key-deepseek"));
    assert!(rx.try_recv().is_err()); // No model request was made or counted.
}

#[tokio::test]
async fn deepseek_search_reads_independent_endpoint_model_and_new_key_on_each_call() {
    use axum::{http::HeaderMap, routing::post, Json, Router};
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}/anthropic/v1", listener.local_addr().unwrap());
    let requests = Arc::new(Mutex::new(Vec::<(String, String)>::new()));
    let captured = requests.clone();
    let app = Router::new().route("/anthropic/v1/messages", post(move |headers: HeaderMap, Json(body): Json<Value>| {
        let captured = captured.clone();
        async move {
            captured.lock().await.push((headers["x-api-key"].to_str().unwrap().to_string(), body["model"].as_str().unwrap().to_string()));
            Json(json!({"model":body["model"], "content":[{"type":"web_search_tool_result","content":[{"type":"web_search_result","url":"https://www.rust-lang.org","title":"Rust"}]}],
                "usage":{"input_tokens":30,"output_tokens":10,"cache_read_input_tokens":20}}))
        }
    }));
    let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap(); });
    let temp = tempfile::tempdir().unwrap();
    let config_path = temp.path().join("config.json");
    let mut chat = search_provider("http://127.0.0.1:1", "proxy/deepseek-v4.1-flash");
    chat.provider.web_search_strategy = None;
    let (tx, mut rx) = mpsc::channel(16);
    for (key, model) in [("search-key-one", "deepseek-search-one"), ("search-key-two", "deepseek-search-two")] {
        let mut config = crate::config::AppConfig::default();
        config.deepseek_search = serde_json::from_value(json!({"base_url":base,"model":model,"api_key":key})).unwrap();
        std::fs::write(&config_path, serde_json::to_vec(&config).unwrap()).unwrap();
        let value = execute_builtin_tool("WebSearch", json!({"query":"Rust"}), ".", &chat, &tx, Some(&config_path)).await.unwrap();
        assert_eq!(value["provider_id"], "deepseek-search");
        assert_eq!(value["model"], model);
    }
    assert_eq!(*requests.lock().await, vec![("search-key-one".into(), "deepseek-search-one".into()), ("search-key-two".into(), "deepseek-search-two".into())]);
    while let Ok(event) = rx.try_recv() {
        if let EngineEvent::RequestSnapshot(snapshot) = event {
            assert_eq!(snapshot["providerId"], "deepseek-search");
            assert!(!snapshot.to_string().contains("search-key"));
            if snapshot["status"] == "completed" {
                assert_eq!(snapshot["apiFormat"], "anthropic");
                assert_eq!(snapshot["totalTokens"], 60);
            }
        }
    }
    server.abort();
}
