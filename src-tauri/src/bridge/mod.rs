use crate::clipboard::ClipboardManager;
use crate::config::{AppConfig, ConfigManager, ConversationStore};
use crate::engine::{EnginePool, EngineState};
use crate::fs::FileOperations;
use crate::git::GitIntegration;
use crate::logger::Logger;
use crate::mcp::{McpConfigManager, McpServerConfig};
use crate::notification::NotificationManager;
use crate::process::ProcessManager;
use crate::prompt::{build_self_hosted_system_prompt, resolve_requested_model_for_mode};
use crate::research::{ResearchEvent, ResearchOrchestrator};
use crate::skills::{DefaultSkills, Skill, SkillManager};
use crate::streaming::{StreamEvent, StreamManager};
use crate::task::{TaskExecutor, TaskRequest, TaskResult};
use crate::terminal::PtyManager;
use crate::updater::AutoUpdater;
use crate::watcher::FileWatcher;
use anyhow::Result;
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::sse::{Event, KeepAlive, Sse},
    routing::{delete, get, post, put},
    Json, Router,
};
use futures::stream::{Stream, StreamExt};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::convert::Infallible;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;
use tower_http::cors::{Any, CorsLayer, AllowOrigin};
use axum::http::header::{HeaderName, ORIGIN, CONTENT_TYPE, AUTHORIZATION, ACCEPT};
use axum::http::Method;

use crate::tools::{execute_tool, get_tool_definitions, ToolDefinition};

#[derive(Clone)]
pub struct BridgeServer {
    engine_pool: Arc<Mutex<EnginePool>>,
    mcp_config: Arc<Mutex<McpConfigManager>>,
    stream_manager: Arc<Mutex<StreamManager>>,
    research_mode: Arc<Mutex<HashMap<String, bool>>>,
    config_manager: Arc<Mutex<Option<ConfigManager>>>,
    skill_manager: Arc<Mutex<SkillManager>>,
    conversation_store: Arc<Mutex<Option<ConversationStore>>>,
    task_executor: Arc<Mutex<Option<TaskExecutor>>>,
    process_manager: Arc<Mutex<ProcessManager>>,
    terminal_manager: Arc<Mutex<PtyManager>>,
    file_watcher: Arc<Mutex<FileWatcher>>,
    clipboard_manager: Arc<Mutex<ClipboardManager>>,
    notification_manager: Arc<Mutex<NotificationManager>>,
    logger: Arc<Mutex<Logger>>,
}

pub type AppState = (
    Arc<Mutex<EnginePool>>,
    Arc<Mutex<McpConfigManager>>,
    Arc<Mutex<StreamManager>>,
    Arc<Mutex<HashMap<String, bool>>>,
    Arc<Mutex<Option<ConfigManager>>>,
    Arc<Mutex<SkillManager>>,
    Arc<Mutex<Option<ConversationStore>>>,
    Arc<Mutex<Option<TaskExecutor>>>,
    Arc<Mutex<ProcessManager>>,
    Arc<Mutex<PtyManager>>,
    Arc<Mutex<FileWatcher>>,
    Arc<Mutex<ClipboardManager>>,
    Arc<Mutex<NotificationManager>>,
    Arc<Mutex<Logger>>,
);

#[derive(Serialize, Deserialize, Clone)]
pub struct ChatRequest {
    pub conversation_id: String,
    pub messages: Option<Vec<serde_json::Value>>,
    pub message: Option<String>,
    pub model: String,
    pub user_mode: Option<String>,
    pub env_token: Option<String>,
    pub env_base_url: Option<String>,
    pub research_mode: Option<bool>,
    pub enable_streaming: Option<bool>,
    pub custom_system_prompt: Option<String>,
}

impl ChatRequest {
    pub fn get_messages(&self) -> Vec<serde_json::Value> {
        if let Some(msgs) = &self.messages {
            return msgs.clone();
        }
        if let Some(msg) = &self.message {
            return vec![serde_json::json!({
                "role": "user",
                "content": msg
            })];
        }
        vec![]
    }
}

#[derive(Serialize, Deserialize, Clone)]
pub struct ToolRequest {
    pub name: String,
    pub input: serde_json::Value,
    pub cwd: Option<String>,
}

#[derive(Serialize)]
pub struct SystemStatus {
    pub platform: String,
    pub git_bash: GitBashStatus,
}

#[derive(Serialize)]
pub struct GitBashStatus {
    pub required: bool,
    pub found: bool,
    pub path: Option<String>,
}

#[derive(Deserialize)]
pub struct StreamQuery {
    pub conversation_id: String,
    pub model: String,
    pub user_mode: Option<String>,
    pub env_token: Option<String>,
    pub env_base_url: Option<String>,
    pub research_mode: Option<bool>,
    pub messages: Option<String>,
}

impl BridgeServer {
    pub fn new(data_dir: PathBuf) -> Self {
        let skills_dir = data_dir.join("skills");
        let store_path = data_dir.join("conversations");
        let log_dir = data_dir.join("logs");

        let mut skill_manager = SkillManager::new(skills_dir.clone());
        if let Err(e) = skill_manager.load_skills() {
            eprintln!("[Bridge] Failed to load skills: {}", e);
        }

        if skill_manager.list_skills().is_empty() {
            for skill in DefaultSkills::get_all_default_skills() {
                let _ = skill_manager.add_skill(skill);
            }
        }

        let conversation_store = ConversationStore::new(store_path);
        let logger = Logger::new(log_dir);
        let file_watcher = FileWatcher::new();

        Self {
            engine_pool: Arc::new(Mutex::new(EnginePool::new())),
            mcp_config: Arc::new(Mutex::new(McpConfigManager::new())),
            stream_manager: Arc::new(Mutex::new(StreamManager::new())),
            research_mode: Arc::new(Mutex::new(HashMap::new())),
            config_manager: Arc::new(Mutex::new(None)),
            skill_manager: Arc::new(Mutex::new(skill_manager)),
            conversation_store: Arc::new(Mutex::new(Some(conversation_store))),
            task_executor: Arc::new(Mutex::new(None)),
            process_manager: Arc::new(Mutex::new(ProcessManager::new())),
            terminal_manager: Arc::new(Mutex::new(PtyManager::new())),
            file_watcher: Arc::new(Mutex::new(file_watcher)),
            clipboard_manager: Arc::new(Mutex::new(ClipboardManager::new())),
            notification_manager: Arc::new(Mutex::new(NotificationManager::new())),
            logger: Arc::new(Mutex::new(logger)),
        }
    }

    pub async fn start(&self, port: u16) -> Result<()> {
        let state: AppState = (
            self.engine_pool.clone(),
            self.mcp_config.clone(),
            self.stream_manager.clone(),
            self.research_mode.clone(),
            self.config_manager.clone(),
            self.skill_manager.clone(),
            self.conversation_store.clone(),
            self.task_executor.clone(),
            self.process_manager.clone(),
            self.terminal_manager.clone(),
            self.file_watcher.clone(),
            self.clipboard_manager.clone(),
            self.notification_manager.clone(),
            self.logger.clone(),
        );

        let allowed_origins = vec![
            "tauri://localhost".parse::<axum::http::HeaderValue>().unwrap(),
            "https://tauri.localhost".parse::<axum::http::HeaderValue>().unwrap(),
            "http://tauri.localhost".parse::<axum::http::HeaderValue>().unwrap(),
            "http://localhost:1420".parse::<axum::http::HeaderValue>().unwrap(),
            "http://localhost:3456".parse::<axum::http::HeaderValue>().unwrap(),
            "http://localhost:5173".parse::<axum::http::HeaderValue>().unwrap(),
            "http://127.0.0.1:1420".parse::<axum::http::HeaderValue>().unwrap(),
            "http://127.0.0.1:3456".parse::<axum::http::HeaderValue>().unwrap(),
            "http://127.0.0.1:5173".parse::<axum::http::HeaderValue>().unwrap(),
            "null".parse::<axum::http::HeaderValue>().unwrap(),
        ];

        let cors = CorsLayer::new()
            .allow_origin(AllowOrigin::list(allowed_origins))
            .allow_methods([Method::GET, Method::POST, Method::PUT, Method::DELETE, Method::OPTIONS])
            .allow_headers([
                CONTENT_TYPE,
                AUTHORIZATION,
                ACCEPT,
                ORIGIN,
            ]);

        let app = Router::new()
            .route("/api/system-status", get(system_status))
            .route("/api/workspace-config", get(workspace_config_get))
            .route("/api/workspace-config", post(workspace_config_set))
            .route("/api/chat", post(chat_handler))
            .route("/api/chat/stream", get(chat_stream_handler))
            .route("/api/tools", post(tools_handler))
            .route("/api/tools/list", get(tools_list_handler))
            .route("/api/tools/execute", post(tool_execute_handler))
            .route("/api/conversations", get(conversations_list))
            .route("/api/conversations", post(conversations_create))
            .route("/api/conversations/{id}", get(conversation_get))
            .route("/api/conversations/{id}", post(conversation_update))
            .route("/api/conversations/{id}", delete(conversation_delete))
            .route("/api/conversations/{id}/messages", get(conversation_messages))
            .route("/api/conversations/{id}/messages/{mid}", delete(conversation_message_delete))
            .route("/api/conversations/{id}/messages-tail/{count}", delete(conversation_messages_tail_delete))
            .route("/api/conversations/{id}/branch", post(conversation_branch_handler))
            .route("/api/conversations/{id}/answer", post(conversation_answer_handler))
            .route("/api/conversations/{id}/permission", post(conversation_permission_handler))
            .route("/api/projects", get(projects_list))
            .route("/api/projects", post(projects_create))
            .route("/api/upload", post(upload_handler))
            .route("/api/providers", get(providers_list))
            .route("/api/providers", post(providers_update))
            .route("/api/config", get(config_get))
            .route("/api/config", post(config_update))
            .route("/api/skills", get(skills_list))
            .route("/api/skills", post(skills_create))
            .route("/api/skills/{name}", get(skill_get))
            .route("/api/skills/{name}", put(skill_update))
            .route("/api/skills/{name}", delete(skill_delete))
            .route("/api/skills/{name}/enable", post(skill_enable))
            .route("/api/skills/match", post(skills_match))
            .route("/api/tasks", post(task_execute))
            .route("/api/tasks/{id}/status", get(task_status))
            .route("/api/tasks/{id}/cancel", post(task_cancel))
            .route("/api/mcp/servers", get(mcp_servers_list))
            .route("/api/mcp/servers", post(mcp_servers_update))
            .route("/api/mcp/servers/{name}/tools", get(mcp_tools_list))
            .route("/api/mcp/servers/{name}/connect", post(mcp_connect_handler))
            .route("/api/mcp/servers/{name}/disconnect", post(mcp_disconnect_handler))
            .route("/api/engines", get(engine_status_handler))
            .route("/api/engines/spawn", post(engine_spawn_handler))
            .route("/api/engines/{conv_id}", delete(engine_kill_handler))
            .route("/api/streams/{conv_id}", get(stream_events_handler))
            .route("/api/research/start", post(research_start_handler))
            .route("/api/research/{id}/stop", post(research_stop_handler))
            .route("/api/research/status/{id}", get(research_status_handler))
            .route("/api/git/status", get(git_status_handler))
            .route("/api/git/log", get(git_log_handler))
            .route("/api/git/diff", get(git_diff_handler))
            .route("/api/git/commit", post(git_commit_handler))
            .route("/api/git/push", post(git_push_handler))
            .route("/api/git/pull", post(git_pull_handler))
            .route("/api/terminal/create", post(terminal_create))
            .route("/api/terminal/write", post(terminal_write))
            .route("/api/terminal/resize", post(terminal_resize))
            .route("/api/terminal/close", post(terminal_close))
            .route("/api/terminal/list", get(terminal_list))
            .route("/api/process/spawn", post(process_spawn))
            .route("/api/process/{pid}", delete(process_kill))
            .route("/api/process/list", get(process_list))
            .route("/api/clipboard/read", get(clipboard_read))
            .route("/api/clipboard/write", post(clipboard_write))
            .route("/api/notification/show", post(notification_show))
            .route("/api/logs", get(logs_read))
            .route("/api/logs/clear", post(logs_clear))
            .route("/api/watcher/start", post(watcher_start))
            .route("/api/watcher/watch", post(watcher_watch))
            .route("/api/watcher/unwatch", post(watcher_unwatch))
            .route("/api/update/check", get(update_check))
            .route("/api/update/download", post(update_download))
            .route("/api/worktrees", get(worktree_list))
            .route("/api/worktrees", post(worktree_create))
            .route("/api/worktrees/sync", post(worktree_sync))
            .route("/api/worktrees/{id}", get(worktree_get))
            .route("/api/worktrees/{id}", delete(worktree_remove))
            .route("/api/worktrees/merge", post(worktree_merge))
            .route("/api/agents", get(agent_list))
            .route("/api/agents/{id}", get(agent_get))
            .route("/api/agents/{id}/cancel", post(agent_cancel))
            .route("/api/ide/status", get(ide_status))
            .route("/api/ide/start", post(ide_start))
            .route("/api/ide/stop", post(ide_stop))
            .route("/api/ide/connections", get(ide_connections))
            .route("/api/ide/connections/{id}", delete(ide_disconnect))
            .route("/api/analytics/track", post(analytics_track))
            .route("/api/analytics/daily/{date}", get(analytics_daily))
            .route("/api/analytics/range", get(analytics_range))
            .route("/api/analytics/summary", get(analytics_summary))
            .route("/api/analytics/event-counts", get(analytics_event_counts))
            .route("/api/analytics/recent-events", get(analytics_recent_events))
            .layer(cors)
            .with_state(state);

        let listener = tokio::net::TcpListener::bind(format!("127.0.0.1:{}", port)).await?;
        println!("[Bridge] Server running on http://127.0.0.1:{}", port);
        axum::serve(listener, app).await?;
        Ok(())
    }
}

async fn system_status() -> Json<SystemStatus> {
    let platform = std::env::consts::OS.to_string();
    let git_bash_path = find_git_bash();

    Json(SystemStatus {
        platform,
        git_bash: GitBashStatus {
            required: cfg!(target_os = "windows"),
            found: git_bash_path.is_some(),
            path: git_bash_path,
        },
    })
}

#[derive(Serialize)]
struct WorkspaceConfig {
    default_dir: String,
}

async fn workspace_config_get() -> Json<WorkspaceConfig> {
    let default_dir = dirs::home_dir()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|| ".".to_string());
    Json(WorkspaceConfig { default_dir })
}

#[derive(Deserialize)]
struct WorkspaceConfigUpdate {
    dir: String,
}

async fn workspace_config_set(
    Json(body): Json<WorkspaceConfigUpdate>,
) -> StatusCode {
    let _ = body.dir;
    StatusCode::OK
}

fn find_git_bash() -> Option<String> {
    let candidates: Vec<String> = if cfg!(target_os = "windows") {
        vec![
            r"C:\Program Files\Git\bin\bash.exe".to_string(),
            r"C:\Program Files (x86)\Git\bin\bash.exe".to_string(),
        ]
    } else {
        vec!["/usr/bin/bash".to_string(), "/bin/bash".to_string()]
    };

    for path in &candidates {
        if std::path::Path::new(path).exists() {
            return Some(path.clone());
        }
    }
    None
}

async fn chat_handler(
    State(state): State<AppState>,
    Json(req): Json<ChatRequest>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let pool = state.0.clone();
    let conv_id = req.conversation_id.clone();

    let rx_opt = {
        let mut pool_guard: tokio::sync::MutexGuard<'_, EnginePool> = pool.lock().await;
        pool_guard.send_message_stream(&req.conversation_id, &req).await.ok().flatten()
    };

    let pool_clone = pool.clone();
    let stream = async_stream::stream! {
        let mut rx = match rx_opt {
            Some(rx) => rx,
            None => {
                yield Ok(Event::default().data(serde_json::json!({"type": "error", "error": "Failed to start message"}).to_string()));
                return;
            }
        };

        while let Some(output) = rx.recv().await {
            let event_data = match output.msg_type.as_str() {
                "ask_user" => {
                    if let Some(ref tool_input) = output.tool_input {
                        let request_id = tool_input.get("request_id")
                            .and_then(|r| r.as_str())
                            .unwrap_or("")
                            .to_string();
                        let tool_use_id = tool_input.get("tool_use_id")
                            .and_then(|r| r.as_str())
                            .unwrap_or("")
                            .to_string();
                        let original_input = tool_input.get("original_input")
                            .cloned()
                            .unwrap_or(serde_json::json!({}));

                        {
                            let mut pool_guard = pool_clone.lock().await;
                            pool_guard.set_ask_user_pending(&conv_id, original_input);
                        }

                        let ask_event = serde_json::json!({
                            "type": "ask_user",
                            "request_id": request_id,
                            "tool_use_id": tool_use_id,
                            "questions": tool_input.get("questions").cloned().unwrap_or(serde_json::json!([])),
                        });
                        Some(ask_event)
                    } else {
                        None
                    }
                }
                "stop" => {
                    let mut stop_event = serde_json::json!({
                        "type": "message_delta",
                        "delta": {"stop_reason": output.stop_reason},
                    });
                    if let Some(usage) = output.usage {
                        stop_event.as_object_mut().unwrap().insert("usage".to_string(), usage);
                    }
                    Some(stop_event)
                }
                "error" => {
                    let err_event = serde_json::json!({
                        "type": "error",
                        "error": output.content,
                    });
                    Some(err_event)
                }
                "text" => {
                    let text_event = serde_json::json!({
                        "type": "content_block_delta",
                        "delta": {"type": "text_delta", "text": output.content},
                    });
                    Some(text_event)
                }
                "tool_call" => {
                    let tool_event = serde_json::json!({
                        "type": "tool_use_done",
                        "tool_use_id": output.tool_use_id,
                        "tool_name": output.tool_name,
                        "tool_input": output.tool_input,
                    });
                    Some(tool_event)
                }
                "ready" => {
                    Some(serde_json::json!({"type": "ready"}))
                }
                "message_start" => {
                    let mut start_event = serde_json::json!({
                        "type": "message_start",
                        "model": output.content,
                    });
                    if let Some(usage) = output.usage {
                        start_event.as_object_mut().unwrap().insert("usage".to_string(), usage);
                    }
                    Some(start_event)
                }
                "tool_permission" => {
                    if let Some(ref tool_input) = output.tool_input {
                        let request_id = tool_input.get("request_id")
                            .and_then(|r| r.as_str())
                            .unwrap_or("")
                            .to_string();
                        let tool_use_id = tool_input.get("tool_use_id")
                            .and_then(|r| r.as_str())
                            .unwrap_or("")
                            .to_string();
                        let input = tool_input.get("input").cloned().unwrap_or(serde_json::json!({}));

                        {
                            let mut pool_guard = pool_clone.lock().await;
                            pool_guard.set_tool_permission_pending(&conv_id, serde_json::json!({
                                "request_id": request_id,
                                "tool_use_id": tool_use_id,
                                "tool_name": output.tool_name,
                                "input": input,
                            }));
                        }

                        let perm_event = serde_json::json!({
                            "type": "tool_permission",
                            "request_id": request_id,
                            "tool_use_id": tool_use_id,
                            "tool_name": output.tool_name,
                            "input": input,
                        });
                        Some(perm_event)
                    } else {
                        None
                    }
                }
                _ => {
                    Some(serde_json::json!({
                        "type": output.msg_type,
                        "content": output.content,
                    }))
                }
            };

            if let Some(data) = event_data {
                let is_stop = data.get("type").and_then(|t| t.as_str()) == Some("message_stop")
                    || data.get("type").and_then(|t| t.as_str()) == Some("error");
                yield Ok(Event::default().data(data.to_string()));
                if is_stop {
                    break;
                }
            }
        }

        {
            let mut pool_guard = pool_clone.lock().await;
            pool_guard.return_message_receiver(&conv_id, rx);
        }
    };

    Sse::new(stream).keep_alive(KeepAlive::default())
}

async fn chat_stream_handler(
    State(state): State<AppState>,
    Query(query): Query<StreamQuery>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, StatusCode> {
    let stream_manager = state.2.clone();
    let mut manager: tokio::sync::MutexGuard<'_, StreamManager> = stream_manager.lock().await;

    let receiver = manager.add_listener(&query.conversation_id)
        .ok_or_else(|| StatusCode::NOT_FOUND)?;

    let stream = async_stream::stream! {
        let mut rx = receiver;
        while let Ok(event) = rx.recv().await {
            let event_name = event.event_type;
            let data = serde_json::to_string(&event.data).unwrap_or_default();
            yield Ok(Event::default()
                .event(&event_name)
                .data(data));
        }
    };

    Ok(Sse::new(stream).keep_alive(KeepAlive::default()))
}

async fn tools_handler(
    Json(req): Json<ToolRequest>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let cwd = req.cwd.clone().unwrap_or_else(|| ".".to_string());
    let name = req.name.clone();
    let input = req.input.clone();

    let result = tokio::task::spawn_blocking(move || {
        execute_tool(&name, input, &cwd)
    }).await;

    match result {
        Ok(Ok(result)) => Ok(Json(result)),
        _ => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

async fn tool_execute_handler(
    State(state): State<AppState>,
    Json(req): Json<ToolRequest>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let cwd = req.cwd.clone().unwrap_or_else(|| ".".to_string());
    let name = req.name.clone();
    let input = req.input.clone();

    let result = tokio::task::spawn_blocking(move || {
        execute_tool(&name, input, &cwd)
    }).await;

    match result {
        Ok(Ok(result)) => Ok(Json(result)),
        _ => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

async fn tools_list_handler() -> Json<Vec<ToolDefinition>> {
    Json(get_tool_definitions())
}

async fn conversations_list(State(state): State<AppState>) -> Json<serde_json::Value> {
    let conversation_store = state.6.clone();
    let store: tokio::sync::MutexGuard<'_, Option<ConversationStore>> = conversation_store.lock().await;
    if let Some(s) = store.as_ref() {
        match s.list_conversations() {
            Ok(convs) => return Json(serde_json::json!({ "conversations": convs })),
            Err(_) => {}
        }
    }
    Json(serde_json::json!({ "conversations": [] }))
}

async fn conversations_create() -> Json<serde_json::Value> {
    Json(serde_json::json!({ "id": uuid::Uuid::new_v4().to_string() }))
}

async fn conversation_get(Path(id): Path<String>, State(state): State<AppState>) -> Json<serde_json::Value> {
    let conversation_store = state.6.clone();
    let store: tokio::sync::MutexGuard<'_, Option<ConversationStore>> = conversation_store.lock().await;
    if let Some(s) = store.as_ref() {
        match s.load_conversation(&id) {
            Ok(messages) => return Json(serde_json::json!({ "id": id, "messages": messages })),
            Err(_) => {}
        }
    }
    Json(serde_json::json!({ "id": id, "messages": [] }))
}

async fn conversation_update(Path(id): Path<String>, State(state): State<AppState>, Json(messages): Json<Vec<serde_json::Value>>) -> Json<serde_json::Value> {
    let conversation_store = state.6.clone();
    let store: tokio::sync::MutexGuard<'_, Option<ConversationStore>> = conversation_store.lock().await;
    if let Some(s) = store.as_ref() {
        let _ = s.save_conversation(&id, &messages);
    }
    Json(serde_json::json!({ "ok": true }))
}

async fn conversation_delete(Path(id): Path<String>, State(state): State<AppState>) -> Json<serde_json::Value> {
    let conversation_store = state.6.clone();
    let store: tokio::sync::MutexGuard<'_, Option<ConversationStore>> = conversation_store.lock().await;
    if let Some(s) = store.as_ref() {
        let _ = s.delete_conversation(&id);
    }
    Json(serde_json::json!({ "ok": true }))
}

async fn conversation_messages(Path(id): Path<String>, State(state): State<AppState>) -> Json<serde_json::Value> {
    let conversation_store = state.6.clone();
    let store: tokio::sync::MutexGuard<'_, Option<ConversationStore>> = conversation_store.lock().await;
    if let Some(s) = store.as_ref() {
        match s.load_conversation(&id) {
            Ok(messages) => return Json(serde_json::json!({ "messages": messages })),
            Err(_) => {}
        }
    }
    Json(serde_json::json!({ "messages": [] }))
}

async fn conversation_message_delete(
    Path((id, mid)): Path<(String, String)>,
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let conversation_store = state.6.clone();
    let mut store: tokio::sync::MutexGuard<'_, Option<ConversationStore>> = conversation_store.lock().await;
    if let Some(s) = store.as_mut() {
        match s.delete_messages_from(&id, &mid) {
            Ok(messages) => return Ok(Json(serde_json::json!({ "success": true, "messages": messages }))),
            Err(e) => { eprintln!("[MessageDelete] Failed: {}", e); return Err(StatusCode::INTERNAL_SERVER_ERROR); }
        }
    }
    Err(StatusCode::NOT_FOUND)
}

async fn conversation_messages_tail_delete(
    Path((id, count)): Path<(String, usize)>,
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let conversation_store = state.6.clone();
    let mut store: tokio::sync::MutexGuard<'_, Option<ConversationStore>> = conversation_store.lock().await;
    if let Some(s) = store.as_mut() {
        match s.delete_messages_tail(&id, count) {
            Ok(messages) => return Ok(Json(serde_json::json!({ "success": true, "messages": messages }))),
            Err(e) => { eprintln!("[MessagesTailDelete] Failed: {}", e); return Err(StatusCode::INTERNAL_SERVER_ERROR); }
        }
    }
    Err(StatusCode::NOT_FOUND)
}

#[derive(Deserialize)]
struct BranchRequest {
    from_message_id: Option<String>,
}

async fn conversation_branch_handler(
    Path(id): Path<String>,
    State(state): State<AppState>,
    Json(req): Json<BranchRequest>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let conversation_store = state.6.clone();
    let store: tokio::sync::MutexGuard<'_, Option<ConversationStore>> = conversation_store.lock().await;
    if let Some(s) = store.as_ref() {
        match s.branch_conversation(&id, req.from_message_id.as_deref()) {
            Ok(new_id) => return Ok(Json(serde_json::json!({ "success": true, "new_conversation_id": new_id }))),
            Err(e) => { eprintln!("[Branch] Failed: {}", e); return Err(StatusCode::INTERNAL_SERVER_ERROR); }
        }
    }
    Err(StatusCode::NOT_FOUND)
}

#[derive(Deserialize)]
struct AnswerRequest {
    request_id: String,
    tool_use_id: Option<String>,
    answers: Option<serde_json::Value>,
}

async fn conversation_answer_handler(
    Path(id): Path<String>,
    State(state): State<AppState>,
    Json(req): Json<AnswerRequest>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let engine_pool = state.0.clone();
    let mut pool: tokio::sync::MutexGuard<'_, EnginePool> = engine_pool.lock().await;

    let original_input = pool.get_ask_user_pending(&id).unwrap_or(serde_json::json!({}));

    let answers = req.answers.unwrap_or(serde_json::json!({}));

    let mut updated_input = original_input;
    if let Some(obj) = updated_input.as_object_mut() {
        obj.insert("answers".to_string(), answers);
    } else {
        updated_input = serde_json::json!({ "answers": answers });
    }

    let tool_use_id = req.tool_use_id.unwrap_or_default();

    match pool.send_control_response(&id, &req.request_id, &tool_use_id, updated_input).await {
        Ok(()) => Ok(Json(serde_json::json!({ "ok": true }))),
        Err(e) => {
            eprintln!("[AskUser] Answer failed: {}", e);
            Err(StatusCode::NOT_FOUND)
        }
    }
}

#[derive(Deserialize)]
struct PermissionRequest {
    request_id: String,
    tool_use_id: Option<String>,
    behavior: Option<String>,
}

async fn conversation_permission_handler(
    Path(id): Path<String>,
    State(state): State<AppState>,
    Json(req): Json<PermissionRequest>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let engine_pool = state.0.clone();
    let mut pool: tokio::sync::MutexGuard<'_, EnginePool> = engine_pool.lock().await;

    let pending = pool.get_tool_permission_pending(&id);
    let tool_use_id = req.tool_use_id
        .or_else(|| pending.as_ref().and_then(|p| p.get("tool_use_id").and_then(|t| t.as_str()).map(String::from)))
        .unwrap_or_default();

    let behavior = req.behavior.unwrap_or_else(|| "allow".to_string());

    let updated_input = pending.and_then(|p| p.get("input").cloned());

    match pool.send_permission_response(&id, &req.request_id, &tool_use_id, &behavior, updated_input).await {
        Ok(()) => Ok(Json(serde_json::json!({ "ok": true }))),
        Err(e) => {
            eprintln!("[Permission] Response failed: {}", e);
            Err(StatusCode::NOT_FOUND)
        }
    }
}

async fn projects_list() -> Json<serde_json::Value> {
    Json(serde_json::json!({ "projects": [] }))
}

async fn projects_create() -> Json<serde_json::Value> {
    Json(serde_json::json!({ "id": uuid::Uuid::new_v4().to_string() }))
}

async fn upload_handler() -> Json<serde_json::Value> {
    Json(serde_json::json!({ "ok": true }))
}

async fn providers_list(State(state): State<AppState>) -> Json<serde_json::Value> {
    let config_manager = state.4.clone();
    let manager: tokio::sync::MutexGuard<'_, Option<ConfigManager>> = config_manager.lock().await;
    if let Some(m) = manager.as_ref() {
        let config = m.get_config();
        return Json(serde_json::json!({ "providers": config.providers }));
    }
    Json(serde_json::json!({ "providers": [] }))
}

async fn providers_update(State(state): State<AppState>, Json(providers): Json<Vec<crate::config::ProviderConfig>>) -> Json<serde_json::Value> {
    let config_manager = state.4.clone();
    let mut manager: tokio::sync::MutexGuard<'_, Option<ConfigManager>> = config_manager.lock().await;
    if let Some(m) = manager.as_mut() {
        let _ = m.update_config(|c| {
            c.providers = providers;
        });
    }
    Json(serde_json::json!({ "ok": true }))
}

async fn config_get(State(state): State<AppState>) -> Json<serde_json::Value> {
    let config_manager = state.4.clone();
    let manager: tokio::sync::MutexGuard<'_, Option<ConfigManager>> = config_manager.lock().await;
    if let Some(m) = manager.as_ref() {
        return Json(serde_json::to_value(m.get_config()).unwrap_or_default());
    }
    Json(serde_json::json!({}))
}

async fn config_update(State(state): State<AppState>, Json(config): Json<AppConfig>) -> Json<serde_json::Value> {
    let config_manager = state.4.clone();
    let mut manager: tokio::sync::MutexGuard<'_, Option<ConfigManager>> = config_manager.lock().await;
    if let Some(m) = manager.as_mut() {
        let _ = m.update_config(|c| *c = config);
    }
    Json(serde_json::json!({ "ok": true }))
}

async fn skills_list(State(state): State<AppState>) -> Json<serde_json::Value> {
    let skill_manager = state.5.clone();
    let manager: tokio::sync::MutexGuard<'_, SkillManager> = skill_manager.lock().await;
    let skills: Vec<&Skill> = manager.list_skills();
    Json(serde_json::json!({ "skills": skills }))
}

async fn skills_create(State(state): State<AppState>, Json(skill): Json<Skill>) -> Json<serde_json::Value> {
    let skill_manager = state.5.clone();
    let mut manager: tokio::sync::MutexGuard<'_, SkillManager> = skill_manager.lock().await;
    match manager.add_skill(skill) {
        Ok(_) => Json(serde_json::json!({ "ok": true })),
        Err(e) => Json(serde_json::json!({ "error": format!("{}", e) })),
    }
}

async fn skill_get(Path(name): Path<String>, State(state): State<AppState>) -> Json<serde_json::Value> {
    let skill_manager = state.5.clone();
    let manager: tokio::sync::MutexGuard<'_, SkillManager> = skill_manager.lock().await;
    if let Some(skill) = manager.get_skill(&name) {
        Json(serde_json::to_value(skill).unwrap_or_default())
    } else {
        Json(serde_json::json!({ "error": "Skill not found" }))
    }
}

async fn skill_update(Path(name): Path<String>, State(state): State<AppState>, Json(skill): Json<Skill>) -> Json<serde_json::Value> {
    let skill_manager = state.5.clone();
    let mut manager: tokio::sync::MutexGuard<'_, SkillManager> = skill_manager.lock().await;
    match manager.update_skill(&name, skill) {
        Ok(_) => Json(serde_json::json!({ "ok": true })),
        Err(e) => Json(serde_json::json!({ "error": format!("{}", e) })),
    }
}

async fn skill_delete(Path(name): Path<String>, State(state): State<AppState>) -> Json<serde_json::Value> {
    let skill_manager = state.5.clone();
    let mut manager: tokio::sync::MutexGuard<'_, SkillManager> = skill_manager.lock().await;
    match manager.delete_skill(&name) {
        Ok(_) => Json(serde_json::json!({ "ok": true })),
        Err(e) => Json(serde_json::json!({ "error": format!("{}", e) })),
    }
}

#[derive(Deserialize)]
pub struct SkillEnableRequest {
    pub enabled: bool,
}

async fn skill_enable(Path(name): Path<String>, State(state): State<AppState>, Json(req): Json<SkillEnableRequest>) -> Json<serde_json::Value> {
    let skill_manager = state.5.clone();
    let mut manager: tokio::sync::MutexGuard<'_, SkillManager> = skill_manager.lock().await;
    match manager.enable_skill(&name, req.enabled) {
        Ok(_) => Json(serde_json::json!({ "ok": true })),
        Err(e) => Json(serde_json::json!({ "error": format!("{}", e) })),
    }
}

#[derive(Deserialize)]
pub struct SkillMatchRequest {
    pub input: String,
}

async fn skills_match(State(state): State<AppState>, Json(req): Json<SkillMatchRequest>) -> Json<serde_json::Value> {
    let skill_manager = state.5.clone();
    let manager: tokio::sync::MutexGuard<'_, SkillManager> = skill_manager.lock().await;
    if let Some(m) = manager.find_matching_skill(&req.input) {
        Json(serde_json::to_value(m).unwrap_or_default())
    } else {
        Json(serde_json::json!({ "matched": false }))
    }
}

#[derive(Deserialize)]
pub struct TaskExecuteRequest {
    pub task_id: String,
    pub prompt: String,
    pub model: Option<String>,
    pub max_tokens: Option<u32>,
    pub context: Option<Vec<serde_json::Value>>,
}

async fn task_execute(
    State(state): State<AppState>,
    Json(req): Json<TaskExecuteRequest>,
) -> Result<Json<TaskResult>, StatusCode> {
    let api_key = std::env::var("ANTHROPIC_API_KEY").unwrap_or_default();
    let base_url = std::env::var("ANTHROPIC_BASE_URL")
        .unwrap_or_else(|_| "https://api.anthropic.com".to_string());

    let executor = TaskExecutor::new(api_key, base_url);

    let task_request = TaskRequest {
        task_id: req.task_id,
        prompt: req.prompt,
        model: req.model,
        max_tokens: req.max_tokens,
        context: req.context,
        tools: None,
    };

    match executor.execute_task(task_request, |_| Box::new(|_| {} )).await {
        Ok(result) => Ok(Json(result)),
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

async fn task_status(Path(id): Path<String>, State(state): State<AppState>) -> Json<serde_json::Value> {
    let task_executor = state.7.clone();
    let executor: tokio::sync::MutexGuard<'_, Option<TaskExecutor>> = task_executor.lock().await;
    if let Some(e) = executor.as_ref() {
        if let Some(status) = e.get_task_status(&id).await {
            return Json(serde_json::json!({ "status": format!("{:?}", status) }));
        }
    }
    Json(serde_json::json!({ "status": "not_found" }))
}

async fn task_cancel(Path(id): Path<String>, State(state): State<AppState>) -> Json<serde_json::Value> {
    let task_executor = state.7.clone();
    let executor: tokio::sync::MutexGuard<'_, Option<TaskExecutor>> = task_executor.lock().await;
    if let Some(e) = executor.as_ref() {
        let cancelled = e.cancel_task(&id).await;
        return Json(serde_json::json!({ "cancelled": cancelled }));
    }
    Json(serde_json::json!({ "cancelled": false }))
}

async fn mcp_servers_list(State(state): State<AppState>) -> Json<serde_json::Value> {
    let mcp_config = state.1.clone();
    let config: tokio::sync::MutexGuard<'_, McpConfigManager> = mcp_config.lock().await;
    let servers: Vec<serde_json::Value> = config.list_servers()
        .iter()
        .map(|s| serde_json::json!({
            "name": s.name,
            "command": s.command,
            "args": s.args,
            "enabled": s.enabled
        }))
        .collect();

    Json(serde_json::json!({ "servers": servers }))
}

async fn mcp_servers_update(State(state): State<AppState>, Json(servers): Json<Vec<McpServerConfig>>) -> Json<serde_json::Value> {
    let mcp_config = state.1.clone();
    let mut config: tokio::sync::MutexGuard<'_, McpConfigManager> = mcp_config.lock().await;

    for server in servers {
        config.add_server(server.name.clone(), server);
    }

    Json(serde_json::json!({ "ok": true }))
}

async fn mcp_tools_list(Path(name): Path<String>, State(state): State<AppState>) -> Result<Json<serde_json::Value>, StatusCode> {
    let mcp_config = state.1.clone();
    let config: tokio::sync::MutexGuard<'_, McpConfigManager> = mcp_config.lock().await;

    if let Some(server) = config.get_server(&name) {
        Ok(Json(serde_json::json!({
            "name": server.name,
            "command": server.command
        })))
    } else {
        Err(StatusCode::NOT_FOUND)
    }
}

async fn mcp_connect_handler(Path(name): Path<String>, State(state): State<AppState>) -> Result<Json<serde_json::Value>, StatusCode> {
    let mcp_config = state.1.clone();
    let config: tokio::sync::MutexGuard<'_, McpConfigManager> = mcp_config.lock().await;

    if let Some(server) = config.get_server(&name) {
        Ok(Json(serde_json::json!({
            "ok": true,
            "name": server.name,
            "status": "ready"
        })))
    } else {
        Err(StatusCode::NOT_FOUND)
    }
}

async fn mcp_disconnect_handler(Path(name): Path<String>, State(state): State<AppState>) -> Result<Json<serde_json::Value>, StatusCode> {
    let mcp_config = state.1.clone();
    let mut config: tokio::sync::MutexGuard<'_, McpConfigManager> = mcp_config.lock().await;

    if config.remove_server(&name).is_some() {
        Ok(Json(serde_json::json!({ "ok": true })))
    } else {
        Err(StatusCode::NOT_FOUND)
    }
}

async fn engine_status_handler(State(state): State<AppState>) -> Json<serde_json::Value> {
    let pool = state.0.clone();
    let pool_guard: tokio::sync::MutexGuard<'_, EnginePool> = pool.lock().await;
    let engines: Vec<serde_json::Value> = pool_guard.list_engines()
        .iter()
        .map(|e| serde_json::json!({
            "conv_id": e.conv_id,
            "pid": e.pid,
            "model": e.model,
            "session_id": e.session_id,
            "state": format!("{:?}", e.state),
            "workspace": e.workspace.to_string_lossy()
        }))
        .collect();

    Json(serde_json::json!({
        "engines": engines,
        "workspace": pool_guard.get_workspace().to_string_lossy()
    }))
}

#[derive(Deserialize)]
pub struct SpawnRequest {
    pub conv_id: String,
    pub model: String,
    pub cwd: Option<String>,
}

async fn engine_spawn_handler(State(state): State<AppState>, Json(req): Json<SpawnRequest>) -> Result<Json<serde_json::Value>, StatusCode> {
    let pool = state.0.clone();
    let mut pool_guard: tokio::sync::MutexGuard<'_, EnginePool> = pool.lock().await;
    match pool_guard.spawn_engine(&req.conv_id, &req.model, req.cwd).await {
        Ok(handle) => Ok(Json(serde_json::json!({
            "ok": true,
            "conv_id": handle.conv_id,
            "session_id": handle.session_id,
            "pid": handle.pid
        }))),
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

async fn engine_kill_handler(Path(conv_id): Path<String>, State(state): State<AppState>) -> Json<serde_json::Value> {
    let pool = state.0.clone();
    let mut pool_guard: tokio::sync::MutexGuard<'_, EnginePool> = pool.lock().await;
    pool_guard.remove_engine(&conv_id).await;
    Json(serde_json::json!({ "ok": true }))
}

async fn stream_events_handler(Path(conv_id): Path<String>, State(state): State<AppState>) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, StatusCode> {
    let stream_manager = state.2.clone();
    let mut manager: tokio::sync::MutexGuard<'_, StreamManager> = stream_manager.lock().await;

    let receiver = manager.add_listener(&conv_id)
        .ok_or_else(|| StatusCode::NOT_FOUND)?;

    let stream = async_stream::stream! {
        let mut rx = receiver;
        while let Ok(event) = rx.recv().await {
            let event_name = event.event_type;
            let data = serde_json::to_string(&event.data).unwrap_or_default();
            yield Ok(Event::default()
                .event(&event_name)
                .data(data));
        }
    };

    Ok(Sse::new(stream).keep_alive(KeepAlive::default()))
}

async fn research_start_handler(State(state): State<AppState>, Json(req): Json<ChatRequest>) -> Json<serde_json::Value> {
    Json(serde_json::json!({ "ok": true, "message": "Research mode not implemented" }))
}

async fn research_stop_handler(Path(id): Path<String>, State(state): State<AppState>) -> Json<serde_json::Value> {
    Json(serde_json::json!({ "ok": true }))
}

async fn research_status_handler(Path(id): Path<String>, State(state): State<AppState>) -> Json<serde_json::Value> {
    Json(serde_json::json!({ "status": "idle" }))
}

#[derive(Deserialize)]
pub struct GitRequest {
    pub cwd: Option<String>,
    pub message: Option<String>,
    pub remote: Option<String>,
    pub branch: Option<String>,
    pub file: Option<String>,
    pub force: Option<bool>,
}

async fn git_status_handler(State(state): State<AppState>, Query(query): Query<GitRequest>) -> Json<serde_json::Value> {
    let git = GitIntegration::with_cwd(query.cwd);
    match git.get_status() {
        Ok(status) => Json(serde_json::json!({ "status": status })),
        Err(e) => Json(serde_json::json!({ "error": format!("{}", e) })),
    }
}

async fn git_log_handler(State(state): State<AppState>, Query(query): Query<GitRequest>) -> Json<serde_json::Value> {
    let git = GitIntegration::with_cwd(query.cwd);
    match git.get_commits(Some(10), None) {
        Ok(commits) => Json(serde_json::json!({ "commits": commits })),
        Err(e) => Json(serde_json::json!({ "error": format!("{}", e) })),
    }
}

async fn git_diff_handler(State(state): State<AppState>, Query(query): Query<GitRequest>) -> Json<serde_json::Value> {
    let git = GitIntegration::with_cwd(query.cwd);
    match git.get_file_diff(query.file.as_deref()) {
        Ok(diff) => Json(serde_json::json!({ "diff": diff })),
        Err(e) => Json(serde_json::json!({ "error": format!("{}", e) })),
    }
}

async fn git_commit_handler(State(state): State<AppState>, Json(req): Json<GitRequest>) -> Result<Json<serde_json::Value>, StatusCode> {
    let git = GitIntegration::with_cwd(req.cwd);
    let message = req.message.ok_or_else(|| StatusCode::BAD_REQUEST)?;

    match git.commit(&message) {
        Ok(_) => Ok(Json(serde_json::json!({ "ok": true }))),
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

async fn git_push_handler(State(state): State<AppState>, Json(req): Json<GitRequest>) -> Result<Json<serde_json::Value>, StatusCode> {
    let git = GitIntegration::with_cwd(req.cwd);
    match git.push(req.remote.as_deref(), req.branch.as_deref(), req.force.unwrap_or(false)) {
        Ok(output) => Ok(Json(serde_json::json!({ "ok": true, "output": output }))),
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

async fn git_pull_handler(State(state): State<AppState>, Json(req): Json<GitRequest>) -> Result<Json<serde_json::Value>, StatusCode> {
    let git = GitIntegration::with_cwd(req.cwd);
    match git.pull(req.remote.as_deref(), req.branch.as_deref()) {
        Ok(output) => Ok(Json(serde_json::json!({ "ok": true, "output": output }))),
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

#[derive(Deserialize)]
pub struct TerminalCreateRequest {
    pub cwd: Option<String>,
    pub shell: Option<String>,
}

async fn terminal_create(State(state): State<AppState>, Json(req): Json<TerminalCreateRequest>) -> Json<serde_json::Value> {
    let terminal_manager = state.9.clone();
    let manager: tokio::sync::MutexGuard<'_, PtyManager> = terminal_manager.lock().await;
    match manager.create_session(req.cwd, req.shell).await {
        Ok(session) => Json(serde_json::to_value(session).unwrap_or_default()),
        Err(e) => Json(serde_json::json!({ "error": format!("{}", e) })),
    }
}

#[derive(Deserialize)]
pub struct TerminalWriteRequest {
    pub session_id: String,
    pub data: String,
}

async fn terminal_write(State(state): State<AppState>, Json(req): Json<TerminalWriteRequest>) -> Json<serde_json::Value> {
    let terminal_manager = state.9.clone();
    let manager: tokio::sync::MutexGuard<'_, PtyManager> = terminal_manager.lock().await;
    match manager.write_input(&req.session_id, &req.data).await {
        Ok(_) => Json(serde_json::json!({ "ok": true })),
        Err(e) => Json(serde_json::json!({ "error": format!("{}", e) })),
    }
}

#[derive(Deserialize)]
pub struct TerminalResizeRequest {
    pub session_id: String,
    pub cols: u16,
    pub rows: u16,
}

async fn terminal_resize(State(state): State<AppState>, Json(req): Json<TerminalResizeRequest>) -> Json<serde_json::Value> {
    let terminal_manager = state.9.clone();
    let manager: tokio::sync::MutexGuard<'_, PtyManager> = terminal_manager.lock().await;
    match manager.resize(&req.session_id, req.cols, req.rows).await {
        Ok(_) => Json(serde_json::json!({ "ok": true })),
        Err(e) => Json(serde_json::json!({ "error": format!("{}", e) })),
    }
}

async fn terminal_close(State(state): State<AppState>, Json(session_id): Json<String>) -> Json<serde_json::Value> {
    let terminal_manager = state.9.clone();
    let manager: tokio::sync::MutexGuard<'_, PtyManager> = terminal_manager.lock().await;
    match manager.close_session(&session_id).await {
        Ok(_) => Json(serde_json::json!({ "ok": true })),
        Err(e) => Json(serde_json::json!({ "error": format!("{}", e) })),
    }
}

async fn terminal_list(State(state): State<AppState>) -> Json<serde_json::Value> {
    let terminal_manager = state.9.clone();
    let manager: tokio::sync::MutexGuard<'_, PtyManager> = terminal_manager.lock().await;
    let sessions = manager.list_sessions().await;
    Json(serde_json::json!({ "sessions": sessions }))
}

#[derive(Deserialize)]
pub struct ProcessSpawnRequest {
    pub command: String,
    pub cwd: Option<String>,
    pub env_vars: Option<std::collections::HashMap<String, String>>,
}

async fn process_spawn(State(state): State<AppState>, Json(req): Json<ProcessSpawnRequest>) -> Json<serde_json::Value> {
    let process_manager = state.8.clone();
    let manager: tokio::sync::MutexGuard<'_, ProcessManager> = process_manager.lock().await;
    match manager.spawn(&req.command, req.cwd.as_deref(), req.env_vars).await {
        Ok(info) => Json(serde_json::to_value(info).unwrap_or_default()),
        Err(e) => Json(serde_json::json!({ "error": format!("{}", e) })),
    }
}

async fn process_kill(Path(pid): Path<u32>, State(state): State<AppState>) -> Json<serde_json::Value> {
    let process_manager = state.8.clone();
    let manager: tokio::sync::MutexGuard<'_, ProcessManager> = process_manager.lock().await;
    match manager.kill(pid).await {
        Ok(_) => Json(serde_json::json!({ "ok": true })),
        Err(e) => Json(serde_json::json!({ "error": format!("{}", e) })),
    }
}

async fn process_list(State(state): State<AppState>) -> Json<serde_json::Value> {
    let process_manager = state.8.clone();
    let manager: tokio::sync::MutexGuard<'_, ProcessManager> = process_manager.lock().await;
    let processes = manager.list_processes().await;
    Json(serde_json::json!({ "processes": processes }))
}

async fn clipboard_read(State(state): State<AppState>) -> Json<serde_json::Value> {
    let clipboard_manager = state.11.clone();
    let manager: tokio::sync::MutexGuard<'_, ClipboardManager> = clipboard_manager.lock().await;
    match manager.read() {
        Ok(content) => Json(serde_json::to_value(content).unwrap_or_default()),
        Err(e) => Json(serde_json::json!({ "error": format!("{}", e) })),
    }
}

#[derive(Deserialize)]
pub struct ClipboardWriteRequest {
    pub text: Option<String>,
}

async fn clipboard_write(State(state): State<AppState>, Json(req): Json<ClipboardWriteRequest>) -> Json<serde_json::Value> {
    let clipboard_manager = state.11.clone();
    let manager: tokio::sync::MutexGuard<'_, ClipboardManager> = clipboard_manager.lock().await;
    let content = crate::clipboard::ClipboardContent {
        text: req.text,
        html: None,
        image: None,
    };
    match manager.write(&content) {
        Ok(_) => Json(serde_json::json!({ "ok": true })),
        Err(e) => Json(serde_json::json!({ "error": format!("{}", e) })),
    }
}

#[derive(Deserialize)]
pub struct NotificationRequest {
    pub title: String,
    pub body: String,
    pub urgency: Option<String>,
}

async fn notification_show(State(state): State<AppState>, Json(req): Json<NotificationRequest>) -> Json<serde_json::Value> {
    let notification_manager = state.12.clone();
    let manager: tokio::sync::MutexGuard<'_, NotificationManager> = notification_manager.lock().await;
    let options = crate::notification::NotificationOptions {
        title: req.title,
        body: req.body,
        icon: None,
        silent: None,
        urgency: req.urgency,
        timeout: None,
    };
    match manager.show(&options) {
        Ok(_) => Json(serde_json::json!({ "ok": true })),
        Err(e) => Json(serde_json::json!({ "error": format!("{}", e) })),
    }
}

#[derive(Deserialize)]
pub struct LogsReadRequest {
    pub level: Option<String>,
    pub source: Option<String>,
    pub search: Option<String>,
    pub limit: Option<usize>,
}

async fn logs_read(State(state): State<AppState>, Query(req): Query<LogsReadRequest>) -> Json<serde_json::Value> {
    let logger = state.13.clone();
    let logger_guard: tokio::sync::MutexGuard<'_, Logger> = logger.lock().await;
    let filter = crate::logger::LogFilter {
        level: req.level,
        source: req.source,
        from_time: None,
        to_time: None,
        search: req.search,
    };
    match logger_guard.read_logs(Some(filter), req.limit.unwrap_or(100)) {
        Ok(entries) => Json(serde_json::json!({ "logs": entries })),
        Err(e) => Json(serde_json::json!({ "error": format!("{}", e) })),
    }
}

#[derive(Deserialize)]
pub struct LogsClearRequest {
    pub days: Option<u32>,
}

async fn logs_clear(State(state): State<AppState>, Json(req): Json<LogsClearRequest>) -> Json<serde_json::Value> {
    let logger = state.13.clone();
    let logger_guard: tokio::sync::MutexGuard<'_, Logger> = logger.lock().await;
    match logger_guard.clear_old_logs(req.days.unwrap_or(30)) {
        Ok(_) => Json(serde_json::json!({ "ok": true })),
        Err(e) => Json(serde_json::json!({ "error": format!("{}", e) })),
    }
}

async fn watcher_start(State(state): State<AppState>) -> Json<serde_json::Value> {
    let file_watcher = state.10.clone();
    let watcher: tokio::sync::MutexGuard<'_, FileWatcher> = file_watcher.lock().await;
    match watcher.start().await {
        Ok(_) => Json(serde_json::json!({ "ok": true })),
        Err(e) => Json(serde_json::json!({ "error": format!("{}", e) })),
    }
}

#[derive(Deserialize)]
pub struct WatcherWatchRequest {
    pub path: String,
}

async fn watcher_watch(State(state): State<AppState>, Json(req): Json<WatcherWatchRequest>) -> Json<serde_json::Value> {
    let file_watcher = state.10.clone();
    let watcher: tokio::sync::MutexGuard<'_, FileWatcher> = file_watcher.lock().await;
    match watcher.watch(&req.path).await {
        Ok(_) => Json(serde_json::json!({ "ok": true })),
        Err(e) => Json(serde_json::json!({ "error": format!("{}", e) })),
    }
}

async fn watcher_unwatch(State(state): State<AppState>, Json(req): Json<WatcherWatchRequest>) -> Json<serde_json::Value> {
    let file_watcher = state.10.clone();
    let watcher: tokio::sync::MutexGuard<'_, FileWatcher> = file_watcher.lock().await;
    match watcher.unwatch(&req.path).await {
        Ok(_) => Json(serde_json::json!({ "ok": true })),
        Err(e) => Json(serde_json::json!({ "error": format!("{}", e) })),
    }
}

async fn update_check() -> Json<serde_json::Value> {
    let updater = AutoUpdater::new(
        "https://clawparrot.com/updates",
        env!("CARGO_PKG_VERSION"),
        std::path::PathBuf::from(std::env::temp_dir()).join("claude-desktop-updates"),
    );
    match updater.check_for_updates().await {
        Ok(Some(info)) => Json(serde_json::to_value(info).unwrap_or_default()),
        Ok(None) => Json(serde_json::json!({ "up_to_date": true })),
        Err(e) => Json(serde_json::json!({ "error": format!("{}", e) })),
    }
}

#[derive(Deserialize)]
pub struct UpdateDownloadRequest {
    pub url: String,
}

async fn update_download(Json(req): Json<UpdateDownloadRequest>) -> Json<serde_json::Value> {
    let updater = AutoUpdater::new(
        "https://clawparrot.com/updates",
        env!("CARGO_PKG_VERSION"),
        std::path::PathBuf::from(std::env::temp_dir()).join("claude-desktop-updates"),
    );
    match updater.download_update(&req.url).await {
        Ok(path) => Json(serde_json::json!({ "path": path.to_string_lossy() })),
        Err(e) => Json(serde_json::json!({ "error": format!("{}", e) })),
    }
}

use crate::worktree::{WorktreeManager, CreateWorktreeRequest, MergeWorktreeRequest};
use crate::ide::{IdeBridge, IdeConfig};

static WORKTREE_MANAGER: once_cell::sync::Lazy<tokio::sync::Mutex<Option<WorktreeManager>>> =
    once_cell::sync::Lazy::new(|| tokio::sync::Mutex::new(None));

static IDE_BRIDGE: once_cell::sync::Lazy<tokio::sync::Mutex<Option<IdeBridge>>> =
    once_cell::sync::Lazy::new(|| tokio::sync::Mutex::new(None));

async fn worktree_create(Json(req): Json<CreateWorktreeRequest>) -> Result<Json<serde_json::Value>, StatusCode> {
    let mut guard = WORKTREE_MANAGER.lock().await;
    if guard.is_none() {
        *guard = Some(WorktreeManager::with_cwd(None));
    }
    if let Some(mgr) = guard.as_ref() {
        match mgr.create_worktree(req).await {
            Ok(info) => Ok(Json(serde_json::json!({ "success": true, "worktree": info }))),
            Err(e) => {
                eprintln!("[Worktree] Create failed: {}", e);
                Err(StatusCode::INTERNAL_SERVER_ERROR)
            }
        }
    } else {
        Err(StatusCode::SERVICE_UNAVAILABLE)
    }
}

async fn worktree_list() -> Result<Json<serde_json::Value>, StatusCode> {
    let guard = WORKTREE_MANAGER.lock().await;
    if let Some(mgr) = guard.as_ref() {
        match mgr.list_worktrees().await {
            Ok(list) => Ok(Json(serde_json::json!({ "success": true, "worktrees": list }))),
            Err(e) => {
                eprintln!("[Worktree] List failed: {}", e);
                Err(StatusCode::INTERNAL_SERVER_ERROR)
            }
        }
    } else {
        Ok(Json(serde_json::json!({ "success": true, "worktrees": [] })))
    }
}

async fn worktree_get(Path(id): Path<String>) -> Result<Json<serde_json::Value>, StatusCode> {
    let guard = WORKTREE_MANAGER.lock().await;
    if let Some(mgr) = guard.as_ref() {
        match mgr.get_worktree(&id).await {
            Some(info) => Ok(Json(serde_json::json!({ "success": true, "worktree": info }))),
            None => Err(StatusCode::NOT_FOUND),
        }
    } else {
        Err(StatusCode::NOT_FOUND)
    }
}

async fn worktree_remove(Path(id): Path<String>) -> Result<Json<serde_json::Value>, StatusCode> {
    let guard = WORKTREE_MANAGER.lock().await;
    if let Some(mgr) = guard.as_ref() {
        match mgr.remove_worktree(&id).await {
            Ok(()) => Ok(Json(serde_json::json!({ "success": true }))),
            Err(e) => {
                eprintln!("[Worktree] Remove failed: {}", e);
                Err(StatusCode::INTERNAL_SERVER_ERROR)
            }
        }
    } else {
        Err(StatusCode::NOT_FOUND)
    }
}

async fn worktree_merge(Json(req): Json<MergeWorktreeRequest>) -> Result<Json<serde_json::Value>, StatusCode> {
    let guard = WORKTREE_MANAGER.lock().await;
    if let Some(mgr) = guard.as_ref() {
        match mgr.merge_worktree(req).await {
            Ok(output) => Ok(Json(serde_json::json!({ "success": true, "output": output }))),
            Err(e) => {
                eprintln!("[Worktree] Merge failed: {}", e);
                Err(StatusCode::INTERNAL_SERVER_ERROR)
            }
        }
    } else {
        Err(StatusCode::SERVICE_UNAVAILABLE)
    }
}

async fn worktree_sync() -> Result<Json<serde_json::Value>, StatusCode> {
    let mut guard = WORKTREE_MANAGER.lock().await;
    if guard.is_none() {
        *guard = Some(WorktreeManager::with_cwd(None));
    }
    if let Some(mgr) = guard.as_ref() {
        match mgr.sync_from_git().await {
            Ok(list) => Ok(Json(serde_json::json!({ "success": true, "worktrees": list }))),
            Err(e) => {
                eprintln!("[Worktree] Sync failed: {}", e);
                Err(StatusCode::INTERNAL_SERVER_ERROR)
            }
        }
    } else {
        Err(StatusCode::SERVICE_UNAVAILABLE)
    }
}

async fn agent_list() -> Result<Json<serde_json::Value>, StatusCode> {
    let guard = WORKTREE_MANAGER.lock().await;
    if let Some(mgr) = guard.as_ref() {
        let agents = mgr.list_agents().await;
        Ok(Json(serde_json::json!({ "success": true, "agents": agents })))
    } else {
        Ok(Json(serde_json::json!({ "success": true, "agents": [] })))
    }
}

async fn agent_get(Path(id): Path<String>) -> Result<Json<serde_json::Value>, StatusCode> {
    let guard = WORKTREE_MANAGER.lock().await;
    if let Some(mgr) = guard.as_ref() {
        match mgr.get_agent(&id).await {
            Some(info) => Ok(Json(serde_json::json!({ "success": true, "agent": info }))),
            None => Err(StatusCode::NOT_FOUND),
        }
    } else {
        Err(StatusCode::NOT_FOUND)
    }
}

async fn agent_cancel(Path(id): Path<String>) -> Result<Json<serde_json::Value>, StatusCode> {
    let guard = WORKTREE_MANAGER.lock().await;
    if let Some(mgr) = guard.as_ref() {
        match mgr.cancel_agent(&id).await {
            Ok(()) => Ok(Json(serde_json::json!({ "success": true }))),
            Err(e) => {
                eprintln!("[Agent] Cancel failed: {}", e);
                Err(StatusCode::INTERNAL_SERVER_ERROR)
            }
        }
    } else {
        Err(StatusCode::NOT_FOUND)
    }
}

async fn ide_status() -> Result<Json<serde_json::Value>, StatusCode> {
    let guard = IDE_BRIDGE.lock().await;
    if let Some(bridge) = guard.as_ref() {
        let status = bridge.get_status().await;
        Ok(Json(serde_json::json!({ "success": true, "status": status })))
    } else {
        Ok(Json(serde_json::json!({
            "success": true,
            "status": { "server_running": false, "port": 0, "active_connections": 0, "total_connections": 0 }
        })))
    }
}

async fn ide_start() -> Result<Json<serde_json::Value>, StatusCode> {
    let mut guard = IDE_BRIDGE.lock().await;
    if guard.is_none() {
        *guard = Some(IdeBridge::new(IdeConfig::default()));
    }
    if let Some(bridge) = guard.as_ref() {
        match bridge.start_server().await {
            Ok(port) => Ok(Json(serde_json::json!({ "success": true, "port": port }))),
            Err(e) => {
                eprintln!("[IDE] Start failed: {}", e);
                Err(StatusCode::INTERNAL_SERVER_ERROR)
            }
        }
    } else {
        Err(StatusCode::SERVICE_UNAVAILABLE)
    }
}

async fn ide_stop() -> Result<Json<serde_json::Value>, StatusCode> {
    let guard = IDE_BRIDGE.lock().await;
    if let Some(bridge) = guard.as_ref() {
        bridge.stop_server().await;
        Ok(Json(serde_json::json!({ "success": true })))
    } else {
        Ok(Json(serde_json::json!({ "success": true })))
    }
}

async fn ide_connections() -> Result<Json<serde_json::Value>, StatusCode> {
    let guard = IDE_BRIDGE.lock().await;
    if let Some(bridge) = guard.as_ref() {
        let conns = bridge.list_connections().await;
        Ok(Json(serde_json::json!({ "success": true, "connections": conns })))
    } else {
        Ok(Json(serde_json::json!({ "success": true, "connections": [] })))
    }
}

async fn ide_disconnect(Path(id): Path<String>) -> Result<Json<serde_json::Value>, StatusCode> {
    let guard = IDE_BRIDGE.lock().await;
    if let Some(bridge) = guard.as_ref() {
        match bridge.disconnect(&id).await {
            Ok(()) => Ok(Json(serde_json::json!({ "success": true }))),
            Err(e) => {
                eprintln!("[IDE] Disconnect failed: {}", e);
                Err(StatusCode::INTERNAL_SERVER_ERROR)
            }
        }
    } else {
        Err(StatusCode::NOT_FOUND)
    }
}

use crate::analytics::{AnalyticsStore, TrackEventRequest};

static ANALYTICS_STORE: once_cell::sync::Lazy<tokio::sync::Mutex<Option<AnalyticsStore>>> =
    once_cell::sync::Lazy::new(|| tokio::sync::Mutex::new(None));

async fn analytics_track(Json(req): Json<TrackEventRequest>) -> Result<Json<serde_json::Value>, StatusCode> {
    let mut guard = ANALYTICS_STORE.lock().await;
    if guard.is_none() {
        let data_dir = std::env::current_dir().unwrap_or_default().join("data").join("analytics");
        *guard = Some(AnalyticsStore::new(data_dir));
    }
    if let Some(store) = guard.as_ref() {
        match store.track_event(&req).await {
            Ok(()) => Ok(Json(serde_json::json!({ "success": true }))),
            Err(e) => {
                eprintln!("[Analytics] Track failed: {}", e);
                Err(StatusCode::INTERNAL_SERVER_ERROR)
            }
        }
    } else {
        Err(StatusCode::SERVICE_UNAVAILABLE)
    }
}

async fn analytics_daily(Path(date): Path<String>) -> Result<Json<serde_json::Value>, StatusCode> {
    let mut guard = ANALYTICS_STORE.lock().await;
    if guard.is_none() {
        let data_dir = std::env::current_dir().unwrap_or_default().join("data").join("analytics");
        *guard = Some(AnalyticsStore::new(data_dir));
    }
    if let Some(store) = guard.as_ref() {
        match store.get_daily_stats(&date).await {
            Some(stats) => Ok(Json(serde_json::json!({ "success": true, "stats": stats }))),
            None => Err(StatusCode::NOT_FOUND),
        }
    } else {
        Err(StatusCode::NOT_FOUND)
    }
}

async fn analytics_range(Query(params): Query<HashMap<String, String>>) -> Result<Json<serde_json::Value>, StatusCode> {
    let mut guard = ANALYTICS_STORE.lock().await;
    if guard.is_none() {
        let data_dir = std::env::current_dir().unwrap_or_default().join("data").join("analytics");
        *guard = Some(AnalyticsStore::new(data_dir));
    }
    if let Some(store) = guard.as_ref() {
        let from = params.get("from").map(|s| s.as_str()).unwrap_or("2025-01-01");
        let to = params.get("to").map(|s| s.as_str()).unwrap_or("2099-12-31");
        let stats = store.get_stats_range(from, to).await;
        Ok(Json(serde_json::json!({ "success": true, "stats": stats })))
    } else {
        Ok(Json(serde_json::json!({ "success": true, "stats": [] })))
    }
}

async fn analytics_summary(Query(params): Query<HashMap<String, String>>) -> Result<Json<serde_json::Value>, StatusCode> {
    let mut guard = ANALYTICS_STORE.lock().await;
    if guard.is_none() {
        let data_dir = std::env::current_dir().unwrap_or_default().join("data").join("analytics");
        *guard = Some(AnalyticsStore::new(data_dir));
    }
    if let Some(store) = guard.as_ref() {
        let days: u32 = params.get("days").and_then(|d| d.parse().ok()).unwrap_or(30);
        let summary = store.get_usage_summary(days).await;
        Ok(Json(serde_json::json!({ "success": true, "summary": summary })))
    } else {
        Err(StatusCode::SERVICE_UNAVAILABLE)
    }
}

async fn analytics_event_counts(Query(params): Query<HashMap<String, String>>) -> Result<Json<serde_json::Value>, StatusCode> {
    let mut guard = ANALYTICS_STORE.lock().await;
    if guard.is_none() {
        let data_dir = std::env::current_dir().unwrap_or_default().join("data").join("analytics");
        *guard = Some(AnalyticsStore::new(data_dir));
    }
    if let Some(store) = guard.as_ref() {
        let days: u32 = params.get("days").and_then(|d| d.parse().ok()).unwrap_or(30);
        let counts = store.get_event_type_counts(days);
        Ok(Json(serde_json::json!({ "success": true, "counts": counts })))
    } else {
        Ok(Json(serde_json::json!({ "success": true, "counts": [] })))
    }
}

async fn analytics_recent_events(Query(params): Query<HashMap<String, String>>) -> Result<Json<serde_json::Value>, StatusCode> {
    let mut guard = ANALYTICS_STORE.lock().await;
    if guard.is_none() {
        let data_dir = std::env::current_dir().unwrap_or_default().join("data").join("analytics");
        *guard = Some(AnalyticsStore::new(data_dir));
    }
    if let Some(store) = guard.as_ref() {
        let limit: usize = params.get("limit").and_then(|d| d.parse().ok()).unwrap_or(50);
        let events = store.get_recent_events(limit);
        Ok(Json(serde_json::json!({ "success": true, "events": events })))
    } else {
        Ok(Json(serde_json::json!({ "success": true, "events": [] })))
    }
}
