use claude_core::{
    native_engine::{
        engine_core::ChatRequest,
        provider_manager::{Provider, ProviderManager},
    },
    runtime::{self, RuntimePaths, SessionStore},
    services::{agents, EnvironmentCredentials, ServiceContext},
};
use serde_json::{json, Value};
use std::sync::Arc;

fn stream(content: Value, finish: &str) -> String {
    format!(
        "data: {}\n\ndata: {}\n\ndata: [DONE]\n\n",
        json!({"choices":[{"index":0,"delta":content,"finish_reason":null}]}),
        json!({"choices":[{"index":0,"delta":{},"finish_reason":finish}]})
    )
}
fn call(id: &str, name: &str, input: Value, index: usize) -> Value {
    json!({"index":index,"id":id,"type":"function","function":{"name":name,"arguments":input.to_string()}})
}

#[tokio::test]
async fn child_uses_real_tools_persists_and_cannot_expand_parent_allowlist() {
    run_case(false, false).await;
}

#[tokio::test]
async fn cancelling_parent_stops_child_shell_and_marks_child_cancelled() {
    run_case(true, false).await;
}

#[tokio::test]
async fn team_run_executes_and_persists_real_child_results() {
    run_case(false, true).await;
}
#[tokio::test]
async fn cancelling_team_run_fails_claim_and_stops_child() {
    run_case(true, true).await;
}

async fn run_case(cancel: bool, team: bool) {
    claude_core::config::OrchestrationConfig::init_global().unwrap();
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("evidence.txt"), "verified child read").unwrap();
    let workspace = dir.path().to_string_lossy().into_owned();
    let team_id = if team {
        claude_core::tools::coordination::execute(
            "team",
            json!({"operation":"create","name":"test team","tasks":[{"prompt":"inspect child"}]}),
            &workspace,
        )
        .unwrap()["team_id"]
            .as_str()
            .unwrap()
            .to_owned()
    } else {
        String::new()
    };
    let mock_team = team_id.clone();
    let mock_workspace = workspace.clone();
    let seen = Arc::new(std::sync::Mutex::new(Vec::<Value>::new()));
    let captured = seen.clone();
    let app = axum::Router::new().route("/v1/chat/completions", axum::routing::post(move |axum::Json(body):axum::Json<Value>| {
        let workspace = mock_workspace.clone(); let team_id = mock_team.clone(); let captured = captured.clone(); async move {
            captured.lock().unwrap().push(body.clone());
            let messages = body["messages"].as_array().unwrap();
            let child = messages.iter().any(|m| m["role"] == "user" && m["content"] == "inspect child");
            let has_tools = messages.iter().any(|m| m["role"] == "tool");
            let response = if !child && !has_tools {
                if team { stream(json!({"tool_calls":[call("team-1","team",json!({"operation":"run","team_id":team_id}),0)]}), "tool_calls") }
                else { stream(json!({"tool_calls":[call("delegate-1","Agent",json!({"prompt":"inspect child","agent_type":"general"}),0)]}), "tool_calls") }
            } else if child && !has_tools && cancel {
                let (shell, command) = if cfg!(windows) { ("powershell", "Set-Content child-started.txt ready; Start-Sleep -Milliseconds 1500; Set-Content child-finished.txt unexpected") } else { ("sh", "echo ready > child-started.txt; sleep 1.5; echo unexpected > child-finished.txt") };
                stream(json!({"tool_calls":[call("shell-1","Bash",json!({"shell":shell,"command":command,"timeout":10}),0)]}), "tool_calls")
            } else if child && !has_tools {
                stream(json!({"tool_calls":[call("read-1","Read",json!({"file_path":format!("{workspace}/evidence.txt")}),0),call("write-1","Write",json!({"file_path":format!("{workspace}/forbidden.txt"),"content":"bad"}),1)]}), "tool_calls")
            } else { stream(json!({"content":"completed with evidence"}), "stop") };
            ([("content-type","text/event-stream")], response)
        }
    }));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    let paths = RuntimePaths::new(dir.path().into()).unwrap();
    let context = ServiceContext::open(paths.clone(), Arc::new(EnvironmentCredentials)).unwrap();
    let provider: Provider = serde_json::from_value(json!({"id":"local","name":"Local","base_url":format!("http://{addr}"),"api_key":"fixture","api_format":"openai","enabled":true,"models":[{"id":"fixture","name":"Fixture","enabled":true,"max_tokens":1000,"supports_vision":false,"supports_web_search":false}]})).unwrap();
    let manager = Arc::new(tokio::sync::Mutex::new(ProviderManager::in_memory(
        vec![provider],
        dir.path().join("providers.json"),
    )));
    let permissions = runtime::default_permissions();
    permissions.set_mode(claude_core::permissions::PermissionMode::BypassPermissions);
    let engine = runtime::create_engine(
        &paths,
        context.db.clone(),
        manager,
        permissions,
        Arc::new(claude_core::cache::FileCache::new()),
    );
    let sessions = SessionStore::new(context.db.clone());
    let id = sessions.create(dir.path(), "fixture", "local").unwrap();
    agents::bind(
        &context.db,
        &id,
        &agents::Definition {
            id: "restricted".into(),
            description: "test".into(),
            system_prompt: "Inspect evidence".into(),
            tools: Some(if cancel {
                vec![
                    if team { "team".into() } else { "Agent".into() },
                    "Bash".into(),
                ]
            } else {
                vec![
                    if team { "team".into() } else { "Agent".into() },
                    "Read".into(),
                ]
            }),
            legacy: None,
        },
    )
    .unwrap();
    let mut events = engine
        .send_message(ChatRequest {
            conversation_id: id.clone(),
            messages: vec![json!({"role":"user","content":"Delegate inspection"})],
            model: "fixture".into(),
            provider_id: Some("local".into()),
            effort: None,
            system_prompt: None,
            max_tokens: Some(1000),
            workspace_path: Some(workspace),
            temperature: None,
            top_p: None,
            reasoning_mode: None,
        })
        .await
        .unwrap();
    if cancel {
        tokio::time::timeout(std::time::Duration::from_secs(10), async {
            while !dir.path().join("child-started.txt").exists() {
                tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            }
        })
        .await
        .unwrap();
        engine.cancel_turn(&id).await;
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        assert!(!dir.path().join("child-finished.txt").exists());
        let raw: String = context
            .db
            .with_conn(|conn| {
                conn.query_row(
                    "SELECT value FROM peripheral_settings WHERE key LIKE 'delegation.%'",
                    [],
                    |r| r.get(0),
                )
            })
            .unwrap()
            .unwrap();
        assert_eq!(
            serde_json::from_str::<Value>(&raw).unwrap()["status"],
            "cancelled"
        );
        if team {
            let record = claude_core::tools::coordination::execute(
                "team",
                json!({"operation":"get","team_id":team_id}),
                dir.path().to_str().unwrap(),
            )
            .unwrap();
            assert_eq!(record["tasks"][0]["status"], "failed");
            assert!(record["tasks"][0]["error"]
                .as_str()
                .unwrap()
                .contains("cancelled"));
        }
        server.abort();
        return;
    }
    let mut delegated = None;
    tokio::time::timeout(std::time::Duration::from_secs(15), async {
        while let Some(event) = events.recv().await {
            if let claude_core::native_engine::tool_loop::EngineEvent::ToolUseDone {
                tool_name,
                output,
                is_error,
                ..
            } = event
            {
                if tool_name == "Agent" || tool_name == "team" {
                    assert!(!is_error, "{output}");
                    delegated = Some(serde_json::from_str::<Value>(&output).unwrap());
                }
            }
        }
    })
    .await
    .unwrap();
    let delegated = delegated.unwrap();
    let delegated = if team {
        &delegated["completed_tasks"][0]["result"]
    } else {
        &delegated
    };
    let child_id = delegated["session_id"].as_str().unwrap().to_owned();
    assert_ne!(id, child_id);
    assert!(!dir.path().join("forbidden.txt").exists());
    let definition = agents::for_session(&context.db, &child_id)
        .unwrap()
        .unwrap();
    assert_eq!(
        definition.tools.unwrap(),
        [if team { "team" } else { "Agent" }, "Read"]
    );
    let rows = context
        .db
        .with_conn(|conn| {
            claude_core::db::message_repo::get_messages_by_conversation(conn, &child_id)
        })
        .unwrap()
        .unwrap();
    let mut results = Vec::new();
    for row in rows {
        results.extend(
            context
                .db
                .with_conn(|conn| {
                    claude_core::db::message_repo::get_tool_calls_for_message(conn, &row.id)
                })
                .unwrap()
                .unwrap(),
        );
    }
    assert!(results.iter().any(|r| r.name == "Read"
        && !r.is_error
        && r.output.as_deref().unwrap().contains("verified child read")));
    assert!(results.iter().any(|r| r.name == "Write" && r.is_error));
    assert_eq!(
        context
            .setting::<Value>(&format!("delegation.{child_id}"))
            .unwrap()
            .unwrap()["status"],
        "completed"
    );
    if team {
        let record = claude_core::tools::coordination::execute(
            "team",
            json!({"operation":"get","team_id":team_id}),
            dir.path().to_str().unwrap(),
        )
        .unwrap();
        assert_eq!(record["tasks"][0]["status"], "completed");
        assert!(record["tasks"][0]["result"]
            .as_str()
            .unwrap()
            .contains("completed with evidence"));
    }
    assert_eq!(seen.lock().unwrap().len(), 4);
    server.abort();
}
