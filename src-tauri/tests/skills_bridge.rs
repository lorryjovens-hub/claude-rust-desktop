use claude_desktop_tauri_lib::{
    bridge::BridgeServer,
    runtime::{RuntimePaths, SessionStore},
    skills::SkillsManager,
};
use serde_json::{json, Value};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

#[tokio::test]
async fn skills_bridge_streams_with_permissions_rejects_disabled_and_cancels_disconnect() {
    claude_desktop_tauri_lib::config::OrchestrationConfig::init_global().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let count = Arc::new(AtomicUsize::new(0));
    let calls = count.clone();
    let app = axum::Router::new().route("/v1/chat/completions", axum::routing::post(move |axum::Json(body):axum::Json<Value>| { let calls = calls.clone(); async move {
        calls.fetch_add(1, Ordering::SeqCst);
        let messages = body["messages"].as_array().unwrap();
        assert!(messages[0]["content"].as_str().unwrap().contains("bridge skill marker"));
        let cancel = messages.iter().any(|m| m["role"] == "user" && m["content"] == "cancel this");
        let (delta, finish) = if messages.iter().any(|m| m["role"] == "tool") {
            (json!({"content":"skill execution completed"}), "stop")
        } else {
            let (shell, command) = if cfg!(windows) {
                ("powershell", if cancel { "Set-Content started.txt ready; Start-Sleep -Milliseconds 2000; Set-Content late.txt unexpected" } else { "Set-Content approved.txt done" })
            } else { ("sh", if cancel { "echo ready > started.txt; sleep 2; echo unexpected > late.txt" } else { "echo done > approved.txt" }) };
            (json!({"content":"partial skill evidence", "tool_calls":[{"index":0,"id":"bash-call","type":"function","function":{"name":"Bash","arguments":json!({"shell":shell,"command":command,"timeout":10}).to_string()}}]}), "tool_calls")
        };
        let stream = format!("data: {}\n\ndata: {}\n\ndata: [DONE]\n\n",json!({"choices":[{"index":0,"delta":delta,"finish_reason":null}]}),json!({"choices":[{"index":0,"delta":{},"finish_reason":finish}]}));
        ([("content-type","text/event-stream")],stream)
    }}));
    let model_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let model_addr = model_listener.local_addr().unwrap();
    let model = tokio::spawn(async move {
        axum::serve(model_listener, app).await.unwrap();
    });
    let config = json!({"providers":[{"id":"fixture","name":"Fixture","provider_type":"openai","api_key":"test","base_url":format!("http://{model_addr}"),"enabled":true,"is_default":true,"settings":{},"supports_web_search":false,"models":[{"id":"fixture-model","name":"Fixture","enabled":true,"max_tokens":1000,"supports_vision":false,"supports_tools":true,"supports_streaming":true}]}]});
    std::fs::write(dir.path().join("config.json"), config.to_string()).unwrap();
    let skills = SkillsManager::with_path(dir.path().join("skills"));
    let skill = skills
        .create_skill(
            "bridge-fixture",
            "fixture",
            "---\nname: bridge-fixture\ndescription: fixture\n---\nbridge skill marker",
        )
        .unwrap();
    std::fs::write(dir.path().join("outside-secret.txt"), "private").unwrap();
    let paths = RuntimePaths::new(dir.path().into()).unwrap();
    let db = paths.open_database().unwrap();
    let sessions = SessionStore::new(db.clone());
    let conversation = sessions
        .create(dir.path(), "fixture-model", "fixture")
        .unwrap();
    let bridge = BridgeServer::new(dir.path().into(), db.clone());
    let key = bridge.get_api_key().to_string();
    let reserved = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = reserved.local_addr().unwrap().port();
    drop(reserved);
    let server = tokio::spawn(async move {
        bridge.start(port).await.unwrap();
    });
    let base = format!("http://127.0.0.1:{port}");
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(20))
        .build()
        .unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(10), async {
        loop {
            if client.get(format!("{base}/health")).send().await.is_ok() {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
    let endpoint = format!("{base}/api/skills/{}", urlencoding::encode(&skill.id));
    let escape = client
        .get(format!("{endpoint}/file"))
        .header("x-api-key", &key)
        .query(&[("path", "../../outside-secret.txt")])
        .send()
        .await
        .unwrap();
    assert_eq!(escape.status(), reqwest::StatusCode::BAD_REQUEST);
    let disabled = client
        .patch(format!("{endpoint}/toggle"))
        .header("x-api-key", &key)
        .json(&json!({"enabled":false}))
        .send()
        .await
        .unwrap();
    assert_eq!(disabled.json::<Value>().await.unwrap()["ok"], true);
    let request = json!({"input":"do this","conversation_id":conversation,"model":"fixture-model","provider_id":"fixture","workspace_path":dir.path()});
    let rejected = client
        .post(format!("{endpoint}/execute"))
        .header("x-api-key", &key)
        .json(&request)
        .send()
        .await
        .unwrap();
    assert_eq!(rejected.status(), reqwest::StatusCode::BAD_REQUEST);
    assert_eq!(count.load(Ordering::SeqCst), 0);
    client
        .patch(format!("{endpoint}/toggle"))
        .header("x-api-key", &key)
        .json(&json!({"enabled":true}))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap();
    client
        .post(format!("{base}/api/conversations/{conversation}/warm"))
        .header("x-api-key", &key)
        .json(&json!({"permission_mode":"ask_permissions"}))
        .send()
        .await
        .unwrap();
    let mut response = client
        .post(format!("{endpoint}/execute"))
        .header("x-api-key", &key)
        .json(&request)
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap();
    assert!(response.headers()["content-type"]
        .to_str()
        .unwrap()
        .starts_with("text/event-stream"));
    let mut buffer = String::new();
    let mut approved = false;
    while let Some(chunk) = response.chunk().await.unwrap() {
        buffer.push_str(&String::from_utf8_lossy(&chunk));
        if !approved {
            for line in buffer
                .lines()
                .filter_map(|line| line.strip_prefix("data: "))
            {
                if let Ok(event) = serde_json::from_str::<Value>(line) {
                    if event["type"] == "tool_permission" {
                        let result = client.post(format!("{base}/api/conversations/{conversation}/permission")).header("x-api-key", &key).json(&json!({"request_id":event["request_id"],"tool_use_id":event["tool_use_id"],"behavior":"allow"})).send().await.unwrap();
                        assert!(result.status().is_success());
                        approved = true;
                        break;
                    }
                }
            }
        }
    }
    assert!(approved && buffer.contains("skill execution completed"));
    assert!(dir.path().join("approved.txt").exists());
    let cancel_id = sessions
        .create(dir.path(), "fixture-model", "fixture")
        .unwrap();
    client
        .post(format!("{base}/api/conversations/{cancel_id}/warm"))
        .header("x-api-key", &key)
        .json(&json!({"permission_mode":"bypass_permissions"}))
        .send()
        .await
        .unwrap();
    let response = client.post(format!("{endpoint}/execute")).header("x-api-key", &key).json(&json!({"input":"cancel this","conversation_id":cancel_id,"model":"fixture-model","provider_id":"fixture","workspace_path":dir.path()})).send().await.unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(10), async {
        while !dir.path().join("started.txt").exists() {
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
    drop(response);
    tokio::time::sleep(std::time::Duration::from_millis(2500)).await;
    assert!(
        !dir.path().join("late.txt").exists(),
        "SSE disconnect left the child process running"
    );
    let rows = db
        .with_conn(|conn| {
            claude_desktop_tauri_lib::db::message_repo::get_messages_by_conversation(
                conn, &cancel_id,
            )
        })
        .unwrap()
        .unwrap();
    assert!(
        rows.iter()
            .any(|row| row.role == "assistant" && row.content.contains("partial skill evidence")),
        "Disconnect lost the partial assistant trajectory"
    );
    server.abort();
    model.abort();
}
