use super::*;

fn provider(base: &str, format: &str) -> ResolvedProvider {
    static INIT: std::sync::Once = std::sync::Once::new();
    INIT.call_once(|| crate::config::OrchestrationConfig::init_global().unwrap());
    ResolvedProvider {
        provider: serde_json::from_value(json!({
            "id": "code-test", "name": "code-test", "base_url": base,
            "api_key": "local-test-key", "api_format": format, "models": [], "enabled": true
        })).unwrap(),
        model: serde_json::from_value(json!({"id": "code-test", "name": "code-test", "enabled": true,
            "supports_vision": false, "supports_web_search": false})).unwrap(),
    }
}

#[tokio::test]
async fn all_modes_preserve_memory_and_settings_in_either_builder_order() {
    let temp = tempfile::tempdir().unwrap();
    let (tx, _rx) = mpsc::channel(16);
    for mode in ["chat", "work", "study", "code"] {
    for memory_first in [true, false] {
        let client = MemExClient::with_path(temp.path().join("memory.sqlite3"), None);
        let mut config = client.get_config().await;
        config.auto_search = false;
        config.auto_ingest = false;
        config.search_top_k = 7;
        client.update_config(config).await.unwrap();
        let profile = PromptProfile::from_workspace_mode(mode);
        let executor = ToolLoopExecutor::new(provider("http://127.0.0.1:1", "openai"), vec![], None, 128, tx.clone(), temp.path().to_string_lossy().into(), ReasoningMode::Standard);
        let mut executor = if memory_first {
            executor.with_memex(client.clone()).with_prompt_profile(profile)
        } else {
            executor.with_prompt_profile(profile).with_memex(client.clone())
        };
        assert!(executor.memex_client.is_some());
        assert!(executor.context_manager.is_some());
        let config = executor.memex_client.as_ref().unwrap().get_config().await;
        assert!(!config.auto_search && !config.auto_ingest);
        assert_eq!(config.search_top_k, 7);
        for name in ["memory_ingest", "memory_search"] {
            let (_, output, is_error) = executor.execute_tool_call(name, &json!({"content":"remember native memory", "query":"native memory", "importance":0.9}), "test").await;
            assert!(!is_error, "{mode}: {output}");
            assert!(output.contains(if name == "memory_ingest" { "ingested" } else { "remember native memory" }));
        }
    }
    }
}

/// Real HTTP/SSE requests against a local mock, with no external model credentials.
/// Exercise real memory tools, automatic recall and ingestion in all four modes.
#[tokio::test]
async fn all_modes_recall_ingest_and_run_memory_tools_on_both_protocols() {
    use axum::{http::header, routing::post, Json, Router};
    for format in ["anthropic", "openai"] {
    for mode in ["chat", "work", "study", "code"] {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let captured = Arc::new(Mutex::new(Vec::<Value>::new()));
        let requests = captured.clone();
        let app = Router::new().route(if format == "anthropic" { "/v1/messages" } else { "/v1/chat/completions" }, post(move |Json(body): Json<Value>| {
            let requests = requests.clone();
            async move {
                let mut requests = requests.lock().await;
                let step = requests.len();
                let first = step < 2;
                let tool_name = if step == 0 { "memory_ingest" } else { "memory_search" };
                let arguments = if step == 0 { json!({"content":"remember toolmemory", "importance":0.9}) } else { json!({"query":"toolmemory"}) }.to_string();
                requests.push(body);
                let events = if format == "anthropic" {
                    if first {
                        vec![json!({"type":"message_start","message":{"id":"test","model":"code-test","usage":{"input_tokens":20}}}),
                            json!({"type":"content_block_start","index":0,"content_block":{"type":"tool_use","id":format!("memory-{step}"),"name":tool_name,"input":{}}}),
                            json!({"type":"content_block_delta","index":0,"delta":{"type":"input_json_delta","partial_json":arguments}}),
                            json!({"type":"content_block_stop","index":0}),
                            json!({"type":"message_delta","delta":{"stop_reason":"tool_use"},"usage":{"output_tokens":10}}),
                            json!({"type":"message_stop"})]
                    } else {
                        vec![json!({"type":"message_start","message":{"id":"test2","model":"code-test","usage":{"input_tokens":20}}}),
                            json!({"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}),
                            json!({"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"verified"}}),
                            json!({"type":"content_block_stop","index":0}),
                            json!({"type":"message_delta","delta":{"stop_reason":"end_turn"},"usage":{"output_tokens":10}}),
                            json!({"type":"message_stop"})]
                    }
                } else if first {
                    vec![json!({"id":"test","model":"code-test","choices":[{"index":0,"delta":{"tool_calls":[{"index":0,"id":format!("memory-{step}"),"type":"function","function":{"name":tool_name,"arguments":arguments}}]},"finish_reason":null}]}),
                        json!({"choices":[{"index":0,"delta":{},"finish_reason":"tool_calls"}]})]
                } else {
                    vec![json!({"id":"test2","model":"code-test","choices":[{"index":0,"delta":{"content":"verified"},"finish_reason":null}]}),
                        json!({"choices":[{"index":0,"delta":{},"finish_reason":"stop"}]})]
                };
                let mut stream = events.into_iter().map(|event| format!("data: {event}\n\n")).collect::<String>();
                if format == "openai" { stream.push_str("data: [DONE]\n\n"); }
                ([(header::CONTENT_TYPE, "text/event-stream")], stream)
            }
        }));
        let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap(); });
        let (tx, mut rx) = mpsc::channel(128);
        let temp = tempfile::tempdir().unwrap();
        let client = MemExClient::with_path(temp.path().join("memory.sqlite3"), None);
        let config = client.get_config().await;
        assert!(config.enabled && config.auto_search && config.auto_ingest);
        client.ingest("recallmarker existing architecture", Some(0.9), None).await.unwrap();
        let profile = PromptProfile::from_workspace_mode(mode);
        let prompt = crate::prompt::compose_system_prompt(Some("custom project rule"), "code-test", profile);
        let mut executor = ToolLoopExecutor::new(provider(&base, format), vec![json!({"role":"user","content":"recallmarker"})], Some(prompt), 128, tx, ".".into(), ReasoningMode::Standard)
            .with_prompt_profile(profile).with_memex(client.clone()).with_conv_id(format!("{mode}-{format}"));
        let (text, _) = tokio::time::timeout(Duration::from_secs(15), executor.execute()).await.unwrap().unwrap();
        server.abort();
        assert_eq!(text, "verified", "{format}");
        let requests = captured.lock().await;
        assert_eq!(requests.len(), 3, "{format} {mode}");
        let first = &requests[0];
        let system = if format == "anthropic" { first["system"].to_string() } else { first["messages"][0]["content"].to_string() };
        assert_eq!(system.contains("<code_behavior>"), mode == "code");
        assert!(system.contains("custom project rule") && system.contains("<memory_guidance>"));
        assert!(first["tools"].to_string().contains("memory_ingest"));
        assert!(first["tools"].to_string().contains("memory_search"));
        assert!(first["messages"].to_string().contains("recallmarker existing architecture"));
        assert!(requests[1]["messages"].to_string().contains("ingested"));
        assert!(requests[2]["messages"].to_string().contains("search_completed"));
        assert!(requests[2]["messages"].to_string().contains("remember toolmemory"));
        let saved = client.search("verified", None).await.unwrap();
        assert!(saved.iter().any(|item| item.content.contains("Assistant: verified")), "automatic ingestion: {mode} {format}");
        let mut completed = Vec::new();
        while let Ok(event) = rx.try_recv() {
            if let EngineEvent::ToolUseDone { tool_name, is_error, .. } = event {
                if is_memory_tool(&tool_name) { assert!(!is_error); completed.push(tool_name); }
            }
        }
        assert_eq!(completed, vec!["memory_ingest", "memory_search"], "{mode} {format}");
    }
    }
}
