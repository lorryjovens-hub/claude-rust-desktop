use crate::bridge::ChatRequest;
use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::{mpsc, RwLock};
use tokio::time::{timeout, Duration};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClaudeCodeConfig {
    pub claude_path: Option<String>,
    pub model: String,
    pub max_tokens: u32,
    pub temperature: Option<f32>,
    pub timeout_ms: u64,
    pub dangerously_skip_permissions: bool,
    pub no_input: bool,
    pub output_format: OutputFormat,
    pub resume: bool,
    pub max_retries: u32,
}

impl Default for ClaudeCodeConfig {
    fn default() -> Self {
        Self {
            claude_path: None,
            model: "sonnet-4-20250514".to_string(),
            max_tokens: 8192,
            temperature: None,
            timeout_ms: 120000,
            dangerously_skip_permissions: true,
            no_input: true,
            output_format: OutputFormat::StreamJson,
            resume: false,
            max_retries: 3,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum OutputFormat {
    StreamJson,
    Json,
    Text,
    VerboseJson,
}

impl Default for OutputFormat {
    fn default() -> Self {
        Self::StreamJson
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClaudeCodeSession {
    pub id: String,
    pub conv_id: String,
    pub model: String,
    pub working_directory: Option<String>,
    pub session_file: Option<String>,
    pub state: SessionState,
    pub message_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum SessionState {
    Created,
    Initializing,
    Running,
    WaitingForInput,
    Completed,
    Error,
    Terminated,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClaudeCodeResult {
    pub session_id: String,
    pub message_id: Option<String>,
    pub content: String,
    pub tool_results: Vec<ToolResult>,
    pub usage: Option<TokenUsage>,
    pub error: Option<String>,
    pub stop_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResult {
    pub tool: String,
    pub tool_call_id: String,
    pub content: String,
    pub success: bool,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_creation_tokens: Option<u64>,
    pub cache_read_tokens: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamEvent {
    pub event_type: String,
    pub data: serde_json::Value,
}

pub struct ClaudeCodeEngine {
    config: ClaudeCodeConfig,
    sessions: Arc<RwLock<HashMap<String, ClaudeCodeSession>>>,
    process_handles: Arc<RwLock<HashMap<String, Child>>>,
}

impl ClaudeCodeEngine {
    pub fn new(config: ClaudeCodeConfig) -> Self {
        Self {
            config,
            sessions: Arc::new(RwLock::new(HashMap::new())),
            process_handles: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub fn with_default_config() -> Self {
        Self::new(ClaudeCodeConfig::default())
    }

    pub async fn find_claude_code_path(&self) -> Option<PathBuf> {
        if let Some(ref path) = self.config.claude_path {
            let p = PathBuf::from(path);
            if p.exists() {
                return Some(p);
            }
        }

        let candidates = if cfg!(target_os = "windows") {
            vec![
                PathBuf::from(r"C:\Program Files\Claude\bin\claude.exe"),
                PathBuf::from(r"C:\Users\user\AppData\Local\claude\bin\claude.exe"),
                PathBuf::from(r"%APPDATA%\claude\bin\claude.exe"),
                PathBuf::from("claude.exe"),
            ]
        } else {
            vec![
                PathBuf::from("/usr/local/bin/claude"),
                PathBuf::from("/opt/claude/bin/claude"),
                PathBuf::from("~/claude/bin/claude"),
                PathBuf::from("claude"),
            ]
        };

        for path in &candidates {
            if path.exists() {
                return Some(path.clone());
            }
        }

        if let Ok(output) = std::process::Command::new("where").arg("claude").output() {
            if output.status.success() {
                let p = String::from_utf8_lossy(&output.stdout).trim().to_string();
                if !p.is_empty() {
                    return Some(PathBuf::from(p.split('\n').next().unwrap_or("")));
                }
            }
        }

        None
    }

    pub async fn create_session(
        &self,
        conv_id: &str,
        working_dir: Option<&str>,
    ) -> Result<ClaudeCodeSession> {
        let session_id = uuid::Uuid::new_v4().to_string();

        let session = ClaudeCodeSession {
            id: session_id.clone(),
            conv_id: conv_id.to_string(),
            model: self.config.model.clone(),
            working_directory: working_dir.map(|s| s.to_string()),
            session_file: None,
            state: SessionState::Created,
            message_count: 0,
        };

        let mut sessions = self.sessions.write().await;
        sessions.insert(session_id.clone(), session.clone());

        Ok(session)
    }

    pub async fn execute_prompt(
        &self,
        session: &mut ClaudeCodeSession,
        prompt: &str,
    ) -> Result<ClaudeCodeResult> {
        session.state = SessionState::Initializing;

        let claude_path = self
            .find_claude_code_path()
            .await
            .ok_or_else(|| anyhow!("Claude Code CLI not found"))?;

        let mut args = self.build_cli_args();

        if let Some(ref work_dir) = session.working_directory {
            args.push("--output-dir".to_string());
            args.push(work_dir.clone());
        }

        if self.config.resume {
            args.push("--resume".to_string());
        }

        if self.config.dangerously_skip_permissions {
            args.push("--dangerously-skip-permissions".to_string());
        }

        args.push("--".to_string());
        args.push(prompt.to_string());

        let mut cmd = Command::new(&claude_path);
        cmd.args(&args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .stdin(Stdio::piped());

        if let Some(ref work_dir) = session.working_directory {
            cmd.current_dir(work_dir);
        }

        let mut child = cmd.spawn().map_err(|e| anyhow!("Failed to spawn Claude: {}", e))?;

        session.state = SessionState::Running;

        let result = self
            .process_session_output(&mut child, session)
            .await;

        let mut sessions = self.sessions.write().await;
        if let Some(s) = sessions.get_mut(&session.id) {
            s.message_count += 1;
            s.state = match &result {
                Ok(_) => SessionState::Completed,
                Err(_) => SessionState::Error,
            };
        }

        result
    }

    pub async fn execute_streaming(
        &self,
        session: &mut ClaudeCodeSession,
        prompt: &str,
        tx: mpsc::Sender<StreamEvent>,
    ) -> Result<ClaudeCodeResult> {
        session.state = SessionState::Initializing;

        let claude_path = self
            .find_claude_code_path()
            .await
            .ok_or_else(|| anyhow!("Claude Code CLI not found"))?;

        let mut args = self.build_cli_args();
        args.push("--output-format".to_string());
        args.push("stream-json".to_string());

        if let Some(ref work_dir) = session.working_directory {
            args.push("--output-dir".to_string());
            args.push(work_dir.clone());
        }

        if self.config.resume {
            args.push("--resume".to_string());
        }

        if self.config.dangerously_skip_permissions {
            args.push("--dangerously-skip-permissions".to_string());
        }

        args.push("--".to_string());
        args.push(prompt.to_string());

        let mut cmd = Command::new(&claude_path);
        cmd.args(&args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .stdin(Stdio::piped());

        if let Some(ref work_dir) = session.working_directory {
            cmd.current_dir(work_dir);
        }

        let mut child = cmd.spawn().map_err(|e| anyhow!("Failed to spawn Claude: {}", e))?;

        session.state = SessionState::Running;

        let result = self
            .process_streaming_output(&mut child, session, tx)
            .await;

        let mut sessions = self.sessions.write().await;
        if let Some(s) = sessions.get_mut(&session.id) {
            s.message_count += 1;
            s.state = match &result {
                Ok(_) => SessionState::Completed,
                Err(_) => SessionState::Error,
            };
        }

        result
    }

    pub async fn execute_with_api(
        &self,
        session: &mut ClaudeCodeSession,
        req: &ChatRequest,
    ) -> Result<ClaudeCodeResult> {
        let prompt = self.build_prompt_from_messages(req)?;
        self.execute_prompt(session, &prompt).await
    }

    fn build_cli_args(&self) -> Vec<String> {
        let mut args = Vec::new();

        args.push("--model".to_string());
        args.push(self.config.model.clone());

        if self.config.max_tokens != 8192 {
            args.push("--max-tokens".to_string());
            args.push(self.config.max_tokens.to_string());
        }

        if let Some(temp) = self.config.temperature {
            args.push("--temperature".to_string());
            args.push(temp.to_string());
        }

        args.push("--output-format".to_string());
        match self.config.output_format {
            OutputFormat::StreamJson => args.push("stream-json".to_string()),
            OutputFormat::Json => args.push("json".to_string()),
            OutputFormat::Text => args.push("text".to_string()),
            OutputFormat::VerboseJson => args.push("verbose-json".to_string()),
        }

        args
    }

    fn build_prompt_from_messages(&self, req: &ChatRequest) -> Result<String> {
        let mut prompt = String::new();

        for msg in req.get_messages() {
            if let Some(role) = msg.get("role").and_then(|r| r.as_str()) {
                if let Some(content) = msg.get("content") {
                    match content {
                        serde_json::Value::String(s) => {
                            prompt.push_str(&format!("\n[{}]: {}\n", role, s));
                        }
                        serde_json::Value::Array(arr) => {
                            let mut text_content = String::new();
                            for item in arr {
                                if let Some(text) = item.get("text").and_then(|t| t.as_str()) {
                                    text_content.push_str(text);
                                } else if let Some(t) = item.get("type").and_then(|t| t.as_str()) {
                                    if t == "tool_use" {
                                        let name = item.get("name").and_then(|n| n.as_str()).unwrap_or("unknown");
                                        let tool_input = item.get("input").map(|i| i.to_string()).unwrap_or_default();
                                        text_content.push_str(&format!("\n[tool:{}] {}\n", name, tool_input));
                                    } else if t == "tool_result" {
                                        let content = item.get("content").and_then(|c| c.as_str()).unwrap_or("");
                                        text_content.push_str(&format!("\n[tool_result]: {}\n", content));
                                    }
                                }
                            }
                            prompt.push_str(&format!("\n[{}]: {}\n", role, text_content));
                        }
                        _ => {}
                    }
                }
            }
        }

        Ok(prompt.trim().to_string())
    }

    async fn process_session_output(
        &self,
        child: &mut Child,
        session: &mut ClaudeCodeSession,
    ) -> Result<ClaudeCodeResult> {
        let stdout = child.stdout.take().ok_or_else(|| anyhow!("No stdout"))?;
        let mut reader = BufReader::new(stdout).lines();

        let mut full_content = String::new();
        let mut tool_results = Vec::new();
        let mut message_id = None;
        let mut stop_reason = None;
        let mut error = None;

        let timeout_duration = Duration::from_millis(self.config.timeout_ms);

        loop {
            let line = match timeout(timeout_duration, reader.next_line()).await {
                Ok(Ok(Some(l))) => l,
                Ok(Ok(None)) => break,
                Ok(Err(e)) => {
                    error = Some(format!("Read error: {}", e));
                    break;
                }
                Err(_) => {
                    error = Some("Timeout".to_string());
                    break;
                }
            };

            if let Ok(event) = serde_json::from_str::<serde_json::Value>(&line) {
                let event_type = event
                    .get("type")
                    .and_then(|t| t.as_str())
                    .unwrap_or("unknown");

                match event_type {
                    "assistant" | "message" => {
                        if let Some(content) = event.get("content") {
                            if let Some(arr) = content.as_array() {
                                for item in arr {
                                    if let Some(text) = item.get("text").and_then(|t| t.as_str()) {
                                        full_content.push_str(text);
                                    }
                                    if item.get("type").and_then(|t| t.as_str()) == Some("tool_use") {
                                        let tool_name = item.get("name").and_then(|n| n.as_str()).unwrap_or("unknown");
                                        let tool_input = item.get("input").map(|i| i.to_string()).unwrap_or_default();
                                        let tool_call_id = item.get("id").and_then(|i| i.as_str()).unwrap_or("").to_string();

                                        tool_results.push(ToolResult {
                                            tool: tool_name.to_string(),
                                            tool_call_id,
                                            content: tool_input,
                                            success: true,
                                            error: None,
                                        });
                                    }
                                }
                            }
                        }
                        message_id = event.get("id").and_then(|id| id.as_str()).map(|s| s.to_string());
                    }
                    "content_block_delta" => {
                        if let Some(delta) = event.get("delta").and_then(|d| d.get("text")) {
                            if let Some(text) = delta.as_str() {
                                full_content.push_str(text);
                            }
                        }
                    }
                    "message_delta" | "message_stop" => {
                        if let Some(usage) = event.get("usage").or_else(|| event.get("delta").and_then(|d| d.get("usage"))) {
                            stop_reason = usage.get("stop_reason").and_then(|s| s.as_str()).map(|s| s.to_string());
                        }
                    }
                    "error" | "error_event" => {
                        error = event.get("error").and_then(|e| e.as_str()).map(|s| s.to_string());
                    }
                    "connection_error" | "authentication_error" | "rate_limit_error" => {
                        let err_type = event_type.replace("_error", "").replace("_event", "");
                        error = Some(format!("{}: {:?}", err_type, event));
                    }
                    _ => {}
                }
            } else if !line.trim().is_empty() {
                if full_content.is_empty() && !line.contains('{') {
                    full_content.push_str(&line);
                }
            }
        }

        child.wait().await.ok();

        Ok(ClaudeCodeResult {
            session_id: session.id.clone(),
            message_id,
            content: full_content,
            tool_results,
            usage: None,
            error,
            stop_reason,
        })
    }

    async fn process_streaming_output(
        &self,
        child: &mut Child,
        session: &mut ClaudeCodeSession,
        tx: mpsc::Sender<StreamEvent>,
    ) -> Result<ClaudeCodeResult> {
        let stdout = child.stdout.take().ok_or_else(|| anyhow!("No stdout"))?;
        let mut reader = BufReader::new(stdout).lines();

        let mut full_content = String::new();
        let mut tool_results = Vec::new();
        let mut message_id = None;
        let mut stop_reason = None;
        let mut error = None;

        let timeout_duration = Duration::from_millis(self.config.timeout_ms);

        loop {
            let line = match timeout(timeout_duration, reader.next_line()).await {
                Ok(Ok(Some(l))) => l,
                Ok(Ok(None)) => break,
                Ok(Err(e)) => {
                    error = Some(format!("Read error: {}", e));
                    break;
                }
                Err(_) => {
                    error = Some("Timeout".to_string());
                    break;
                }
            };

            if let Ok(event) = serde_json::from_str::<serde_json::Value>(&line) {
                let event_type = event
                    .get("type")
                    .and_then(|t| t.as_str())
                    .unwrap_or("unknown");

                let _ = tx
                    .send(StreamEvent {
                        event_type: event_type.to_string(),
                        data: event.clone(),
                    })
                    .await;

                match event_type {
                    "assistant" | "message" => {
                        if let Some(content) = event.get("content") {
                            if let Some(arr) = content.as_array() {
                                for item in arr {
                                    if let Some(text) = item.get("text").and_then(|t| t.as_str()) {
                                        full_content.push_str(text);
                                    }
                                    if item.get("type").and_then(|t| t.as_str()) == Some("tool_use") {
                                        let tool_name = item.get("name").and_then(|n| n.as_str()).unwrap_or("unknown");
                                        let tool_input = item.get("input").map(|i| i.to_string()).unwrap_or_default();
                                        let tool_call_id = item.get("id").and_then(|i| i.as_str()).unwrap_or("").to_string();

                                        tool_results.push(ToolResult {
                                            tool: tool_name.to_string(),
                                            tool_call_id,
                                            content: tool_input,
                                            success: true,
                                            error: None,
                                        });
                                    }
                                }
                            }
                        }
                        message_id = event.get("id").and_then(|id| id.as_str()).map(|s| s.to_string());
                    }
                    "content_block_delta" => {
                        if let Some(delta) = event.get("delta").and_then(|d| d.get("text")) {
                            if let Some(text) = delta.as_str() {
                                full_content.push_str(text);
                            }
                        }
                    }
                    "message_delta" | "message_stop" => {
                        if let Some(usage) = event.get("usage").or_else(|| event.get("delta").and_then(|d| d.get("usage"))) {
                            stop_reason = usage.get("stop_reason").and_then(|s| s.as_str()).map(|s| s.to_string());
                        }
                    }
                    "error" | "error_event" => {
                        error = event.get("error").and_then(|e| e.as_str()).map(|s| s.to_string());
                    }
                    _ => {}
                }
            }
        }

        child.wait().await.ok();

        Ok(ClaudeCodeResult {
            session_id: session.id.clone(),
            message_id,
            content: full_content,
            tool_results,
            usage: None,
            error,
            stop_reason,
        })
    }

    pub async fn send_tool_result(
        &self,
        session: &mut ClaudeCodeSession,
        tool_call_id: &str,
        result: &str,
    ) -> Result<()> {
        let mut processes = self.process_handles.write().await;
        if let Some(child) = processes.get_mut(&session.id) {
            if let Some(ref mut stdin) = child.stdin {
                let input = serde_json::json!({
                    "type": "tool_result",
                    "tool_call_id": tool_call_id,
                    "content": result,
                    "is_error": false
                });
                stdin
                    .write_all(format!("{}\n", input).as_bytes())
                    .await
                    .map_err(|e| anyhow!("Failed to send tool result: {}", e))?;
            }
        }
        Ok(())
    }

    pub async fn terminate_session(&self, session_id: &str) -> Result<()> {
        let mut sessions = self.sessions.write().await;
        if let Some(session) = sessions.get_mut(session_id) {
            session.state = SessionState::Terminated;
        }

        let mut processes = self.process_handles.write().await;
        if let Some(mut child) = processes.remove(session_id) {
            child.kill().await.ok();
        }

        Ok(())
    }

    pub async fn get_session(&self, session_id: &str) -> Option<ClaudeCodeSession> {
        let sessions = self.sessions.read().await;
        sessions.get(session_id).cloned()
    }

    pub async fn list_sessions(&self) -> Vec<ClaudeCodeSession> {
        let sessions = self.sessions.read().await;
        sessions.values().cloned().collect()
    }

    pub async fn cleanup_completed(&self) -> Result<()> {
        let mut sessions = self.sessions.write().await;
        let mut to_remove = Vec::new();

        for (id, session) in sessions.iter() {
            if session.state == SessionState::Completed || session.state == SessionState::Terminated {
                to_remove.push(id.clone());
            }
        }

        for id in to_remove {
            sessions.remove(&id);
        }

        Ok(())
    }
}

pub struct EnginePool {
    engines: HashMap<String, ClaudeCodeSession>,
    default_engine: ClaudeCodeEngine,
    workspace: PathBuf,
}

impl EnginePool {
    pub fn new() -> Self {
        Self {
            engines: HashMap::new(),
            default_engine: ClaudeCodeEngine::with_default_config(),
            workspace: Self::get_workspace_path(),
        }
    }

    pub fn with_config(config: ClaudeCodeConfig) -> Self {
        Self {
            engines: HashMap::new(),
            default_engine: ClaudeCodeEngine::new(config),
            workspace: Self::get_workspace_path(),
        }
    }

    fn get_workspace_path() -> PathBuf {
        let home = std::env::var("HOME")
            .or_else(|_| std::env::var("USERPROFILE"))
            .unwrap_or_else(|_| ".".to_string());
        PathBuf::from(home).join("Claude Desktop").join("sessions")
    }

    pub async fn create_session(&self, conv_id: &str, working_dir: Option<&str>) -> Result<ClaudeCodeSession> {
        self.default_engine.create_session(conv_id, working_dir).await
    }

    pub async fn execute(
        &self,
        conv_id: &str,
        req: &ChatRequest,
    ) -> Result<ClaudeCodeResult> {
        let mut session = self.default_engine.create_session(conv_id, None).await?;
        self.default_engine.execute_with_api(&mut session, req).await
    }

    pub async fn execute_streaming(
        &self,
        conv_id: &str,
        req: &ChatRequest,
        tx: mpsc::Sender<StreamEvent>,
    ) -> Result<ClaudeCodeResult> {
        let mut session = self.default_engine.create_session(conv_id, None).await?;
        let prompt = self.default_engine.build_prompt_from_messages(req)?;
        self.default_engine
            .execute_streaming(&mut session, &prompt, tx)
            .await
    }

    pub async fn send_message(
        &mut self,
        conv_id: &str,
        req: &ChatRequest,
    ) -> Result<serde_json::Value> {
        let result = self.execute(conv_id, req).await?;

        Ok(serde_json::json!({
            "content": result.content,
            "id": result.message_id,
            "stop_reason": result.stop_reason,
            "tool_results": result.tool_results,
            "usage": result.usage,
            "error": result.error,
        }))
    }

    pub async fn terminate(&self, conv_id: &str) -> Result<()> {
        let session_id = conv_id.to_string();
        self.default_engine.terminate_session(&session_id).await
    }
}

impl Default for EnginePool {
    fn default() -> Self {
        Self::new()
    }
}