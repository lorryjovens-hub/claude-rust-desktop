use super::*;
use crate::native_engine::{
    openai_client::{OpenAIClient, OpenAIContent},
    provider_manager::Provider,
};
use axum::{
    extract::State,
    http::{HeaderMap, Uri},
    routing::post,
    Json, Router,
};
use serde_json::json;
use std::sync::{Arc, Mutex};

fn provider(id: &str, api: &str, url: &str) -> ResolvedProvider {
    let p: Provider = serde_json::from_value(json!({"id":"test","name":"test","base_url":url,"api_key":"fixture-only","api_format":"openai","builtin_id":id,"enabled":true,"models":[{"id":"fixture-model","name":"fixture","enabled":true,"max_tokens":8192,"supports_vision":true,"supports_web_search":false,"api":api}]})).unwrap();
    ResolvedProvider {
        model: p.models[0].clone(),
        provider: p,
    }
}
fn user() -> OpenAIMessage {
    OpenAIMessage {
        role: "user".into(),
        content: OpenAIContent::Text("hello".into()),
        tool_calls: None,
        tool_call_id: None,
        reasoning_content: None,
        provider_content: None,
    }
}

#[test]
fn requests_preserve_protocol_and_reasoning_replay() {
    let p = provider("openai", "openai-responses", "http://localhost/v1");
    let mut assistant = user();
    assistant.role = "assistant".into();
    assistant.provider_content = Some(
        json!({"api":"openai-responses","items":[{"type":"reasoning","id":"rs_1","encrypted_content":"opaque"},{"type":"function_call","call_id":"call_1","name":"read","arguments":"{}"}]}),
    );
    let body = messages::request(
        "openai-responses",
        &p,
        &Value::Null,
        vec![user(), assistant],
        Some("system"),
        vec![],
        4000,
    )
    .unwrap();
    assert_eq!(body["input"][1]["encrypted_content"], "opaque");
    assert_eq!(body["input"][2]["call_id"], "call_1");
    assert_eq!(body["max_output_tokens"], 4000);
    assert_eq!(body["store"], false);
    let mut p = provider("deepseek", "openai-completions", "http://localhost");
    p.model.effort = Some("high".into());
    let body = messages::request(
        "openai-completions",
        &p,
        &json!({"compat":{"thinkingFormat":"deepseek"}}),
        vec![user()],
        None,
        vec![],
        100,
    )
    .unwrap();
    assert_eq!(body["thinking"]["type"], "enabled");
    assert_eq!(body["reasoning_effort"], "high");
}

#[test]
fn anthropic_stream_keeps_signatures_tools_and_cache_usage() {
    let mut s = events::State::default();
    let events = vec![
        json!({"type":"message_start","message":{"id":"msg_1","model":"claude","usage":{"input_tokens":2,"cache_read_input_tokens":3,"cache_creation_input_tokens":4}}}),
        json!({"type":"content_block_start","index":0,"content_block":{"type":"thinking","thinking":""}}),
        json!({"type":"content_block_delta","index":0,"delta":{"type":"thinking_delta","thinking":"reason"}}),
        json!({"type":"content_block_delta","index":0,"delta":{"type":"signature_delta","signature":"signed"}}),
        json!({"type":"content_block_start","index":1,"content_block":{"type":"tool_use","id":"tool_1","name":"read","input":{}}}),
        json!({"type":"content_block_delta","index":1,"delta":{"type":"input_json_delta","partial_json":"{\"path\":\"a\"}"}}),
        json!({"type":"content_block_stop","index":1}),
        json!({"type":"message_delta","delta":{"stop_reason":"tool_use"},"usage":{"output_tokens":5}}),
        json!({"type":"message_stop"}),
    ];
    let out: Vec<_> = events
        .into_iter()
        .flat_map(|e| s.convert("anthropic-messages", e).unwrap())
        .collect();
    let last = out.last().unwrap();
    assert_eq!(last["provider_content"]["items"][0]["signature"], "signed");
    assert_eq!(last["provider_content"]["items"][1]["input"]["path"], "a");
    assert_eq!(last["usage"]["prompt_tokens"], 9);
    assert_eq!(last["usage"]["total_tokens"], 14);
    assert_eq!(last["choices"][0]["finish_reason"], "tool_calls");
    assert!(s.finished);
}

#[test]
fn gemini_keeps_thought_signatures_and_function_results() {
    let mut s = events::State::default();
    let out = s.convert("google-generative-ai",json!({"candidates":[{"content":{"parts":[{"thought":true,"text":"reason","thoughtSignature":"opaque"},{"functionCall":{"name":"read","args":{"path":"a"}},"thoughtSignature":"tool-signature"}]},"finishReason":"STOP"}],"usageMetadata":{"promptTokenCount":2,"candidatesTokenCount":3,"thoughtsTokenCount":4,"totalTokenCount":9}})).unwrap();
    let last = out.last().unwrap();
    assert_eq!(
        last["provider_content"]["items"][1]["thoughtSignature"],
        "tool-signature"
    );
    assert_eq!(last["usage"]["completion_tokens"], 7);
    assert_eq!(last["choices"][0]["finish_reason"], "tool_calls");
    assert!(last["provider_content"]["items"][1]["functionCall"]["id"]
        .as_str()
        .unwrap()
        .starts_with("call_"));
}

#[tokio::test]
async fn native_http_stream_uses_selected_endpoint_and_assembles_fragmented_calls() {
    type Requests = Arc<Mutex<Vec<(String, HeaderMap, Value)>>>;
    async fn handler(
        State(requests): State<Requests>,
        uri: Uri,
        headers: HeaderMap,
        Json(body): Json<Value>,
    ) -> String {
        requests
            .lock()
            .unwrap()
            .push((uri.to_string(), headers, body));
        [json!({"choices":[{"delta":{"content":"hello","tool_calls":[{"index":0,"id":"call_1","type":"function","function":{"name":"read","arguments":"{\"p\":"}}]},"finish_reason":null}]}),json!({"choices":[{"delta":{"tool_calls":[{"index":0,"function":{"arguments":"\"x\"}"}}]},"finish_reason":"tool_calls"}],"usage":{"prompt_tokens":2,"completion_tokens":3,"total_tokens":5}})].into_iter().map(|v| format!("data: {v}\r\n\r\n")).collect::<String>() + "data: [DONE]\r\n\r\n"
    }
    let requests = Requests::default();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let app = Router::new()
        .route("/custom/chat/completions", post(handler))
        .with_state(requests.clone());
    let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let p = provider(
        "deepseek",
        "openai-completions",
        &format!("http://{addr}/custom"),
    );
    let result = OpenAIClient::new()
        .send_message(&p, vec![user()], None, vec![], 1024)
        .await
        .unwrap();
    assert_eq!(
        result.choices[0].message.tool_calls.as_ref().unwrap()[0]
            .function
            .arguments,
        "{\"p\":\"x\"}"
    );
    let requests = requests.lock().unwrap();
    assert_eq!(requests[0].0, "/custom/chat/completions");
    assert_eq!(requests[0].1["authorization"], "Bearer fixture-only");
    assert_eq!(requests[0].2["max_tokens"], 1024);
    server.abort();
}

#[tokio::test]
async fn unsupported_protocol_fails_before_network_or_process_launch() {
    let p = provider("amazon-bedrock", "unknown-protocol", "http://127.0.0.1:1");
    let error = stream(&p, vec![user()], None, vec![], 100)
        .await
        .err()
        .unwrap()
        .to_string();
    assert!(error.contains("not implemented"));
}

#[test]
fn responses_preserves_output_and_reports_failure() {
    let mut state = events::State::default();
    let call = json!({"type":"function_call","id":"fc_1","call_id":"call_1","name":"read","arguments":"{}"});
    let out = state
        .convert(
            "openai-responses",
            json!({"type":"response.output_item.done","item":call}),
        )
        .unwrap();
    assert_eq!(
        out[0]["choices"][0]["delta"]["tool_calls"][0]["id"],
        "call_1"
    );
    let output = json!([{"type":"reasoning","encrypted_content":"encrypted"},call]);
    let out = state.convert("openai-responses",json!({"type":"response.completed","response":{"id":"resp_1","model":"gpt","status":"completed","output":output,"usage":{"input_tokens":3,"output_tokens":7,"total_tokens":10,"input_tokens_details":{"cached_tokens":2},"output_tokens_details":{"reasoning_tokens":4}}}})).unwrap();
    assert_eq!(out[0]["provider_content"]["items"], output);
    assert_eq!(
        out[0]["usage"]["completion_tokens_details"]["reasoning_tokens"],
        4
    );
    assert!(state
        .convert(
            "openai-responses",
            json!({"type":"response.failed","response":{"error":{"message":"fixture failure"}}})
        )
        .is_err());
}

#[tokio::test]
async fn fragmented_utf8_and_incomplete_stream_are_handled_without_data_loss() {
    async fn handler() -> axum::response::Response {
        let payload = format!(
            "data: {}\n\n",
            json!({"choices":[{"delta":{"content":"你好"},"finish_reason":null}]})
        );
        let bytes = payload
            .into_bytes()
            .into_iter()
            .map(|byte| Ok::<_, std::io::Error>(bytes::Bytes::from(vec![byte])));
        axum::response::Response::new(axum::body::Body::from_stream(futures::stream::iter(bytes)))
    }
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        axum::serve(
            listener,
            Router::new().route("/chat/completions", post(handler)),
        )
        .await
        .unwrap()
    });
    let p = provider("deepseek", "openai-completions", &format!("http://{addr}"));
    let mut response = stream(&p, vec![user()], None, vec![], 100).await.unwrap();
    assert!(response.next().await.unwrap().unwrap().contains("你好"));
    assert!(response
        .next()
        .await
        .unwrap()
        .unwrap_err()
        .to_string()
        .contains("before completion"));
    server.abort();
}

#[tokio::test]
async fn dropping_stream_cancels_http_body() {
    use std::sync::atomic::{AtomicBool, Ordering};
    struct Dropped(Arc<AtomicBool>);
    impl Drop for Dropped {
        fn drop(&mut self) {
            self.0.store(true, Ordering::SeqCst);
        }
    }
    async fn handler(State(dropped): State<Arc<AtomicBool>>) -> axum::response::Response {
        let stream = async_stream::stream! {
            let _guard = Dropped(dropped);
            loop {
                yield Ok::<_,std::io::Error>(bytes::Bytes::from_static(b"data: {\"choices\":[{\"delta\":{\"content\":\"x\"},\"finish_reason\":null}]}\n\n"));
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
        };
        axum::response::Response::new(axum::body::Body::from_stream(stream))
    }
    let dropped = Arc::new(AtomicBool::new(false));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let app = Router::new()
        .route("/chat/completions", post(handler))
        .with_state(dropped.clone());
    let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let p = provider("deepseek", "openai-completions", &format!("http://{addr}"));
    let mut response = stream(&p, vec![user()], None, vec![], 100).await.unwrap();
    response.next().await.unwrap().unwrap();
    drop(response);
    tokio::time::timeout(std::time::Duration::from_secs(3), async {
        while !dropped.load(Ordering::SeqCst) {
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("HTTP body was not cancelled");
    server.abort();
}

#[tokio::test]
async fn native_protocol_matrix_uses_correct_endpoints_headers_and_payloads() {
    type Captured = Arc<Mutex<Vec<(String, HeaderMap, Value)>>>;
    #[derive(Clone)]
    struct Fixture {
        requests: Captured,
        response: Vec<u8>,
    }
    async fn handler(
        State(state): State<Fixture>,
        uri: Uri,
        headers: HeaderMap,
        Json(body): Json<Value>,
    ) -> Vec<u8> {
        state
            .requests
            .lock()
            .unwrap()
            .push((uri.to_string(), headers, body));
        state.response
    }
    let sse = |events: Vec<Value>| {
        events
            .into_iter()
            .map(|e| format!("data: {e}\n\n"))
            .collect::<String>()
            .into_bytes()
    };
    for (id, api, base, path) in [
        ("anthropic", "anthropic-messages", "", "/v1/messages"),
        ("openai", "openai-responses", "/v1", "/v1/responses"),
        (
            "google",
            "google-generative-ai",
            "/v1beta",
            "/v1beta/models/fixture-model:streamGenerateContent?alt=sse",
        ),
        (
            "google-vertex",
            "google-vertex",
            "/v1/publishers/google",
            "/v1/publishers/google/models/fixture-model:streamGenerateContent?alt=sse",
        ),
        (
            "azure-openai-responses",
            "azure-openai-responses",
            "/openai/v1",
            "/openai/v1/responses?api-version=v1",
        ),
        (
            "openai-codex",
            "openai-codex-responses",
            "/backend-api",
            "/backend-api/codex/responses",
        ),
        (
            "mistral",
            "mistral-conversations",
            "",
            "/v1/chat/completions",
        ),
        (
            "amazon-bedrock",
            "bedrock-converse-stream",
            "",
            "/model/fixture-model/converse-stream",
        ),
    ] {
        let response = match api {
            "anthropic-messages" => sse(vec![
                json!({"type":"message_start","message":{"id":"m","model":"fixture-model","usage":{"input_tokens":2}}}),
                json!({"type":"content_block_start","index":0,"content_block":{"type":"text","text":"native reply"}}),
                json!({"type":"message_delta","delta":{"stop_reason":"end_turn"},"usage":{"output_tokens":3}}),
                json!({"type":"message_stop"}),
            ]),
            "google-generative-ai" | "google-vertex" => sse(vec![
                json!({"candidates":[{"content":{"parts":[{"text":"native reply"}]},"finishReason":"STOP"}],"usageMetadata":{"promptTokenCount":2,"candidatesTokenCount":3,"totalTokenCount":5}}),
            ]),
            "bedrock-converse-stream" => [
                ("messageStart", json!({"role":"assistant"})),
                (
                    "contentBlockDelta",
                    json!({"contentBlockIndex":0,"delta":{"text":"native reply"}}),
                ),
                ("messageStop", json!({"stopReason":"end_turn"})),
                (
                    "metadata",
                    json!({"usage":{"inputTokens":2,"outputTokens":3,"totalTokens":5}}),
                ),
            ]
            .into_iter()
            .flat_map(|(kind, value)| bedrock::frame(kind, value))
            .collect(),
            "mistral-conversations" => sse(vec![
                json!({"choices":[{"delta":{"content":[{"type":"thinking","thinking":[{"type":"text","text":"reason"}]},{"type":"text","text":"native reply"}]},"finish_reason":"stop"}],"usage":{"prompt_tokens":2,"completion_tokens":3,"total_tokens":5}}),
            ]),
            _ => sse(vec![
                json!({"type":"response.output_text.delta","delta":"native reply"}),
                json!({"type":"response.completed","response":{"id":"r","model":"fixture-model","status":"completed","output":[],"usage":{"input_tokens":2,"output_tokens":3,"total_tokens":5}}}),
            ]),
        };
        let requests = Captured::default();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let app = Router::new().fallback(post(handler)).with_state(Fixture {
            requests: requests.clone(),
            response,
        });
        let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let mut p = provider(id, api, &format!("http://{address}{base}"));
        if api == "openai-codex-responses" {
            p.provider.api_key = format!(
                "header.{}.signature",
                base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(
                    json!({"https://api.openai.com/auth":{"chatgpt_account_id":"fixture-account"}})
                        .to_string()
                )
            );
        }
        let response = OpenAIClient::new()
            .send_message(&p, vec![user()], Some("system"), vec![], 4096)
            .await
            .unwrap_or_else(|e| panic!("{api}: {e:#}"));
        assert!(
            matches!(&response.choices[0].message.content,OpenAIContent::Text(s) if s=="native reply"),
            "{api}"
        );
        assert_eq!(response.usage.unwrap().total_tokens, 5, "{api}");
        let captured = requests.lock().unwrap();
        assert_eq!(captured[0].0, path, "{api}");
        let headers = &captured[0].1;
        let body = &captured[0].2;
        match api {
            "anthropic-messages" => {
                assert_eq!(headers["x-api-key"], "fixture-only");
                assert_eq!(body["max_tokens"], 4096);
                assert!(body["system"] == "system" || body["system"][0]["text"] == "system");
            }
            "google-generative-ai" | "google-vertex" => {
                assert_eq!(headers["x-goog-api-key"], "fixture-only");
                assert_eq!(body["generationConfig"]["maxOutputTokens"], 4096);
            }
            "azure-openai-responses" => {
                assert_eq!(headers["api-key"], "fixture-only");
                assert!(!headers.contains_key("authorization"));
            }
            "openai-codex-responses" => {
                assert_eq!(headers["chatgpt-account-id"], "fixture-account");
                assert!(body.get("max_output_tokens").is_none());
            }
            "bedrock-converse-stream" => {
                assert_eq!(headers["authorization"], "Bearer fixture-only");
                assert_eq!(body["inferenceConfig"]["maxTokens"], 4096);
            }
            _ => {
                assert_eq!(headers["authorization"], "Bearer fixture-only");
            }
        }
        server.abort();
    }
}
