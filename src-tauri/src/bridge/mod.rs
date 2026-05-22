use crate::clipboard::ClipboardManager;
use crate::config::{AppConfig, ConfigManager};
use crate::db::DbManager;
use crate::engine::{EnginePool, EngineState};
use crate::fs::FileOperations;
use crate::git::GitIntegration;
use crate::logger::Logger;
use crate::mcp::{McpServerManager, McpServerConfig, McpServerStatus, McpResource, McpResourceContent};
use crate::native_engine::{NativeEngine, ProviderManager};
use crate::notification::NotificationManager;
use crate::permissions::{AuditLogger, PermissionManager, PermissionMode};
use crate::process::ProcessManager;
use crate::prompt::{build_self_hosted_system_prompt, resolve_requested_model_for_mode};
use crate::research::{ResearchEvent, ResearchOrchestrator, ResearchRequest};
use crate::multiagent::{MultiAgentOrchestrator as PipelineOrchestrator, OrchestratorConfig, OrchestratorEvent};
use crate::orchestration::{MultiAgentOrchestrator, OrchestratorConfigFile};
use crate::skills::{Skill, SkillsManager, SkillExecutionContext};
use crate::streaming::{StreamEvent, StreamManager};
use crate::task::{TaskExecutor, TaskRequest, TaskResult};
use crate::terminal::PtyManager;
use crate::updater::AutoUpdater;
use crate::watcher::FileWatcher;
use anyhow::Result;
use axum::{
    extract::{Path, Query, State, Multipart},
    http::StatusCode,
    response::sse::{Event, KeepAlive, Sse},
    routing::{delete, get, patch, post, put},
    Json, Router,
};
use futures::stream::{Stream, StreamExt};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::convert::Infallible;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::{Mutex, broadcast};
use tower_http::cors::{Any, CorsLayer, AllowOrigin};
use axum::response::IntoResponse;
use axum::http::header::{HeaderName, ORIGIN, CONTENT_TYPE, AUTHORIZATION, ACCEPT};
use axum::http::Method;

use crate::tools::{execute_tool, get_tool_definitions, ToolDefinition};

struct ResearchTask {
    handle: tokio::task::JoinHandle<()>,
    event_tx: broadcast::Sender<ResearchEvent>,
}

#[derive(Clone)]
pub struct BridgeServer {
    engine_pool: Arc<Mutex<EnginePool>>,
    native_engine: Arc<Mutex<Option<NativeEngine>>>,
    mcp_server_manager: Arc<McpServerManager>,
    stream_manager: Arc<Mutex<StreamManager>>,
    research_mode: Arc<Mutex<HashMap<String, bool>>>,
    config_manager: Arc<Mutex<Option<ConfigManager>>>,
    skill_manager: Arc<Mutex<SkillsManager>>,
    db_manager: Arc<DbManager>,
    task_executor: Arc<Mutex<Option<TaskExecutor>>>,
    process_manager: Arc<Mutex<ProcessManager>>,
    terminal_manager: Arc<Mutex<PtyManager>>,
    file_watcher: Arc<Mutex<FileWatcher>>,
    clipboard_manager: Arc<Mutex<ClipboardManager>>,
    notification_manager: Arc<Mutex<NotificationManager>>,
    logger: Arc<Mutex<Logger>>,
    active_research: Arc<Mutex<HashMap<String, ResearchTask>>>,
    orchestrator: Arc<Mutex<Option<crate::orchestration::MultiAgentOrchestrator>>>,
}

pub type AppState = (
    Arc<Mutex<EnginePool>>,
    Arc<McpServerManager>,
    Arc<Mutex<StreamManager>>,
    Arc<Mutex<HashMap<String, bool>>>,
    Arc<Mutex<Option<ConfigManager>>>,
    Arc<Mutex<SkillsManager>>,
    Arc<DbManager>,
    Arc<Mutex<Option<TaskExecutor>>>,
    Arc<Mutex<ProcessManager>>,
    Arc<Mutex<PtyManager>>,
    Arc<Mutex<FileWatcher>>,
    Arc<Mutex<ClipboardManager>>,
    Arc<Mutex<NotificationManager>>,
    Arc<Mutex<Logger>>,
    Arc<Mutex<Option<NativeEngine>>>,
    Arc<Mutex<HashMap<String, ResearchTask>>>,
    Arc<Mutex<Option<crate::orchestration::MultiAgentOrchestrator>>>,
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
    pub permission_mode: Option<String>,
}

impl ChatRequest {
    pub fn get_messages(&self) -> Vec<serde_json::Value> {
        if let Some(msgs) = &self.messages {
            if !msgs.is_empty() {
                return msgs.clone();
            }
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
        let log_dir = data_dir.join("logs");

        let mut skill_manager = SkillsManager::new();
        if let Err(e) = skill_manager.install_bundled_skills() {
            eprintln!("[Bridge] Failed to install bundled skills: {}", e);
        }

        let db_path = data_dir.join("claude_desktop.db");
        let db_manager = DbManager::new(db_path.clone()).expect("Failed to initialize database");
        db_manager.init().expect("Failed to initialize database schema");
        println!("[Bridge] Database initialized at {:?}", db_path);
        println!("[Bridge] Running migration check...");
        {
            let data_dir_ref = &data_dir;
            db_manager.with_conn(|conn| {
                if let Err(e) = crate::db::migration::check_and_migrate(data_dir_ref, conn) {
                    eprintln!("[Bridge] Migration warning: {}", e);
                }
            }).ok();
        }
        println!("[Bridge] Migration check completed");
        let db_manager = Arc::new(db_manager);
        let logger = Logger::new(log_dir);
        let file_watcher = FileWatcher::new();

        let config_dir = data_dir.clone();
        let config_manager = ConfigManager::new(config_dir.clone());
        println!("[Bridge] ConfigManager initialized at {:?}", data_dir.display());
        let config_manager = Arc::new(Mutex::new(Some(config_manager)));

        let provider_manager = Arc::new(Mutex::new(ProviderManager::new(
            data_dir.join("providers.json")
        )));
        let task_executor = TaskExecutor::new_with_provider_manager(
            provider_manager.clone(),
            db_manager.clone(),
        );
        
        let audit_logger = Arc::new(AuditLogger::new(1000));
        let permission_manager = Arc::new(PermissionManager::new(audit_logger));
        
        let native_engine = Arc::new(Mutex::new(Some(NativeEngine::new(
            provider_manager,
            db_manager.clone(),
            data_dir.join("workspaces"),
            permission_manager,
        ))));
        println!("[Bridge] NativeEngine initialized");

        let config_path = std::path::Path::new("config/orchestration.toml");
        let orchestrator_config = if config_path.exists() {
            OrchestratorConfigFile::load_or_default(config_path)
        } else {
            OrchestratorConfigFile::default()
        };
        let orchestrator = MultiAgentOrchestrator::new(
            (&orchestrator_config).into(),
            &data_dir,
        );
        let orchestrator = Arc::new(Mutex::new(Some(orchestrator)));
        println!("[Bridge] MultiAgentOrchestrator initialized");

        Self {
            engine_pool: Arc::new(Mutex::new(EnginePool::new())),
            native_engine,
            mcp_server_manager: Arc::new(McpServerManager::new(config_dir.join("mcp-servers.json"))),
            stream_manager: Arc::new(Mutex::new(StreamManager::new())),
            research_mode: Arc::new(Mutex::new(HashMap::new())),
            config_manager,
            skill_manager: Arc::new(Mutex::new(skill_manager)),
            db_manager,
            task_executor: Arc::new(Mutex::new(Some(task_executor))),
            process_manager: Arc::new(Mutex::new(ProcessManager::new())),
            terminal_manager: Arc::new(Mutex::new(PtyManager::new())),
            file_watcher: Arc::new(Mutex::new(file_watcher)),
            clipboard_manager: Arc::new(Mutex::new(ClipboardManager::new())),
            notification_manager: Arc::new(Mutex::new(NotificationManager::new())),
            logger: Arc::new(Mutex::new(logger)),
            active_research: Arc::new(Mutex::new(HashMap::new())),
            orchestrator,
        }
    }

    pub async fn start(&self, port: u16) -> Result<()> {
        if let Err(e) = self.mcp_server_manager.initialize().await {
            eprintln!("[Bridge] Failed to initialize MCP server manager: {}", e);
        }

        let state: AppState = (
            self.engine_pool.clone(),
            self.mcp_server_manager.clone(),
            self.stream_manager.clone(),
            self.research_mode.clone(),
            self.config_manager.clone(),
            self.skill_manager.clone(),
            self.db_manager.clone(),
            self.task_executor.clone(),
            self.process_manager.clone(),
            self.terminal_manager.clone(),
            self.file_watcher.clone(),
            self.clipboard_manager.clone(),
            self.notification_manager.clone(),
            self.logger.clone(),
            self.native_engine.clone(),
            self.active_research.clone(),
            self.orchestrator.clone(),
        );
        println!("[Bridge] Database manager ready");

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
            .allow_methods([Method::GET, Method::POST, Method::PUT, Method::PATCH, Method::DELETE, Method::OPTIONS])
            .allow_headers([
                CONTENT_TYPE,
                AUTHORIZATION,
                ACCEPT,
                ORIGIN,
                HeaderName::from_static("x-conversation-id"),
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
            .route("/api/conversations/{id}/warm", post(conversation_warm_handler))
            .route("/api/projects", get(projects_list))
            .route("/api/projects", post(projects_create))
            .route("/api/upload", post(upload_handler))
            .route("/api/uploads/{id}/raw", get(upload_get_handler))
            .route("/api/uploads/{id}", delete(upload_delete_handler))
            .route("/api/providers", get(providers_list))
            .route("/api/providers", post(providers_create))
            .route("/api/providers/models", get(providers_models_list))
            .route("/api/providers/{id}", patch(providers_patch))
            .route("/api/providers/{id}", delete(providers_delete))
            .route("/api/providers/{id}/test-websearch", post(providers_test_websearch))
            .route("/api/config", get(config_get))
            .route("/api/config", post(config_update))
            .route("/api/skills", get(skills_list))
            .route("/api/skills", post(skills_create))
            .route("/api/skills/{name}", get(skill_get))
            .route("/api/skills/{name}", put(skill_update))
            .route("/api/skills/{name}", delete(skill_delete))
            .route("/api/skills/{name}/enable", post(skill_enable))
            .route("/api/skills/{name}/execute", post(skill_execute))
            .route("/api/skills/match", post(skills_match))
            .route("/api/workflow/execute", post(workflow_execute))
            .route("/api/workflow/stats", get(workflow_stats))
            .route("/api/workflow/config", get(workflow_config_get))
            .route("/api/workflow/config", post(workflow_config_set))
            .route("/api/tasks", post(task_execute))
            .route("/api/tasks/{id}/status", get(task_status))
            .route("/api/tasks/{id}/cancel", post(task_cancel))
            .route("/api/mcp/servers", get(mcp_servers_list))
            .route("/api/mcp/servers", post(mcp_servers_update))
            .route("/api/mcp/servers/{name}/tools", get(mcp_tools_list))
            .route("/api/mcp/servers/{name}/resources", get(mcp_resources_list))
            .route("/api/mcp/servers/{name}/resources/{uri}", get(mcp_resource_read))
            .route("/api/mcp/servers/{name}/resources/{uri}/monitor", post(mcp_resource_monitor))
            .route("/api/mcp/servers/{name}/connect", post(mcp_connect_handler))
            .route("/api/mcp/servers/{name}/disconnect", post(mcp_disconnect_handler))
            .route("/api/engines", get(engine_status_handler))
            .route("/api/engines/spawn", post(engine_spawn_handler))
            .route("/api/engines/{conv_id}", delete(engine_kill_handler))
            .route("/api/streams/{conv_id}", get(stream_events_handler))
            .route("/api/research/start", post(research_start_handler))
            .route("/api/research/{id}/stop", post(research_stop_handler))
            .route("/api/research/status/{id}", get(research_status_handler))
            .route("/api/research/{id}/events", get(research_events_handler))
            .route("/api/multiagent/research", post(multiagent_research_handler))
            .route("/api/computer-use/screen-info", get(computer_use_screen_info))
            .route("/api/computer-use/execute", post(computer_use_execute))
            .route("/api/computer-use/screenshot", get(computer_use_screenshot))
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
) -> impl IntoResponse {
    let native_engine = state.14.clone();
    let config_manager = state.4.clone();
    let conv_id = req.conversation_id.clone();
    let model = req.model.clone();
    let messages = req.get_messages();

    println!("[Chat] Received request: conv_id={}, model={}, messages={}", conv_id, model, messages.len());

    // Sync providers from ConfigManager to NativeEngine before each request
    let providers_to_sync = {
        let cm_guard: tokio::sync::MutexGuard<'_, Option<ConfigManager>> = config_manager.lock().await;
        if let Some(cm) = cm_guard.as_ref() {
            cm.get_config().providers.iter().map(|p| {
                crate::native_engine::provider_manager::Provider {
                    id: p.id.clone(),
                    name: p.name.clone(),
                    base_url: p.base_url.clone(),
                    api_key: p.api_key.clone().unwrap_or_default(),
                    api_format: {
                        // DeepSeek uses OpenAI-compatible API even if user selected wrong format
                        let is_deepseek = p.base_url.contains("deepseek");
                        if p.provider_type == "anthropic" && !is_deepseek {
                            crate::native_engine::provider_manager::ApiFormat::Anthropic
                        } else {
                            crate::native_engine::provider_manager::ApiFormat::OpenAI
                        }
                    },
                    models: p.models.iter().map(|m| crate::native_engine::provider_manager::ModelConfig {
                        id: m.id.clone(),
                        name: m.name.clone(),
                        enabled: m.enabled,
                        max_tokens: m.max_tokens,
                        supports_vision: m.supports_vision,
                        supports_web_search: false,
                    }).collect(),
                    enabled: p.enabled,
                    web_search_strategy: p.web_search_strategy.clone(),
                }
            }).collect::<Vec<_>>()
        } else {
            Vec::new()
        }
    };

    let rx_opt = {
        let mut engine_guard: tokio::sync::MutexGuard<'_, Option<NativeEngine>> = native_engine.lock().await;
        if let Some(engine) = engine_guard.as_mut() {
            // Sync latest providers
            engine.sync_providers(providers_to_sync).await;

            // Set permission mode from frontend
            if let Some(pm) = &req.permission_mode {
                let mode = crate::permissions::PermissionMode::from_str(pm);
                engine.set_permission_mode(mode).await;
                println!("[Chat] Permission mode set to: {}", pm);
            }

            let chat_req = crate::native_engine::engine_core::ChatRequest {
                conversation_id: conv_id.clone(),
                messages: messages.clone(),
                model: if model.is_empty() { "claude-sonnet-4-20250514".to_string() } else { model.clone() },
                system_prompt: req.custom_system_prompt.clone(),
                max_tokens: None,
                workspace_path: None,
                temperature: None,
                top_p: None,
            };
            match engine.send_message(chat_req).await {
                Ok(rx) => Some(rx),
                Err(e) => {
                    eprintln!("[Chat] NativeEngine send_message error: {}", e);
                    None
                }
            }
        } else {
            eprintln!("[Chat] NativeEngine not initialized");
            None
        }
    };

    let stream = async_stream::stream! {
        let mut rx = match rx_opt {
            Some(rx) => rx,
            None => {
                yield Ok::<Event, Infallible>(Event::default().data(serde_json::json!({"type": "error", "error": "Failed to start message: NativeEngine not available"}).to_string()));
                return;
            }
        };

        let mut full_text = String::new();

        while let Some(event) = rx.recv().await {
            let event_data = match event {
                crate::native_engine::tool_loop::EngineEvent::MessageStart { model } => {
                    Some(serde_json::json!({
                        "type": "message_start",
                        "model": model,
                    }))
                }
                crate::native_engine::tool_loop::EngineEvent::Text(text) => {
                    full_text.push_str(&text);
                    Some(serde_json::json!({
                        "type": "content_block_delta",
                        "delta": {"type": "text_delta", "text": text},
                    }))
                }
                crate::native_engine::tool_loop::EngineEvent::Thinking(thinking) => {
                    Some(serde_json::json!({
                        "type": "thinking",
                        "thinking": thinking,
                    }))
                }
                crate::native_engine::tool_loop::EngineEvent::ToolUseStart { tool_use_id, tool_name, tool_input, text_before } => {
                    println!("[Chat] Tool use started: {} ({})", tool_name, tool_use_id);
                    Some(serde_json::json!({
                        "type": "tool_use_start",
                        "tool_use_id": tool_use_id,
                        "tool_name": tool_name,
                        "tool_input": tool_input,
                        "textBefore": text_before,
                    }))
                }
                crate::native_engine::tool_loop::EngineEvent::ToolArgDelta { tool_use_id, delta } => {
                    Some(serde_json::json!({
                        "type": "tool_arg_delta",
                        "tool_use_id": tool_use_id,
                        "delta": delta,
                    }))
                }
                crate::native_engine::tool_loop::EngineEvent::ToolUseDone { tool_use_id, tool_name, tool_input, output, is_error } => {
                    println!("[Chat] Tool use completed: {} ({}) is_error={}", tool_name, tool_use_id, is_error);
                    Some(serde_json::json!({
                        "type": "tool_use_done",
                        "tool_use_id": tool_use_id,
                        "tool_name": tool_name,
                        "tool_input": tool_input,
                        "output": output,
                        "content": output,
                        "is_error": is_error,
                    }))
                }
                crate::native_engine::tool_loop::EngineEvent::MessageDelta { stop_reason } => {
                    Some(serde_json::json!({
                        "type": "message_delta",
                        "delta": {"stop_reason": stop_reason},
                    }))
                }
                crate::native_engine::tool_loop::EngineEvent::MessageStop { full_text: _, stop_reason } => {
                    Some(serde_json::json!({
                        "type": "message_stop",
                        "stop_reason": stop_reason,
                        "full_text": full_text.clone(),
                    }))
                }
                crate::native_engine::tool_loop::EngineEvent::Error(err) => {
                    eprintln!("[Chat] Engine error: {}", err);
                    Some(serde_json::json!({
                        "type": "error",
                        "error": err,
                    }))
                }
                crate::native_engine::tool_loop::EngineEvent::Usage(usage) => {
                    Some(serde_json::json!({
                        "type": "usage",
                        "usage": usage,
                    }))
                }
                crate::native_engine::tool_loop::EngineEvent::AskUser { question, options } => {
                    let options_json: Vec<serde_json::Value> = options.iter()
                        .map(|o| serde_json::json!({"label": o, "description": ""}))
                        .collect();
                    Some(serde_json::json!({
                        "type": "ask_user",
                        "request_id": "ask_user_request",
                        "tool_use_id": "ask_user_tool",
                        "questions": [{
                            "question": question,
                            "options": options_json
                        }],
                    }))
                }
            };

            if let Some(data) = event_data {
                let is_stop = data.get("type").and_then(|t| t.as_str()) == Some("message_stop")
                    || data.get("type").and_then(|t| t.as_str()) == Some("error");
                yield Ok::<Event, Infallible>(Event::default().data(data.to_string()));
                if is_stop {
                    break;
                }
            }
        }

        println!("[Chat] Stream ended for conv_id={}", conv_id);
    };

    let mut response = Sse::new(stream).keep_alive(KeepAlive::default()).into_response();
    response.headers_mut().insert(
        CONTENT_TYPE,
        "text/event-stream; charset=utf-8".parse().unwrap(),
    );
    response
}

async fn chat_stream_handler(
    State(state): State<AppState>,
    Query(query): Query<StreamQuery>,
) -> Result<impl IntoResponse, StatusCode> {
    let stream_manager = state.2.clone();
    let mut manager: tokio::sync::MutexGuard<'_, StreamManager> = stream_manager.lock().await;

    let receiver = manager.add_listener(&query.conversation_id)
        .ok_or_else(|| StatusCode::NOT_FOUND)?;

    let stream = async_stream::stream! {
        let mut rx = receiver;
        while let Ok(event) = rx.recv().await {
            let event_name = event.event_type;
            let data = serde_json::to_string(&event.data).unwrap_or_default();
            yield Ok::<Event, Infallible>(Event::default()
                .event(&event_name)
                .data(data));
        }
    };

    let mut response = Sse::new(stream).keep_alive(KeepAlive::default()).into_response();
    response.headers_mut().insert(
        CONTENT_TYPE,
        "text/event-stream; charset=utf-8".parse().unwrap(),
    );
    Ok(response)
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
    let db = state.6.clone();
    let result = tokio::task::spawn_blocking(move || {
        db.with_conn(|conn| crate::db::conversation_repo::list_conversations(conn))
    }).await;
    match result {
        Ok(Ok(Ok(convs))) => Json(serde_json::json!({ "conversations": convs })),
        _ => Json(serde_json::json!({ "conversations": [] })),
    }
}

async fn conversations_create(State(state): State<AppState>) -> Json<serde_json::Value> {
    let id = uuid::Uuid::new_v4().to_string();
    let db = state.6.clone();
    let id_clone = id.clone();
    let _ = tokio::task::spawn_blocking(move || {
        db.with_conn(|conn| {
            let now = chrono::Utc::now().to_rfc3339();
            crate::db::conversation_repo::insert_conversation(conn, &id_clone, None, None, None, None, None, false, false, false, &now, &now, 0)
        })
    }).await;
    Json(serde_json::json!({ "id": id }))
}

async fn conversation_get(Path(id): Path<String>, State(state): State<AppState>) -> Json<serde_json::Value> {
    let db = state.6.clone();
    let id_clone = id.clone();
    let result = tokio::task::spawn_blocking(move || {
        db.with_conn(|conn| crate::db::message_repo::get_messages_by_conversation(conn, &id_clone))
    }).await;
    match result {
        Ok(Ok(Ok(messages))) => Json(serde_json::json!({ "id": id, "messages": messages })),
        _ => Json(serde_json::json!({ "id": id, "messages": [] })),
    }
}

async fn conversation_update(Path(id): Path<String>, State(state): State<AppState>, Json(messages): Json<Vec<serde_json::Value>>) -> Json<serde_json::Value> {
    let db = state.6.clone();
    let _ = tokio::task::spawn_blocking(move || {
        db.with_conn(|conn: &rusqlite::Connection| {
            let tx = conn.unchecked_transaction()?;
            crate::db::message_repo::delete_messages_from(&tx, &id, 0)?;
            for (idx, msg) in messages.iter().enumerate() {
                let msg_id = msg.get("id").and_then(|v| v.as_str()).unwrap_or(&uuid::Uuid::new_v4().to_string()).to_string();
                let role = msg.get("role").and_then(|v| v.as_str()).unwrap_or("user");
                let content = match msg.get("content") {
                    Some(v) if v.is_string() => v.as_str().unwrap_or("").to_string(),
                    Some(v) => serde_json::to_string(v).unwrap_or_default(),
                    None => String::new(),
                };
                let now = chrono::Utc::now().to_rfc3339();
                crate::db::message_repo::insert_message(&tx, &msg_id, &id, role, &content, None, &now, false, idx as i64)?;
            }
            crate::db::conversation_repo::increment_message_count(&tx, &id)?;
            tx.commit()?;
            Ok::<(), anyhow::Error>(())
        })
    }).await;
    Json(serde_json::json!({ "ok": true }))
}

async fn conversation_delete(Path(id): Path<String>, State(state): State<AppState>) -> Json<serde_json::Value> {
    let db = state.6.clone();
    let _ = tokio::task::spawn_blocking(move || {
        db.with_conn(|conn| {
            crate::db::message_repo::delete_messages_from(conn, &id, 0).ok();
            crate::db::conversation_repo::delete_conversation(conn, &id)
        })
    }).await;
    Json(serde_json::json!({ "ok": true }))
}

async fn conversation_messages(Path(id): Path<String>, State(state): State<AppState>) -> Json<serde_json::Value> {
    let db = state.6.clone();
    let result = tokio::task::spawn_blocking(move || {
        db.with_conn(|conn| crate::db::message_repo::get_messages_by_conversation(conn, &id))
    }).await;
    match result {
        Ok(Ok(Ok(messages))) => Json(serde_json::json!({ "messages": messages })),
        _ => Json(serde_json::json!({ "messages": [] })),
    }
}

async fn conversation_message_delete(
    Path((id, mid)): Path<(String, String)>,
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let db = state.6.clone();
    let result = tokio::task::spawn_blocking(move || {
        db.with_conn(|conn| {
            let msg = crate::db::message_repo::get_message(conn, &mid)?;
            if let Some(m) = msg {
                crate::db::message_repo::delete_messages_from(conn, &id, m.sort_order)?;
            }
            crate::db::message_repo::get_messages_by_conversation(conn, &id)
        })
    }).await;
    match result {
        Ok(Ok(Ok(messages))) => Ok(Json(serde_json::json!({ "success": true, "messages": messages }))),
        Ok(Ok(Err(e))) => { eprintln!("[MessageDelete] Failed: {}", e); Err(StatusCode::INTERNAL_SERVER_ERROR) }
        Ok(Err(e)) => { eprintln!("[MessageDelete] DB lock error: {}", e); Err(StatusCode::INTERNAL_SERVER_ERROR) }
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

async fn conversation_messages_tail_delete(
    Path((id, count)): Path<(String, i64)>,
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let db = state.6.clone();
    let result = tokio::task::spawn_blocking(move || {
        db.with_conn(|conn| {
            crate::db::message_repo::delete_messages_tail(conn, &id, count)?;
            crate::db::message_repo::get_messages_by_conversation(conn, &id)
        })
    }).await;
    match result {
        Ok(Ok(Ok(messages))) => Ok(Json(serde_json::json!({ "success": true, "messages": messages }))),
        Ok(Ok(Err(e))) => { eprintln!("[MessagesTailDelete] Failed: {}", e); Err(StatusCode::INTERNAL_SERVER_ERROR) }
        Ok(Err(e)) => { eprintln!("[MessagesTailDelete] DB lock error: {}", e); Err(StatusCode::INTERNAL_SERVER_ERROR) }
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
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
    let db = state.6.clone();
    let result = tokio::task::spawn_blocking(move || {
        db.with_conn(|conn| {
            let new_id = uuid::Uuid::new_v4().to_string();
            let now = chrono::Utc::now().to_rfc3339();
            let source = crate::db::conversation_repo::get_conversation(conn, &id)?;
            let title = source.as_ref().and_then(|c| c.title.as_deref()).unwrap_or("Branched conversation");
            let model = source.as_ref().and_then(|c| c.model.as_deref());
            crate::db::conversation_repo::insert_conversation(
                conn, &new_id, Some(&format!("{} (branch)", title)), model, None, None, None, false, false, false, &now, &now, 0,
            )?;
            let mut messages = crate::db::message_repo::get_messages_by_conversation(conn, &id)?;
            if let Some(mid) = req.from_message_id.as_deref() {
                if let Some(m) = crate::db::message_repo::get_message(conn, mid)? {
                    messages.retain(|msg| msg.sort_order < m.sort_order);
                }
            }
            for msg in &messages {
                let msg_id = uuid::Uuid::new_v4().to_string();
                crate::db::message_repo::insert_message(
                    conn, &msg_id, &new_id, &msg.role, &msg.content, msg.thinking.as_deref(), &msg.created_at, msg.is_compact_boundary, msg.sort_order,
                )?;
            }
            Ok::<String, anyhow::Error>(new_id)
        })
    }).await;
    match result {
        Ok(Ok(Ok(new_id))) => Ok(Json(serde_json::json!({ "success": true, "new_conversation_id": new_id }))),
        Ok(Ok(Err(e))) => { eprintln!("[Branch] Failed: {}", e); Err(StatusCode::INTERNAL_SERVER_ERROR) }
        Ok(Err(e)) => { eprintln!("[Branch] DB lock error: {}", e); Err(StatusCode::INTERNAL_SERVER_ERROR) }
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
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
        obj.insert("answers".to_string(), answers.clone());
    } else {
        updated_input = serde_json::json!({ "answers": answers.clone() });
    }

    let tool_use_id = req.tool_use_id.unwrap_or_default();

    match pool.send_control_response(&id, &req.request_id, &tool_use_id, updated_input).await {
        Ok(()) => Ok(Json(serde_json::json!({ "ok": true }))),
        Err(_) => {
            drop(pool);
            let native_engine = state.14.clone();
            let engine_guard: tokio::sync::MutexGuard<'_, Option<NativeEngine>> = native_engine.lock().await;
            if let Some(engine) = engine_guard.as_ref() {
                let answer_str = serde_json::to_string(&answers).unwrap_or_default();
                match engine.resume_with_answer(&id, answer_str).await {
                    Ok(()) => Ok(Json(serde_json::json!({ "ok": true }))),
                    Err(e) => {
                        eprintln!("[AskUser] Native engine answer failed: {}", e);
                        Err(StatusCode::NOT_FOUND)
                    }
                }
            } else {
                eprintln!("[AskUser] No engine available for conversation {}", id);
                Err(StatusCode::NOT_FOUND)
            }
        }
    }
}

#[derive(Deserialize)]
struct WarmRequest {
    permission_mode: Option<String>,
}

async fn conversation_warm_handler(
    Path(id): Path<String>,
    State(state): State<AppState>,
    Json(req): Json<WarmRequest>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    if let Some(ref pm_str) = req.permission_mode {
        let perm_mode = match pm_str.as_str() {
            "ask_permissions" => PermissionMode::AskPermissions,
            "accept_edits" => PermissionMode::AcceptEdits,
            "plan_mode" => PermissionMode::PlanMode,
            "bypass_permissions" => PermissionMode::BypassPermissions,
            _ => PermissionMode::AskPermissions,
        };
        if let Some(engine) = state.14.lock().await.as_ref() {
            engine.set_permission_mode(perm_mode).await;
            eprintln!("[Bridge] Warm: permission_mode set to {:?} for conversation {}", perm_mode, id);
        }
    }
    Ok(Json(serde_json::json!({ "ok": true })))
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
        Err(_) => {
            drop(pool);
            let native_engine = state.14.clone();
            let engine_guard: tokio::sync::MutexGuard<'_, Option<NativeEngine>> = native_engine.lock().await;
            if let Some(engine) = engine_guard.as_ref() {
                let answer = if behavior == "allow" { "allow".to_string() } else { "deny".to_string() };
                match engine.resume_with_answer(&id, answer).await {
                    Ok(()) => Ok(Json(serde_json::json!({ "ok": true }))),
                    Err(e) => {
                        eprintln!("[Permission] Native engine answer failed: {}", e);
                        Err(StatusCode::NOT_FOUND)
                    }
                }
            } else {
                eprintln!("[Permission] No pool engine and no native engine for conversation {}", id);
                Err(StatusCode::NOT_FOUND)
            }
        }
    }
}

async fn projects_list() -> Json<serde_json::Value> {
    Json(serde_json::json!({ "projects": [] }))
}

async fn projects_create() -> Json<serde_json::Value> {
    Json(serde_json::json!({ "id": uuid::Uuid::new_v4().to_string() }))
}

static UPLOAD_DIR: once_cell::sync::Lazy<std::sync::Mutex<Option<PathBuf>>> =
    once_cell::sync::Lazy::new(|| std::sync::Mutex::new(None));

fn get_upload_dir() -> PathBuf {
    let guard = UPLOAD_DIR.lock().unwrap();
    if let Some(dir) = guard.as_ref() {
        return dir.clone();
    }
    drop(guard);
    let default_dir = dirs::data_local_dir()
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_default())
        .join("claude-desktop")
        .join("uploads");
    let mut guard = UPLOAD_DIR.lock().unwrap();
    *guard = Some(default_dir.clone());
    default_dir
}

async fn upload_handler(mut multipart: Multipart) -> Result<Json<serde_json::Value>, StatusCode> {
    let upload_dir = get_upload_dir();
    std::fs::create_dir_all(&upload_dir).map_err(|e| {
        eprintln!("[Upload] Failed to create upload dir: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let mut field_data: Option<(String, Vec<u8>)> = None;

    while let Some(field) = multipart.next_field().await.map_err(|e| {
        eprintln!("[Upload] Multipart error: {}", e);
        StatusCode::BAD_REQUEST
    })? {
        let name = field.name().unwrap_or("").to_string();
        if name == "file" {
            let file_name = field.file_name()
                .unwrap_or("unnamed")
                .to_string();
            let content_type = field.content_type()
                .unwrap_or("application/octet-stream")
                .to_string();
            let data = field.bytes().await.map_err(|_| StatusCode::BAD_REQUEST)?;

            field_data = Some((file_name.clone(), data.to_vec()));

            let file_size = data.len();
            let file_id = uuid::Uuid::new_v4().to_string();
            let ext = std::path::Path::new(&file_name)
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("");

            let file_type = if content_type.starts_with("image/") {
                "image"
            } else if content_type == "application/pdf" || ext == "pdf" {
                "document"
            } else if content_type.starts_with("text/") || matches!(ext, "txt" | "md" | "csv" | "json" | "xml" | "yaml" | "yml") {
                "text"
            } else {
                "document"
            };

            let dest_path = upload_dir.join(&file_id);
            tokio::fs::write(&dest_path, &data).await.map_err(|e| {
                eprintln!("[Upload] Failed to save file: {}", e);
                StatusCode::INTERNAL_SERVER_ERROR
            })?;

            println!("[Upload] File saved: {} ({} bytes, type: {})", file_name, file_size, file_type);

            return Ok(Json(serde_json::json!({
                "fileId": file_id,
                "fileName": file_name,
                "fileType": file_type,
                "mimeType": content_type,
                "size": file_size,
            })));
        }
    }

    Err(StatusCode::BAD_REQUEST)
}

use axum::body::Body;
use axum::response::Response;
use axum::http::header;

async fn upload_get_handler(Path(id): Path<String>) -> Result<Response<Body>, StatusCode> {
    let upload_dir = get_upload_dir();
    let file_path = upload_dir.join(&id);

    if !file_path.exists() {
        return Err(StatusCode::NOT_FOUND);
    }

    let data = tokio::fs::read(&file_path).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let ext = file_path.extension().and_then(|e| e.to_str()).unwrap_or("");
    
    let mime_type = match ext {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "pdf" => "application/pdf",
        "txt" => "text/plain",
        "md" => "text/markdown",
        "json" => "application/json",
        "xml" => "application/xml",
        "html" => "text/html",
        "css" => "text/css",
        "js" => "text/javascript",
        "csv" => "text/csv",
        _ => "application/octet-stream",
    };

    let response = Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, mime_type)
        .header(header::CONTENT_LENGTH, data.len())
        .header(header::CACHE_CONTROL, "public, max-age=31536000")
        .body(Body::from(data))
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(response)
}

async fn upload_delete_handler(Path(id): Path<String>) -> Result<Json<serde_json::Value>, StatusCode> {
    let upload_dir = get_upload_dir();
    let file_path = upload_dir.join(&id);

    if file_path.exists() {
        tokio::fs::remove_file(&file_path).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        println!("[Upload] File deleted: {}", id);
    }

    Ok(Json(serde_json::json!({ "ok": true })))
}

async fn providers_list(State(state): State<AppState>) -> Json<serde_json::Value> {
    let config_manager = state.4.clone();
    let manager: tokio::sync::MutexGuard<'_, Option<ConfigManager>> = config_manager.lock().await;
    if let Some(m) = manager.as_ref() {
        let config = m.get_config();
        let providers: Vec<serde_json::Value> = config.providers.iter().map(|p| {
            serde_json::json!({
                "id": p.id,
                "name": p.name,
                "apiKey": p.api_key,
                "baseUrl": p.base_url,
                "format": p.provider_type,
                "models": p.models.iter().map(|m| serde_json::json!({
                    "id": m.id,
                    "name": m.name,
                    "enabled": m.enabled,
                })).collect::<Vec<_>>(),
                "enabled": p.enabled,
                "supportsWebSearch": p.supports_web_search,
                "webSearchStrategy": p.web_search_strategy,
                "webSearchTestedAt": p.web_search_tested_at,
                "webSearchTestReason": p.web_search_test_reason,
            })
        }).collect();
        return Json(serde_json::json!({ "providers": providers }));
    }
    Json(serde_json::json!({ "providers": [] }))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateProviderRequest {
    name: String,
    base_url: Option<String>,
    api_key: Option<String>,
    format: Option<String>,
    models: Option<Vec<serde_json::Value>>,
    enabled: Option<bool>,
    supports_web_search: Option<bool>,
}

async fn providers_create(State(state): State<AppState>, Json(req): Json<CreateProviderRequest>) -> Json<serde_json::Value> {
    let config_manager = state.4.clone();
    let mut manager: tokio::sync::MutexGuard<'_, Option<ConfigManager>> = config_manager.lock().await;
    if let Some(m) = manager.as_mut() {
        let id = uuid::Uuid::new_v4().to_string();
        let provider_type = req.format.unwrap_or_else(|| "openai".to_string());
        let models: Vec<crate::config::ModelConfig> = req.models.unwrap_or_default().iter().map(|m| {
            crate::config::ModelConfig {
                id: m.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                name: m.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                enabled: m.get("enabled").and_then(|v| v.as_bool()).unwrap_or(true),
                max_tokens: None,
                supports_vision: false,
                supports_tools: true,
                supports_streaming: true,
                context_window: None,
                cost_per_1k_input: None,
                cost_per_1k_output: None,
            }
        }).collect();
        let provider = crate::config::ProviderConfig {
            id: id.clone(),
            name: req.name,
            provider_type,
            api_key: req.api_key,
            base_url: req.base_url.unwrap_or_default(),
            models,
            enabled: req.enabled.unwrap_or(true),
            is_default: false,
            settings: std::collections::HashMap::new(),
            supports_web_search: req.supports_web_search.unwrap_or(false),
            web_search_strategy: None,
            web_search_tested_at: None,
            web_search_test_reason: None,
        };
        match m.add_provider(provider) {
            Ok(()) => {
                let p = m.get_provider(&id);
                if let Some(created) = p {
                    Json(serde_json::json!({
                        "id": created.id,
                        "name": created.name,
                        "apiKey": created.api_key,
                        "baseUrl": created.base_url,
                        "format": created.provider_type,
                        "models": created.models.iter().map(|m| serde_json::json!({"id": m.id, "name": m.name, "enabled": m.enabled})).collect::<Vec<_>>(),
                        "enabled": created.enabled,
                        "supportsWebSearch": created.supports_web_search,
                        "webSearchStrategy": created.web_search_strategy,
                    }))
                } else {
                    Json(serde_json::json!({ "error": "Provider created but not found" }))
                }
            }
            Err(e) => Json(serde_json::json!({ "error": format!("{}", e) }))
        }
    } else {
        Json(serde_json::json!({ "error": "Config manager not initialized" }))
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct UpdateProviderRequest {
    name: Option<String>,
    base_url: Option<String>,
    api_key: Option<String>,
    format: Option<String>,
    models: Option<Vec<serde_json::Value>>,
    enabled: Option<bool>,
    supports_web_search: Option<bool>,
    web_search_strategy: Option<Option<String>>,
    web_search_tested_at: Option<Option<u64>>,
    web_search_test_reason: Option<Option<String>>,
}

async fn providers_patch(Path(id): Path<String>, State(state): State<AppState>, Json(req): Json<UpdateProviderRequest>) -> Json<serde_json::Value> {
    let config_manager = state.4.clone();
    let mut manager: tokio::sync::MutexGuard<'_, Option<ConfigManager>> = config_manager.lock().await;
    if let Some(m) = manager.as_mut() {
        if let Some(provider) = m.get_provider_mut(&id) {
            if let Some(name) = req.name { provider.name = name; }
            if let Some(base_url) = req.base_url { provider.base_url = base_url; }
            if let Some(api_key) = req.api_key { provider.api_key = Some(api_key); }
            if let Some(format) = req.format { provider.provider_type = format; }
            if let Some(enabled) = req.enabled { provider.enabled = enabled; }
            if let Some(sws) = req.supports_web_search { provider.supports_web_search = sws; }
            if let Some(wss) = req.web_search_strategy { provider.web_search_strategy = wss; }
            if let Some(wsta) = req.web_search_tested_at { provider.web_search_tested_at = wsta; }
            if let Some(wstr) = req.web_search_test_reason { provider.web_search_test_reason = wstr; }
            if let Some(models_val) = req.models {
                provider.models = models_val.iter().map(|m| {
                    crate::config::ModelConfig {
                        id: m.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                        name: m.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                        enabled: m.get("enabled").and_then(|v| v.as_bool()).unwrap_or(true),
                        max_tokens: None,
                        supports_vision: false,
                        supports_tools: true,
                        supports_streaming: true,
                        context_window: None,
                        cost_per_1k_input: None,
                        cost_per_1k_output: None,
                    }
                }).collect();
            }
            let _ = m.save();
            let p = m.get_provider(&id).unwrap();
            Json(serde_json::json!({
                "id": p.id,
                "name": p.name,
                "apiKey": p.api_key,
                "baseUrl": p.base_url,
                "format": p.provider_type,
                "models": p.models.iter().map(|m| serde_json::json!({"id": m.id, "name": m.name, "enabled": m.enabled})).collect::<Vec<_>>(),
                "enabled": p.enabled,
                "supportsWebSearch": p.supports_web_search,
                "webSearchStrategy": p.web_search_strategy,
                "webSearchTestedAt": p.web_search_tested_at,
                "webSearchTestReason": p.web_search_test_reason,
            }))
        } else {
            Json(serde_json::json!({ "error": format!("Provider '{}' not found", id) }))
        }
    } else {
        Json(serde_json::json!({ "error": "Config manager not initialized" }))
    }
}

async fn providers_delete(Path(id): Path<String>, State(state): State<AppState>) -> Json<serde_json::Value> {
    let config_manager = state.4.clone();
    let mut manager: tokio::sync::MutexGuard<'_, Option<ConfigManager>> = config_manager.lock().await;
    if let Some(m) = manager.as_mut() {
        match m.remove_provider(&id) {
            Ok(()) => Json(serde_json::json!({ "ok": true })),
            Err(e) => Json(serde_json::json!({ "error": format!("{}", e) })),
        }
    } else {
        Json(serde_json::json!({ "error": "Config manager not initialized" }))
    }
}

async fn providers_models_list(State(state): State<AppState>) -> Json<serde_json::Value> {
    let config_manager = state.4.clone();
    let manager: tokio::sync::MutexGuard<'_, Option<ConfigManager>> = config_manager.lock().await;
    if let Some(m) = manager.as_ref() {
        let config = m.get_config();
        let models: Vec<serde_json::Value> = config.providers.iter()
            .filter(|p| p.enabled)
            .flat_map(|p| {
                p.models.iter()
                    .filter(|m| m.enabled)
                    .map(|m| serde_json::json!({
                        "id": m.id,
                        "name": m.name,
                        "providerId": p.id,
                        "providerName": p.name,
                    }))
            })
            .collect();
        return Json(serde_json::json!({ "models": models }));
    }
    Json(serde_json::json!({ "models": [] }))
}

async fn providers_test_websearch(Path(id): Path<String>, State(state): State<AppState>) -> Json<serde_json::Value> {
    let config_manager = state.4.clone();
    let manager: tokio::sync::MutexGuard<'_, Option<ConfigManager>> = config_manager.lock().await;
    if let Some(m) = manager.as_ref() {
        if let Some(provider) = m.get_provider(&id) {
            let api_key = provider.api_key.clone().unwrap_or_default();
            let base_url = provider.base_url.clone();
            let provider_type = provider.provider_type.clone();
            drop(manager);

            let result = test_web_search_capability(&id, &api_key, &base_url, &provider_type).await;

            let config_manager = state.4.clone();
            let mut manager: tokio::sync::MutexGuard<'_, Option<ConfigManager>> = config_manager.lock().await;
            if let Some(m) = manager.as_mut() {
                if let Some(provider) = m.get_provider_mut(&id) {
                    provider.supports_web_search = result.get("ok").and_then(|v| v.as_bool()).unwrap_or(false);
                    provider.web_search_strategy = result.get("strategy").and_then(|v| v.as_str()).map(String::from);
                    provider.web_search_tested_at = Some(std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs());
                    provider.web_search_test_reason = result.get("reason").and_then(|v| v.as_str()).map(String::from);
                    let _ = m.save();
                }
            }
            return Json(result);
        }
    }
    Json(serde_json::json!({ "ok": false, "reason": "Provider not found" }))
}

async fn test_web_search_capability(_id: &str, _api_key: &str, _base_url: &str, _provider_type: &str) -> serde_json::Value {
    serde_json::json!({ "ok": false, "reason": "Web search test not yet implemented" })
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
    let manager: tokio::sync::MutexGuard<'_, SkillsManager> = skill_manager.lock().await;
    match manager.load_skills().await {
        Ok(skills) => Json(serde_json::json!({ "skills": skills })),
        Err(e) => Json(serde_json::json!({ "skills": [], "error": format!("{}", e) })),
    }
}

async fn skills_create(State(state): State<AppState>, Json(skill): Json<Skill>) -> Json<serde_json::Value> {
    let skill_manager = state.5.clone();
    let manager: tokio::sync::MutexGuard<'_, SkillsManager> = skill_manager.lock().await;
    match manager.create_skill(&skill.name, &skill.description, &skill.content.unwrap_or_default()) {
        Ok(_) => Json(serde_json::json!({ "ok": true })),
        Err(e) => Json(serde_json::json!({ "error": format!("{}", e) })),
    }
}

async fn skill_get(Path(name): Path<String>, State(state): State<AppState>) -> Json<serde_json::Value> {
    let skill_manager = state.5.clone();
    let manager: tokio::sync::MutexGuard<'_, SkillsManager> = skill_manager.lock().await;
    match manager.get_skill_by_id(&name).await {
        Ok(Some(skill)) => Json(serde_json::to_value(skill).unwrap_or_default()),
        Ok(None) => Json(serde_json::json!({ "error": "Skill not found" })),
        Err(e) => Json(serde_json::json!({ "error": format!("{}", e) })),
    }
}

async fn skill_update(Path(name): Path<String>, State(state): State<AppState>, Json(updates): Json<HashMap<String, serde_json::Value>>) -> Json<serde_json::Value> {
    let skill_manager = state.5.clone();
    let manager: tokio::sync::MutexGuard<'_, SkillsManager> = skill_manager.lock().await;
    match manager.update_skill(&name, updates) {
        Ok(_) => Json(serde_json::json!({ "ok": true })),
        Err(e) => Json(serde_json::json!({ "error": format!("{}", e) })),
    }
}

async fn skill_delete(Path(name): Path<String>, State(state): State<AppState>) -> Json<serde_json::Value> {
    let skill_manager = state.5.clone();
    let manager: tokio::sync::MutexGuard<'_, SkillsManager> = skill_manager.lock().await;
    match manager.delete_skill(&name) {
        Ok(_) => Json(serde_json::json!({ "ok": true })),
        Err(e) => Json(serde_json::json!({ "error": format!("{}", e) })),
    }
}

#[derive(Deserialize)]
pub struct SkillEnableRequest {
    pub enabled: bool,
}

async fn skill_enable(Path(name): Path<String>, State(state): State<AppState>, Json(_req): Json<SkillEnableRequest>) -> Json<serde_json::Value> {
    let skill_manager = state.5.clone();
    let manager: tokio::sync::MutexGuard<'_, SkillsManager> = skill_manager.lock().await;
    match manager.toggle_skill(&name).await {
        Ok(_) => Json(serde_json::json!({ "ok": true })),
        Err(e) => Json(serde_json::json!({ "error": format!("{}", e) })),
    }
}

#[derive(Deserialize)]
pub struct SkillExecuteRequest {
    pub input: String,
    pub conversation_id: Option<String>,
    pub workspace_path: Option<String>,
    pub variables: Option<serde_json::Map<String, serde_json::Value>>,
}

async fn skill_execute(
    Path(name): Path<String>,
    State(state): State<AppState>,
    Json(req): Json<SkillExecuteRequest>,
) -> Json<serde_json::Value> {
    let skill_manager = state.5.clone();
    let mcp_server_manager = state.1.clone();
    
    let manager: tokio::sync::MutexGuard<'_, SkillsManager> = skill_manager.lock().await;
    
    let input = req.input.clone();
    
    let mut context = SkillExecutionContext::default();
    context.current_input = input.clone();
    context.conversation_id = req.conversation_id.unwrap_or_default();
    context.workspace_path = req.workspace_path;
    
    if let Some(vars) = req.variables {
        for (key, value) in vars {
            if let Some(s) = value.as_str() {
                context.variables.insert(key, s.to_string());
            }
        }
    }
    
    context = context.with_mcp_manager(mcp_server_manager.clone());
    
    let mcp_tools = mcp_server_manager.get_all_tools().await;
    context.available_mcp_tools = mcp_tools;
    
    match manager.execute_skill(&name, &input, Some(context)).await {
        Ok(result) => Json(serde_json::json!({ "success": true, "result": result })),
        Err(e) => Json(serde_json::json!({ "success": false, "error": format!("{}", e) })),
    }
}

#[derive(Deserialize)]
pub struct SkillMatchRequest {
    pub input: String,
}

async fn skills_match(State(state): State<AppState>, Json(req): Json<SkillMatchRequest>) -> Json<serde_json::Value> {
    let skill_manager = state.5.clone();
    let manager: tokio::sync::MutexGuard<'_, SkillsManager> = skill_manager.lock().await;
    match manager.execute_skill("match", &req.input, None).await {
        Ok(result) => Json(serde_json::json!({ "matched": true, "result": result })),
        Err(_) => Json(serde_json::json!({ "matched": false })),
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
    let task_executor = state.7.clone();
    let executor: tokio::sync::MutexGuard<'_, Option<TaskExecutor>> = task_executor.lock().await;
    if let Some(e) = executor.as_ref() {
        let task_request = TaskRequest {
            task_id: req.task_id,
            prompt: req.prompt,
            model: req.model,
            max_tokens: req.max_tokens,
            context: req.context,
            tools: None,
        };

        match e.execute_task(task_request).await {
            Ok(result) => Ok(Json(result)),
            Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
        }
    } else {
        Err(StatusCode::SERVICE_UNAVAILABLE)
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
    let mcp_server_manager = state.1.clone();
    let servers: Vec<crate::mcp::McpServerStatus> = mcp_server_manager.list_servers().await;
    let servers_json: Vec<serde_json::Value> = servers
        .iter()
        .map(|s| serde_json::json!({
            "id": s.id,
            "name": s.name,
            "command": s.command,
            "args": s.args,
            "enabled": s.enabled,
            "running": s.running,
            "pid": s.pid,
            "tools_count": s.tools_count,
            "resources_count": s.resources_count,
            "error": s.error,
            "transport_type": s.transport_type
        }))
        .collect();

    Json(serde_json::json!({ "servers": servers_json }))
}

async fn mcp_servers_update(State(state): State<AppState>, Json(servers): Json<Vec<McpServerConfig>>) -> Json<serde_json::Value> {
    let mcp_server_manager = state.1.clone();
    
    for server in servers {
        if let Err(e) = mcp_server_manager.add_server(server).await {
            eprintln!("[Bridge] Failed to add MCP server: {}", e);
        }
    }

    Json(serde_json::json!({ "ok": true }))
}

async fn mcp_tools_list(Path(name): Path<String>, State(state): State<AppState>) -> Result<Json<serde_json::Value>, StatusCode> {
    let mcp_server_manager = state.1.clone();
    let tools: Vec<crate::mcp::McpTool> = mcp_server_manager.get_all_tools().await;
    
    let tools_json: Vec<serde_json::Value> = tools
        .iter()
        .filter(|t| t.server_name == name)
        .map(|t| serde_json::json!({
            "name": t.name,
            "description": t.description,
            "input_schema": t.input_schema
        }))
        .collect();

    Ok(Json(serde_json::json!({ "tools": tools_json })))
}

async fn mcp_resources_list(Path(name): Path<String>, State(state): State<AppState>) -> Result<Json<serde_json::Value>, StatusCode> {
    let mcp_server_manager = state.1.clone();
    let resources: Vec<crate::mcp::McpResource> = mcp_server_manager.get_all_resources().await;

    let resources_json: Vec<serde_json::Value> = resources
        .iter()
        .map(|r| serde_json::json!({
            "uri": r.uri,
            "name": r.name,
            "mime_type": r.mime_type
        }))
        .collect();

    Ok(Json(serde_json::json!({ "resources": resources_json })))
}

async fn mcp_resource_read(Path((name, uri)): Path<(String, String)>, State(state): State<AppState>) -> Result<Json<serde_json::Value>, StatusCode> {
    let mcp_server_manager = state.1.clone();
    
    match mcp_server_manager.read_resource(&name, &uri, None).await {
        Ok(content) => Ok(Json(serde_json::json!({
            "uri": content.uri,
            "content": content.content,
            "content_type": content.content_type,
            "metadata": content.metadata
        }))),
        Err(e) => {
            eprintln!("[Bridge] Failed to read resource: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

async fn mcp_resource_monitor(Path((name, uri)): Path<(String, String)>, State(state): State<AppState>) -> Result<Json<serde_json::Value>, StatusCode> {
    let mcp_server_manager = state.1.clone();
    
    match mcp_server_manager.monitor_resource(&name, &uri, true).await {
        Ok(enabled) => Ok(Json(serde_json::json!({
            "uri": uri,
            "enabled": enabled
        }))),
        Err(e) => {
            eprintln!("[Bridge] Failed to monitor resource: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

async fn mcp_connect_handler(Path(name): Path<String>, State(state): State<AppState>) -> Result<Json<serde_json::Value>, StatusCode> {
    let mcp_server_manager = state.1.clone();
    
    match mcp_server_manager.start_server(&name).await {
        Ok(_) => {
            if let Some(status) = mcp_server_manager.get_server(&name).await {
                Ok(Json(serde_json::json!({
                    "ok": true,
                    "name": status.name,
                    "status": if status.running { "running" } else { "ready" },
                    "tools_count": status.tools_count,
                    "resources_count": status.resources_count
                })))
            } else {
                Err(StatusCode::NOT_FOUND)
            }
        },
        Err(e) => {
            eprintln!("[Bridge] Failed to connect MCP server: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

async fn mcp_disconnect_handler(Path(name): Path<String>, State(state): State<AppState>) -> Result<Json<serde_json::Value>, StatusCode> {
    let mcp_server_manager = state.1.clone();
    
    match mcp_server_manager.stop_server(&name).await {
        Ok(_) => Ok(Json(serde_json::json!({ "ok": true }))),
        Err(e) => {
            eprintln!("[Bridge] Failed to disconnect MCP server: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
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

async fn stream_events_handler(Path(conv_id): Path<String>, State(state): State<AppState>) -> Result<impl IntoResponse, StatusCode> {
    let stream_manager = state.2.clone();
    let mut manager: tokio::sync::MutexGuard<'_, StreamManager> = stream_manager.lock().await;

    let receiver = manager.add_listener(&conv_id)
        .ok_or_else(|| StatusCode::NOT_FOUND)?;

    let stream = async_stream::stream! {
        let mut rx = receiver;
        while let Ok(event) = rx.recv().await {
            let event_name = event.event_type;
            let data = serde_json::to_string(&event.data).unwrap_or_default();
            yield Ok::<Event, Infallible>(Event::default()
                .event(&event_name)
                .data(data));
        }
    };

    let mut response = Sse::new(stream).keep_alive(KeepAlive::default()).into_response();
    response.headers_mut().insert(
        CONTENT_TYPE,
        "text/event-stream; charset=utf-8".parse().unwrap(),
    );
    Ok(response)
}

async fn research_start_handler(State(state): State<AppState>, Json(req): Json<ChatRequest>) -> Json<serde_json::Value> {
    let research_id = uuid::Uuid::new_v4().to_string();
    let native_engine = state.14.clone();
    let config_manager = state.4.clone();
    let active_research = state.15.clone();

    let model = if req.model.is_empty() { "claude-sonnet-4-20250514".to_string() } else { req.model.clone() };
    let query = req.get_messages().last()
        .and_then(|m| m.get("content").and_then(|c| c.as_str()).map(String::from))
        .unwrap_or_default();

    let providers_to_sync = {
        let cm_guard: tokio::sync::MutexGuard<'_, Option<ConfigManager>> = config_manager.lock().await;
        if let Some(cm) = cm_guard.as_ref() {
            cm.get_config().providers.iter().map(|p| {
                crate::native_engine::provider_manager::Provider {
                    id: p.id.clone(),
                    name: p.name.clone(),
                    base_url: p.base_url.clone(),
                    api_key: p.api_key.clone().unwrap_or_default(),
                    api_format: {
                        let is_deepseek = p.base_url.contains("deepseek");
                        if p.provider_type == "anthropic" && !is_deepseek {
                            crate::native_engine::provider_manager::ApiFormat::Anthropic
                        } else {
                            crate::native_engine::provider_manager::ApiFormat::OpenAI
                        }
                    },
                    models: p.models.iter().map(|m| crate::native_engine::provider_manager::ModelConfig {
                        id: m.id.clone(),
                        name: m.name.clone(),
                        enabled: m.enabled,
                        max_tokens: m.max_tokens,
                        supports_vision: m.supports_vision,
                        supports_web_search: false,
                    }).collect(),
                    enabled: p.enabled,
                    web_search_strategy: p.web_search_strategy.clone(),
                }
            }).collect::<Vec<_>>()
        } else {
            Vec::new()
        }
    };

    let resolved = {
        let mut engine_guard: tokio::sync::MutexGuard<'_, Option<NativeEngine>> = native_engine.lock().await;
        if let Some(engine) = engine_guard.as_mut() {
            engine.sync_providers(providers_to_sync).await;
            engine.resolve_provider(&model).await
        } else {
            None
        }
    };

    let resolved = match resolved {
        Some(r) => r,
        None => return Json(serde_json::json!({ "ok": false, "error": format!("No provider found for model: {}", model) })),
    };

    let api_key = resolved.provider.api_key.clone();
    let base_url = resolved.provider.base_url.clone();

    let (bcast_tx, _) = broadcast::channel::<ResearchEvent>(256);
    let (mpsc_tx, mut mpsc_rx) = tokio::sync::mpsc::unbounded_channel::<ResearchEvent>();

    let bcast_tx_clone = bcast_tx.clone();
    let research_request = ResearchRequest {
        query: query.clone(),
        api_key,
        base_url,
        model,
    };

    let handle = tokio::spawn(async move {
        let bcast = bcast_tx_clone.clone();
        let forward_handle = tokio::spawn(async move {
            while let Some(event) = mpsc_rx.recv().await {
                let _ = bcast.send(event);
            }
        });

        let orchestrator = ResearchOrchestrator::new(reqwest::Client::new());
        if let Err(e) = orchestrator.run_pipeline(research_request, mpsc_tx).await {
            eprintln!("[Research] Pipeline error: {}", e);
        }

        let _ = forward_handle.await;
    });

    {
        let mut research: tokio::sync::MutexGuard<'_, HashMap<String, ResearchTask>> = active_research.lock().await;
        research.insert(research_id.clone(), ResearchTask {
            handle,
            event_tx: bcast_tx,
        });
    }

    Json(serde_json::json!({ "ok": true, "research_id": research_id }))
}

async fn research_stop_handler(Path(id): Path<String>, State(state): State<AppState>) -> Json<serde_json::Value> {
    let active_research = state.15.clone();
    let mut research: tokio::sync::MutexGuard<'_, HashMap<String, ResearchTask>> = active_research.lock().await;
    if let Some(task) = research.remove(&id) {
        task.handle.abort();
        Json(serde_json::json!({ "ok": true }))
    } else {
        Json(serde_json::json!({ "ok": false, "error": "Research task not found" }))
    }
}

async fn research_status_handler(Path(id): Path<String>, State(state): State<AppState>) -> Json<serde_json::Value> {
    let active_research = state.15.clone();
    let research: tokio::sync::MutexGuard<'_, HashMap<String, ResearchTask>> = active_research.lock().await;
    if let Some(task) = research.get(&id) {
        if task.handle.is_finished() {
            Json(serde_json::json!({ "status": "Completed" }))
        } else {
            Json(serde_json::json!({ "status": "Running" }))
        }
    } else {
        Json(serde_json::json!({ "status": "NotFound" }))
    }
}

async fn research_events_handler(
    Path(id): Path<String>,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, StatusCode> {
    let active_research = state.15.clone();
    let research: tokio::sync::MutexGuard<'_, HashMap<String, ResearchTask>> = active_research.lock().await;
    let task = research.get(&id).ok_or(StatusCode::NOT_FOUND)?;
    let mut rx = task.event_tx.subscribe();
    drop(research);

    let stream = async_stream::stream! {
        while let Ok(event) = rx.recv().await {
            let data = serde_json::to_string(&event).unwrap_or_default();
            yield Ok::<Event, Infallible>(Event::default()
                .event("research")
                .data(data));
        }
    };

    let mut response = Sse::new(stream).keep_alive(KeepAlive::default()).into_response();
    response.headers_mut().insert(
        CONTENT_TYPE,
        "text/event-stream; charset=utf-8".parse().unwrap(),
    );
    Ok(response)
}

#[derive(Deserialize)]
struct MultiAgentResearchRequest {
    query: String,
    model: Option<String>,
}

async fn multiagent_research_handler(
    State(state): State<AppState>,
    Json(req): Json<MultiAgentResearchRequest>,
) -> Json<serde_json::Value> {
    let native_engine = state.14.clone();
    let config_manager = state.4.clone();

    let model = req.model.unwrap_or_else(|| "claude-sonnet-4-20250514".to_string());

    let providers_to_sync = {
        let cm_guard: tokio::sync::MutexGuard<'_, Option<ConfigManager>> = config_manager.lock().await;
        if let Some(cm) = cm_guard.as_ref() {
            cm.get_config().providers.iter().map(|p| {
                crate::native_engine::provider_manager::Provider {
                    id: p.id.clone(),
                    name: p.name.clone(),
                    base_url: p.base_url.clone(),
                    api_key: p.api_key.clone().unwrap_or_default(),
                    api_format: {
                        let is_deepseek = p.base_url.contains("deepseek");
                        if p.provider_type == "anthropic" && !is_deepseek {
                            crate::native_engine::provider_manager::ApiFormat::Anthropic
                        } else {
                            crate::native_engine::provider_manager::ApiFormat::OpenAI
                        }
                    },
                    models: p.models.iter().map(|m| crate::native_engine::provider_manager::ModelConfig {
                        id: m.id.clone(),
                        name: m.name.clone(),
                        enabled: m.enabled,
                        max_tokens: m.max_tokens,
                        supports_vision: m.supports_vision,
                        supports_web_search: false,
                    }).collect(),
                    enabled: p.enabled,
                    web_search_strategy: p.web_search_strategy.clone(),
                }
            }).collect::<Vec<_>>()
        } else {
            Vec::new()
        }
    };

    let resolved = {
        let mut engine_guard: tokio::sync::MutexGuard<'_, Option<NativeEngine>> = native_engine.lock().await;
        if let Some(engine) = engine_guard.as_mut() {
            engine.sync_providers(providers_to_sync).await;
            engine.resolve_provider(&model).await
        } else {
            None
        }
    };

    let resolved = match resolved {
        Some(r) => r,
        None => return Json(serde_json::json!({ "ok": false, "error": format!("No provider found for model: {}", model) })),
    };

    let orchestrator = PipelineOrchestrator::new(OrchestratorConfig::default());
    match orchestrator.execute_research(req.query, &resolved).await {
        Ok(result) => Json(serde_json::json!({ "ok": true, "result": result })),
        Err(e) => Json(serde_json::json!({ "ok": false, "error": format!("{}", e) })),
    }
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

async fn computer_use_screen_info() -> Json<serde_json::Value> {
    let manager = crate::computer_use::ComputerUseManager::new(crate::computer_use::ComputerUseConfig::default());
    let info = manager.get_screen_info();
    Json(serde_json::json!({
        "width": info.width,
        "height": info.height,
        "scaleFactor": info.scale_factor,
    }))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ComputerUseRequest {
    action_type: String,
    coordinate: Option<[i32; 2]>,
    button: Option<String>,
    key: Option<String>,
    text: Option<String>,
    scroll_y: Option<i32>,
    scroll_x: Option<i32>,
    duration_ms: Option<u64>,
}

async fn computer_use_execute(Json(req): Json<ComputerUseRequest>) -> Json<serde_json::Value> {
    let manager = crate::computer_use::ComputerUseManager::new(crate::computer_use::ComputerUseConfig::default());
    let action = crate::computer_use::ComputerAction {
        action_type: match req.action_type.as_str() {
            "mouse_move" => crate::computer_use::ComputerActionType::MouseMove,
            "mouse_click" => crate::computer_use::ComputerActionType::MouseClick,
            "mouse_down" => crate::computer_use::ComputerActionType::MouseDown,
            "mouse_up" => crate::computer_use::ComputerActionType::MouseUp,
            "mouse_scroll" => crate::computer_use::ComputerActionType::MouseScroll,
            "key_press" => crate::computer_use::ComputerActionType::KeyPress,
            "key_down" => crate::computer_use::ComputerActionType::KeyDown,
            "key_up" => crate::computer_use::ComputerActionType::KeyUp,
            "type_text" => crate::computer_use::ComputerActionType::TypeText,
            "screenshot" => crate::computer_use::ComputerActionType::Screenshot,
            "wait" => crate::computer_use::ComputerActionType::Wait,
            _ => crate::computer_use::ComputerActionType::Wait,
        },
        coordinate: req.coordinate.map(|c| crate::computer_use::ScreenCoordinate { x: c[0], y: c[1] }),
        button: req.button.map(|b| match b.as_str() {
            "right" => crate::computer_use::MouseButton::Right,
            "middle" => crate::computer_use::MouseButton::Middle,
            _ => crate::computer_use::MouseButton::Left,
        }),
        key: req.key,
        text: req.text,
        scroll_y: req.scroll_y,
        scroll_x: req.scroll_x,
        duration_ms: req.duration_ms,
    };
    match manager.execute_action(action).await {
        Ok(result) => Json(serde_json::json!({
            "ok": result.success,
            "screenshot": result.screenshot,
            "error": result.error,
            "durationMs": result.duration_ms,
        })),
        Err(e) => Json(serde_json::json!({ "ok": false, "error": format!("{}", e) })),
    }
}

async fn computer_use_screenshot() -> Json<serde_json::Value> {
    let manager = crate::computer_use::ComputerUseManager::new(crate::computer_use::ComputerUseConfig::default());
    let action = crate::computer_use::ComputerAction {
        action_type: crate::computer_use::ComputerActionType::Screenshot,
        coordinate: None,
        button: None,
        key: None,
        text: None,
        scroll_y: None,
        scroll_x: None,
        duration_ms: None,
    };
    match manager.execute_action(action).await {
        Ok(result) => Json(serde_json::json!({ "ok": result.success, "screenshot": result.screenshot, "error": result.error })),
        Err(e) => Json(serde_json::json!({ "ok": false, "error": format!("{}", e) })),
    }
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

#[derive(Debug, Serialize, Deserialize)]
pub struct WorkflowExecuteRequest {
    pub goal: String,
    pub provider_id: Option<String>,
    pub model: Option<String>,
}

async fn workflow_execute(
    State(state): State<AppState>,
    Json(req): Json<WorkflowExecuteRequest>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let config_manager: Arc<Mutex<Option<ConfigManager>>> = state.4.clone();
    let orchestrator: Arc<Mutex<Option<crate::orchestration::MultiAgentOrchestrator>>> = state.16.clone();
    
    let orchestrator_guard: tokio::sync::MutexGuard<'_, Option<crate::orchestration::MultiAgentOrchestrator>> = orchestrator.lock().await;
    let orchestrator = orchestrator_guard.as_ref()
        .ok_or_else(|| {
            eprintln!("[Bridge] Orchestrator not initialized");
            StatusCode::SERVICE_UNAVAILABLE
        })?;

    let config_guard: tokio::sync::MutexGuard<'_, Option<ConfigManager>> = config_manager.lock().await;
    let config = config_guard.as_ref()
        .ok_or_else(|| {
            eprintln!("[Bridge] Config manager not initialized");
            StatusCode::SERVICE_UNAVAILABLE
        })?;

    let provider_config = match req.provider_id {
        Some(id) => config.get_provider(&id),
        None => config.get_default_provider(),
    };

    let provider_config = provider_config
        .ok_or_else(|| {
            eprintln!("[Bridge] No provider configured");
            StatusCode::BAD_REQUEST
        })?;

    let model_config = provider_config.models.first()
        .ok_or_else(|| {
            eprintln!("[Bridge] No model configured for provider");
            StatusCode::BAD_REQUEST
        })?;

    let api_format = if provider_config.provider_type.to_lowercase() == "anthropic" {
        crate::native_engine::provider_manager::ApiFormat::Anthropic
    } else {
        crate::native_engine::provider_manager::ApiFormat::OpenAI
    };

    let provider = crate::native_engine::provider_manager::Provider {
        id: provider_config.id.clone(),
        name: provider_config.name.clone(),
        base_url: provider_config.base_url.clone(),
        api_key: provider_config.api_key.clone().unwrap_or_default(),
        api_format,
        models: provider_config.models.iter().map(|m| crate::native_engine::provider_manager::ModelConfig {
            id: m.id.clone(),
            name: m.name.clone(),
            enabled: m.enabled,
            max_tokens: m.max_tokens,
            supports_vision: m.supports_vision,
            supports_web_search: false,
        }).collect(),
        enabled: provider_config.enabled,
        web_search_strategy: provider_config.web_search_strategy.clone(),
    };

    let model = crate::native_engine::provider_manager::ModelConfig {
        id: model_config.id.clone(),
        name: model_config.name.clone(),
        enabled: model_config.enabled,
        max_tokens: model_config.max_tokens,
        supports_vision: model_config.supports_vision,
        supports_web_search: false,
    };

    let resolved_provider = crate::native_engine::provider_manager::ResolvedProvider {
        provider,
        model,
    };

    match orchestrator.execute_workflow(&req.goal, &resolved_provider).await {
        Ok(result) => Ok(Json(serde_json::json!({ "success": true, "result": result }))),
        Err(e) => {
            eprintln!("[Bridge] Workflow execution failed: {}", e);
            Ok(Json(serde_json::json!({ "success": false, "error": format!("{}", e) })))
        }
    }
}

async fn workflow_stats(
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let orchestrator: Arc<Mutex<Option<crate::orchestration::MultiAgentOrchestrator>>> = state.16.clone();
    
    let orchestrator_guard: tokio::sync::MutexGuard<'_, Option<crate::orchestration::MultiAgentOrchestrator>> = orchestrator.lock().await;
    if let Some(orchestrator) = orchestrator_guard.as_ref() {
        let stats: serde_json::Value = orchestrator.get_scheduling_stats().await;
        Ok(Json(stats))
    } else {
        Ok(Json(serde_json::json!({ "success": false, "error": "Orchestrator not initialized" })))
    }
}

async fn workflow_config_get() -> Result<Json<serde_json::Value>, StatusCode> {
    let config_path = std::path::Path::new("config/orchestration.toml");
    let config = OrchestratorConfigFile::load_or_default(config_path);
    Ok(Json(serde_json::json!({ "success": true, "config": config })))
}

async fn workflow_config_set(
    Json(config): Json<OrchestratorConfigFile>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let config_path = std::path::Path::new("config/orchestration.toml");
    let config_dir = config_path.parent().unwrap_or(std::path::Path::new("."));
    std::fs::create_dir_all(config_dir).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    
    config.save_to_file(config_path).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    
    Ok(Json(serde_json::json!({ "success": true })))
}
