//! Real executable + local model fixture. No Desktop, paid API or user config.
use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::post,
    Json, Router,
};
use serde_json::{json, Value};
use std::{
    path::Path,
    process::{Output, Stdio},
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::process::Command;

#[derive(Clone)]
struct Fixture {
    requests: Arc<Mutex<Vec<Value>>>,
    tool: Option<String>,
    fail: bool,
}

async fn respond(State(state): State<Fixture>, Json(body): Json<Value>) -> Response {
    state.requests.lock().unwrap().push(body.clone());
    if state.fail {
        return (StatusCode::BAD_REQUEST, "fixture failure").into_response();
    }
    let last = body["messages"].as_array().unwrap().last().unwrap();
    let (delta, finish) = if let Some(tool) =
        state.tool.as_deref().filter(|_| last["role"] != "tool")
    {
        let args = if tool == "Read" {
            json!({"file_path":"fixture.txt"})
        } else {
            json!({"file_path":"created.txt","content":"written by shared core"})
        };
        (
            json!({"tool_calls":[{"index":0,"id":"call-fixture","type":"function","function":{"name":tool,"arguments":args.to_string()}}]}),
            "tool_calls",
        )
    } else {
        let content = if last["role"] == "tool" {
            format!("tool completed: {}", last["content"].as_str().unwrap())
        } else {
            "fixture response 中文".to_string()
        };
        (json!({"content":content}), "stop")
    };
    let chunks = [
        json!({"id":"fixture","model":"fixture-model","choices":[{"index":0,"delta":delta,"finish_reason":null}]}),
        json!({"id":"fixture","model":"fixture-model","choices":[{"index":0,"delta":{},"finish_reason":finish}],"usage":{"prompt_tokens":12,"completion_tokens":8}}),
    ];
    let body = chunks
        .iter()
        .map(|v| format!("data: {v}\n\n"))
        .collect::<String>()
        + "data: [DONE]\n\n";
    ([("content-type", "text/event-stream")], body).into_response()
}

async fn server(tool: Option<&str>, fail: bool) -> (String, Fixture, tokio::task::JoinHandle<()>) {
    let fixture = Fixture {
        requests: Arc::new(Mutex::new(vec![])),
        tool: tool.map(String::from),
        fail,
    };
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let app = Router::new()
        .route("/v1/chat/completions", post(respond))
        .with_state(fixture.clone());
    let task = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    (url, fixture, task)
}

fn command(data: &Path, project: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_claude-code"));
    command.args([
        "--data-dir",
        data.to_str().unwrap(),
        "--path",
        project.to_str().unwrap(),
    ]);
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    for key in [
        "ANTHROPIC_API_KEY",
        "OPENAI_API_KEY",
        "DEEPSEEK_API_KEY",
        "API_BASE_URL",
        "OPENAI_BASE_URL",
        "ANTHROPIC_BASE_URL",
        "CLAUDE_MODEL",
        "RUST_LOG",
    ] {
        command.env_remove(key);
    }
    command.env("CLAUDE_FIXTURE_KEY", "fixture-only-secret");
    command
}

async fn execute(mut command: Command) -> Output {
    tokio::time::timeout(Duration::from_secs(30), command.output())
        .await
        .expect("CLI hung")
        .unwrap()
}

#[tokio::test]
async fn builtin_provider_and_shared_tools_run_without_node_on_path() {
    let temp = tempfile::tempdir().unwrap();
    let data = temp.path().join("data");
    std::fs::create_dir(&data).unwrap();
    std::fs::write(temp.path().join("fixture.txt"),"native-only fixture").unwrap();
    let (url, fixture, task) = server(Some("Read"), false).await;
    let providers = json!([{"id":"builtin","name":"Built-in DeepSeek","base_url":format!("{url}/v1"),"api_key":"fixture-only-secret","api_format":"openai","builtin_id":"deepseek","enabled":true,"models":[{"id":"fixture-model","name":"Fixture","enabled":true,"supports_vision":false,"supports_web_search":false,"max_tokens":8192,"api":"openai-completions"}]}]);
    std::fs::write(data.join("providers.json"),providers.to_string()).unwrap();
    let mut cli = command(&data,temp.path());
    cli.env("PATH","").args(["--provider","builtin","--model","fixture-model","--api-key-env","CLAUDE_FIXTURE_KEY","--output-format","json","--permission-mode","accept-edits","query","-p","Read the fixture"]);
    let output = execute(cli).await;
    assert!(output.status.success(),"{}",String::from_utf8_lossy(&output.stderr));
    let output: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(output["result"].as_str().unwrap().contains("native-only fixture"));
    assert_eq!(fixture.requests.lock().unwrap().len(),2);
    task.abort();
}

fn query(data: &Path, project: &Path, url: &str) -> Command {
    let mut command = command(data, project);
    command.args([
        "--model",
        "fixture-model",
        "--api-format",
        "openai",
        "--base-url",
        url,
        "--api-key-env",
        "CLAUDE_FIXTURE_KEY",
        "--output-format",
        "json",
    ]);
    command
}

#[tokio::test]
async fn agent_alias_custom_definition_and_readonly_limit_use_shared_tool_loop() {
    let temp = tempfile::tempdir().unwrap();
    let data = temp.path().join("data");
    let (url, fixture, task) = server(Some("Write"), false).await;
    let mut cli = query(&data,temp.path(),&url);
    cli.args(["--permission-mode","bypass","agent","--agent-type","claude-code-guide","--prompt","Inspect project"]);
    let output = execute(cli).await;
    assert!(output.status.success(),"{}",String::from_utf8_lossy(&output.stderr));
    assert!(!temp.path().join("created.txt").exists());
    let first: Value = serde_json::from_slice(&output.stdout).unwrap();
    {
        let requests = fixture.requests.lock().unwrap();
        assert!(requests[0]["messages"][0]["content"].as_str().unwrap().contains("Agent role (guide)"));
        assert!(!requests[0]["tools"].as_array().unwrap().iter().any(|t|t["function"]["name"]=="Write"));
        assert!(requests[1]["messages"].as_array().unwrap().iter().any(|m|m["role"]=="tool" && m["content"].as_str().unwrap().contains("not allowed")));
    }
    let mut cli = query(&data,temp.path(),&url);
    cli.args(["--resume",first["session_id"].as_str().unwrap(),"--permission-mode","bypass","query","-p","Continue inspection"]);
    assert!(execute(cli).await.status.success());
    assert!(!temp.path().join("created.txt").exists());
    let source = temp.path().join("agent.json");
    std::fs::write(&source,json!({"id":"custom","description":"Custom reader","system_prompt":"Report CUSTOM_SENTINEL.","tools":["Read"]}).to_string()).unwrap();
    let mut cli = command(&data,temp.path()); cli.args(["agents","import",source.to_str().unwrap()]);
    assert!(execute(cli).await.status.success());
    let mut cli = query(&data,temp.path(),&url); cli.args(["agent","--agent-type","custom","--prompt","Inspect"]);
    assert!(execute(cli).await.status.success());
    assert!(!temp.path().join("created.txt").exists());
    let mut cli = query(&data,temp.path(),&url); cli.args(["agent","--agent-type","missing","--prompt","Inspect"]);
    assert!(!execute(cli).await.status.success());
    task.abort();
}

#[tokio::test]
async fn single_query_and_resume_share_sqlite_without_desktop_or_saved_credentials() {
    let temp = tempfile::tempdir().unwrap();
    let data = temp.path().join("data");
    let project = temp.path().join("project");
    std::fs::create_dir(&project).unwrap();
    let (url, fixture, task) = server(None, false).await;
    let mut cli = query(&data, &project, &url);
    cli.args(["query", "-p", "first prompt"]);
    let first = execute(cli).await;
    assert!(
        first.status.success(),
        "{}",
        String::from_utf8_lossy(&first.stderr)
    );
    let first: Value = serde_json::from_slice(&first.stdout).unwrap();
    assert_eq!(first["result"], "fixture response 中文");
    assert_eq!(first["is_error"], false);
    assert!(!data.join("providers.json").exists());
    assert!(!data.join("config.json").exists());
    let mut cli = query(&data, &project, &url);
    cli.args([
        "--resume",
        first["session_id"].as_str().unwrap(),
        "query",
        "-p",
        "second prompt",
    ]);
    let second = execute(cli).await;
    assert!(
        second.status.success(),
        "{}",
        String::from_utf8_lossy(&second.stderr)
    );
    let requests = fixture.requests.lock().unwrap();
    assert_eq!(requests.len(), 2);
    assert!(requests[1]["messages"]
        .as_array()
        .unwrap()
        .iter()
        .any(|m| m["content"] == "first prompt"));
    assert!(requests[1]["messages"]
        .as_array()
        .unwrap()
        .iter()
        .any(|m| m["content"] == "fixture response 中文"));
    assert!(requests[0]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .any(|t| t["function"]["description"]
            .as_str()
            .unwrap()
            .contains("Code mode:")));
    task.abort();
}

#[tokio::test]
async fn native_read_and_write_tools_operate_in_selected_project() {
    for tool in ["Read", "Write"] {
        let temp = tempfile::tempdir().unwrap();
        let data = temp.path().join("data");
        std::fs::write(temp.path().join("fixture.txt"), "source content 中文").unwrap();
        let (url, fixture, task) = server(Some(tool), false).await;
        let mut cli = query(&data, temp.path(), &url);
        cli.args([
            "--permission-mode",
            "accept-edits",
            "query",
            "-p",
            "perform task",
        ]);
        let output = execute(cli).await;
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let result: Value = serde_json::from_slice(&output.stdout).unwrap_or_else(|e|panic!("{e}; stderr: {}",String::from_utf8_lossy(&output.stderr)));
        assert!(result["result"]
            .as_str()
            .unwrap()
            .contains("tool completed"));
        assert_eq!(fixture.requests.lock().unwrap().len(), 2);
        if tool == "Read" {
            assert!(result["result"]
                .as_str()
                .unwrap()
                .contains("source content 中文"));
        } else {
            assert_eq!(
                std::fs::read_to_string(temp.path().join("created.txt")).unwrap(),
                "written by shared core"
            );
        }
        let session_id = result["session_id"].as_str().unwrap();
        let mut resumed = query(&data, temp.path(), &url);
        resumed.args([
            "--resume",
            session_id,
            "--permission-mode",
            "accept-edits",
            "query",
            "-p",
            "continue",
        ]);
        let resumed = execute(resumed).await;
        assert!(
            resumed.status.success(),
            "{}",
            String::from_utf8_lossy(&resumed.stderr)
        );
        let requests = fixture.requests.lock().unwrap();
        let history = requests[2]["messages"].as_array().unwrap();
        assert!(history
            .iter()
            .any(|m| m["role"] == "tool" && m["tool_call_id"].as_str().is_some()));
        assert!(history
            .iter()
            .any(|m| m["role"] == "assistant" && m["tool_calls"].is_array()));
        task.abort();
    }
}

#[tokio::test]
async fn noninteractive_approval_and_api_errors_fail_without_hanging() {
    for fail in [false, true] {
        let temp = tempfile::tempdir().unwrap();
        let data = temp.path().join("data");
        let (url, _, task) = server(Some("Write"), fail).await;
        let mut cli = query(&data, temp.path(), &url);
        cli.args(["query", "-p", "perform task"]);
        let output = execute(cli).await;
        assert!(!output.status.success());
        assert!(!temp.path().join("created.txt").exists());
        let result: Value = serde_json::from_slice(&output.stdout).unwrap_or_else(|e|panic!("{e}; stderr: {}",String::from_utf8_lossy(&output.stderr)));
        assert_eq!(result["is_error"], true);
        let error = result["error"].as_str().unwrap();
        assert!(
            error.contains(if fail {
                "fixture failure"
            } else {
                "approval required"
            }),
            "{error}"
        );
        task.abort();
    }
}

#[tokio::test]
async fn piped_input_stream_json_and_legacy_import_are_scriptable() {
    use tokio::io::AsyncWriteExt;
    let temp = tempfile::tempdir().unwrap();
    let data = temp.path().join("data");
    let (url, fixture, task) = server(None, false).await;
    let mut cli = query(&data, temp.path(), &url);
    cli.args(["--output-format", "stream-json", "--print"])
        .stdin(Stdio::piped());
    let mut child = cli.spawn().unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all("piped 中文".as_bytes())
        .await
        .unwrap();
    let output = tokio::time::timeout(Duration::from_secs(30), child.wait_with_output())
        .await
        .unwrap()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let events: Vec<Value> = String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .map(|s| serde_json::from_str(s).unwrap())
        .collect();
    assert!(events.iter().any(|v| v["type"] == "text"));
    assert_eq!(events.last().unwrap()["type"], "result");
    assert!(fixture.requests.lock().unwrap()[0]["messages"]
        .as_array()
        .unwrap()
        .iter()
        .any(|m| m["content"] == "piped 中文"));
    task.abort();
    let old = temp.path().join("settings.json");
    let original = json!({"model":"deepseek-chat","api":{"base_url":"https://api.deepseek.com","api_key":"legacy-only-secret","max_tokens":4096}}).to_string();
    std::fs::write(&old, &original).unwrap();
    let mut cli = command(&data, temp.path());
    cli.args(["config", "import-legacy", old.to_str().unwrap()]);
    let imported = execute(cli).await;
    assert!(
        imported.status.success(),
        "{}",
        String::from_utf8_lossy(&imported.stderr)
    );
    assert_eq!(std::fs::read_to_string(old).unwrap(), original);
    let mut cli = command(&data, temp.path());
    cli.args(["config", "show"]);
    let shown = execute(cli).await;
    assert!(shown.status.success());
    let shown = String::from_utf8(shown.stdout).unwrap();
    assert!(!shown.contains("legacy-only-secret"));
    assert!(shown.contains("[redacted]"));
}

#[tokio::test]
async fn empty_stdin_and_malformed_config_fail_without_overwriting() {
    let temp = tempfile::tempdir().unwrap();
    let data = temp.path().join("data");
    let output = execute(command(&data, temp.path())).await;
    assert!(!output.status.success());
    assert!(!data.exists());
    std::fs::create_dir(&data).unwrap();
    std::fs::write(data.join("config.json"), "not json").unwrap();
    let mut cli = command(&data, temp.path());
    cli.args(["config", "set", "model", "sonnet"]);
    assert!(!execute(cli).await.status.success());
    assert_eq!(
        std::fs::read_to_string(data.join("config.json")).unwrap(),
        "not json"
    );
}

#[tokio::test]
async fn legacy_agent_import_preserves_allowlist_in_real_shared_engine() {
    let temp = tempfile::tempdir().unwrap();
    let data = temp.path().join("data");
    let source = temp.path().join("old-agent.json");
    let raw = json!({"agent_type":"Custom","name":"Old Reviewer","description":"Read only","when_to_use":"Review","tools":["file_read"],"model":"old-model","system_prompt":"LEGACY_SENTINEL","source":"user","base_dir":"unused-old-path"}).to_string();
    std::fs::write(&source, &raw).unwrap();
    let mut cli = command(&data, temp.path());
    cli.args([
        "agents",
        "import-legacy",
        source.to_str().unwrap(),
        "--id",
        "LegacyReader",
        "--preview",
    ]);
    let preview = execute(cli).await;
    assert!(preview.status.success());
    let mut cli = command(&data, temp.path());
    cli.args(["agents", "list"]);
    assert!(!String::from_utf8_lossy(&execute(cli).await.stdout).contains("LegacyReader"));
    for _ in 0..2 {
        let mut cli = command(&data, temp.path());
        cli.args([
            "agents",
            "import-legacy",
            source.to_str().unwrap(),
            "--id",
            "LegacyReader",
        ]);
        assert!(execute(cli).await.status.success());
    }
    let (url, fixture, task) = server(Some("Write"), false).await;
    let mut cli = query(&data, temp.path(), &url);
    cli.args([
        "--permission-mode",
        "bypass",
        "agent",
        "--agent-type",
        "LegacyReader",
        "--prompt",
        "Review",
    ]);
    let output = execute(cli).await;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!temp.path().join("created.txt").exists());
    let requests = fixture.requests.lock().unwrap();
    assert_eq!(requests[0]["model"], "fixture-model");
    assert!(requests[0]["messages"][0]["content"]
        .as_str()
        .unwrap()
        .contains("LEGACY_SENTINEL"));
    assert!(!requests[0]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .any(|t| t["function"]["name"] == "Write"));
    assert!(requests[1]["messages"]
        .as_array()
        .unwrap()
        .iter()
        .any(|m| m["role"] == "tool" && m["content"].as_str().unwrap().contains("not allowed")));
    assert_eq!(std::fs::read_to_string(source).unwrap(), raw);
    drop(requests);
    task.abort();
    std::fs::write(temp.path().join("fixture.txt"), "READ_ALLOWED_SENTINEL").unwrap();
    let (url, _, task) = server(Some("Read"), false).await;
    let mut cli = query(&data, temp.path(), &url);
    cli.args([
        "agent",
        "--agent-type",
        "LegacyReader",
        "--prompt",
        "Read fixture",
    ]);
    let output = execute(cli).await;
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("Tool approval required"));
    let mut cli = query(&data, temp.path(), &url);
    cli.args(["--permission-mode", "accept-edits", "agent", "--agent-type", "LegacyReader", "--prompt", "Read fixture"]);
    let output = execute(cli).await;
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert!(String::from_utf8_lossy(&output.stdout).contains("READ_ALLOWED_SENTINEL"));
    task.abort();
}
