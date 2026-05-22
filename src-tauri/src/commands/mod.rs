use serde::{Serialize, Deserialize};
use std::collections::HashMap;
use tauri::{AppHandle, Manager, Emitter};

#[derive(Serialize)]
pub struct PlatformInfo {
    pub os: String,
    pub arch: String,
    pub is_electron: bool,
}

#[tauri::command]
pub async fn get_platform() -> Result<PlatformInfo, String> {
    Ok(PlatformInfo {
        os: std::env::consts::OS.to_string(),
        arch: std::env::consts::ARCH.to_string(),
        is_electron: false,
    })
}

#[tauri::command]
pub async fn get_app_path(app: AppHandle) -> Result<String, String> {
    let path = app.path().app_data_dir().map_err(|e| e.to_string())?;
    Ok(path.to_string_lossy().to_string())
}

#[tauri::command]
pub async fn select_directory(app: AppHandle) -> Result<Option<String>, String> {
    use tauri_plugin_dialog::{DialogExt, FilePath};
    use tokio::sync::oneshot;

    let (tx, rx) = oneshot::channel::<Option<FilePath>>();

    #[cfg(not(mobile))]
    {
        app.dialog().file().pick_folder(move |dir| {
            let _ = tx.send(dir);
        });
    }

    #[cfg(mobile)]
    {
        app.dialog().file().pick_file(move |file| {
            let _ = tx.send(file);
        });
    }

    let dir = rx.await.map_err(|e| e.to_string())?;
    Ok(dir.map(|p| p.to_string()))
}

#[tauri::command]
pub async fn show_item_in_folder(path: String) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg("-R")
            .arg(&path)
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("explorer")
            .arg(format!("/select,{}", path))
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    #[cfg(target_os = "linux")]
    {
        std::process::Command::new("xdg-open")
            .arg(
                std::path::Path::new(&path)
                    .parent()
                    .unwrap_or(std::path::Path::new(".")),
            )
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
pub async fn open_folder(path: String) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(&path)
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("explorer")
            .arg(&path)
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    #[cfg(target_os = "linux")]
    {
        std::process::Command::new("xdg-open")
            .arg(&path)
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
pub async fn open_external_url(url: String) -> Result<(), String> {
    open::that(&url).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn resize_window(app: AppHandle, width: f64, height: f64) -> Result<(), String> {
    #[cfg(not(mobile))]
    {
        if let Some(window) = app.get_webview_window("main") {
            let _ = window
                .set_size(tauri::LogicalSize::new(width, height));
        }
    }
    let _ = (width, height);
    Ok(())
}

#[tauri::command]
pub async fn show_main_window(app: AppHandle) -> Result<(), String> {
    #[cfg(not(mobile))]
    {
        if let Some(window) = app.get_webview_window("main") {
            let _ = window.show();
            let _ = window.set_focus();
        }
    }
    Ok(())
}

#[tauri::command]
pub async fn export_workspace(
    app: AppHandle,
    _workspace_id: String,
    context_markdown: String,
    default_filename: String,
) -> Result<String, String> {
    let download_dir = app.path().download_dir().map_err(|e| e.to_string())?;
    let output_path = download_dir.join(&default_filename);

    std::fs::write(&output_path, context_markdown).map_err(|e| e.to_string())?;

    Ok(output_path.to_string_lossy().to_string())
}

#[derive(Serialize)]
pub struct SystemStatusResult {
    pub platform: String,
    pub git_bash: GitBashStatusResult,
}

#[derive(Serialize)]
pub struct GitBashStatusResult {
    pub required: bool,
    pub found: bool,
    pub path: Option<String>,
}

#[tauri::command]
pub async fn get_system_status() -> Result<SystemStatusResult, String> {
    let platform = std::env::consts::OS.to_string();
    let git_bash_path = find_git_bash();

    Ok(SystemStatusResult {
        platform,
        git_bash: GitBashStatusResult {
            required: cfg!(target_os = "windows"),
            found: git_bash_path.is_some(),
            path: git_bash_path,
        },
    })
}

fn find_git_bash() -> Option<String> {
    if cfg!(target_os = "windows") {
        let candidates = [
            r"C:\Program Files\Git\bin\bash.exe",
            r"C:\Program Files (x86)\Git\bin\bash.exe",
        ];
        for path in &candidates {
            if std::path::Path::new(path).exists() {
                return Some(path.to_string());
            }
        }
    }
    None
}

#[tauri::command]
pub async fn chat_send(
    app: AppHandle,
    conversation_id: String,
    message: String,
    model: Option<String>,
) -> Result<String, String> {
    let state = app.state::<Arc<Mutex<Option<NativeEngine>>>>();
    let engine_guard = state.lock().await;
    let engine = engine_guard.as_ref().ok_or("Native engine not initialized")?;

    let chat_request = ChatRequest {
        conversation_id: conversation_id.clone(),
        messages: vec![serde_json::json!({
            "role": "user",
            "content": message
        })],
        model: model.unwrap_or_default(),
        system_prompt: None,
        max_tokens: None,
        workspace_path: None,
        temperature: None,
        top_p: None,
    };

    let mut rx = engine.send_message(chat_request).await.map_err(|e| e.to_string())?;

    let mut full_text = String::new();
    while let Some(event) = rx.recv().await {
        match event {
            EngineEvent::Text(text) => full_text.push_str(&text),
            EngineEvent::Error(err) => return Err(err),
            EngineEvent::MessageStop { .. } => break,
            _ => {}
        }
    }

    Ok(full_text)
}

#[tauri::command]
pub async fn chat_stream(
    app: AppHandle,
    conversation_id: String,
    message: String,
    model: Option<String>,
) -> Result<String, String> {
    let state = app.state::<Arc<Mutex<Option<NativeEngine>>>>();
    let engine_guard = state.lock().await;
    let engine = engine_guard.as_ref().ok_or("Native engine not initialized")?;

    let conv_id = conversation_id.clone();

    let chat_request = ChatRequest {
        conversation_id: conversation_id.clone(),
        messages: vec![serde_json::json!({
            "role": "user",
            "content": message
        })],
        model: model.unwrap_or_default(),
        system_prompt: None,
        max_tokens: None,
        workspace_path: None,
        temperature: None,
        top_p: None,
    };

    let mut rx = engine.send_message(chat_request).await.map_err(|e| e.to_string())?;

    let app_clone = app.clone();
    let conv_id_clone = conv_id.clone();

    tokio::spawn(async move {
        while let Some(event) = rx.recv().await {
            let event_name = match &event {
                EngineEvent::Text(_) => "engine:text",
                EngineEvent::Thinking(_) => "engine:thinking",
                EngineEvent::ToolUseStart { .. } => "engine:tool_use_start",
                EngineEvent::ToolArgDelta { .. } => "engine:tool_arg_delta",
                EngineEvent::ToolUseDone { .. } => "engine:tool_use_done",
                EngineEvent::MessageStart { .. } => "engine:message_start",
                EngineEvent::MessageDelta { .. } => "engine:message_delta",
                EngineEvent::MessageStop { .. } => "engine:message_stop",
                EngineEvent::Error(_) => "engine:error",
                EngineEvent::Usage(_) => "engine:usage",
                EngineEvent::AskUser { .. } => "engine:ask_user",
            };

            let payload = serde_json::json!({
                "conversation_id": conv_id_clone,
                "event": match event {
                    EngineEvent::Text(text) => serde_json::json!({ "type": "text", "text": text }),
                    EngineEvent::Thinking(text) => serde_json::json!({ "type": "thinking", "text": text }),
                    EngineEvent::ToolUseStart { tool_use_id, tool_name, tool_input, text_before } => {
                        serde_json::json!({ "type": "tool_use_start", "tool_use_id": tool_use_id, "tool_name": tool_name, "tool_input": tool_input, "textBefore": text_before })
                    }
                    EngineEvent::ToolArgDelta { tool_use_id, delta } => {
                        serde_json::json!({ "type": "tool_arg_delta", "tool_use_id": tool_use_id, "delta": delta })
                    }
                    EngineEvent::ToolUseDone { tool_use_id, tool_name, tool_input, output, is_error } => {
                        serde_json::json!({ "type": "tool_use_done", "tool_use_id": tool_use_id, "tool_name": tool_name, "tool_input": tool_input, "output": output, "is_error": is_error })
                    }
                    EngineEvent::MessageStart { model } => serde_json::json!({ "type": "message_start", "model": model }),
                    EngineEvent::MessageDelta { stop_reason } => serde_json::json!({ "type": "message_delta", "stop_reason": stop_reason }),
                    EngineEvent::MessageStop { full_text, stop_reason } => serde_json::json!({ "type": "message_stop", "full_text": full_text, "stop_reason": stop_reason }),
                    EngineEvent::Error(error) => serde_json::json!({ "type": "error", "error": error }),
                    EngineEvent::Usage(usage) => serde_json::json!({ "type": "usage", "usage": usage }),
                    EngineEvent::AskUser { question, options } => serde_json::json!({ "type": "ask_user", "question": question, "options": options }),
                }
            });

            let _ = app_clone.emit(event_name, payload);
        }
    });

    Ok("streaming_started".to_string())
}

#[tauri::command]
pub async fn execute_tool(
    name: String,
    input: serde_json::Value,
    cwd: Option<String>,
) -> Result<serde_json::Value, String> {
    let cwd = cwd.unwrap_or_else(|| ".".to_string());
    crate::tools::execute_tool(&name, input, &cwd).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn check_update(_app: AppHandle) -> Result<serde_json::Value, String> {
    Ok(serde_json::json!({ "has_update": false }))
}

#[tauri::command]
pub async fn install_update(app: AppHandle) -> Result<(), String> {
    app.restart();
}

#[derive(Serialize)]
pub struct SlashCommandInfo {
    pub name: String,
    pub description: String,
    pub category: String,
}

#[tauri::command]
pub async fn list_slash_commands() -> Result<Vec<SlashCommandInfo>, String> {
    let registry = crate::slash_commands::SlashCommandRegistry::new();
    Ok(registry.list_commands().iter().map(|cmd| SlashCommandInfo {
        name: cmd.name.clone(),
        description: cmd.description.clone(),
        category: cmd.category.clone(),
    }).collect())
}

#[tauri::command]
pub async fn search_slash_commands(query: String) -> Result<Vec<SlashCommandInfo>, String> {
    let registry = crate::slash_commands::SlashCommandRegistry::new();
    Ok(registry.search_commands(&query).iter().map(|cmd| SlashCommandInfo {
        name: cmd.name.clone(),
        description: cmd.description.clone(),
        category: cmd.category.clone(),
    }).collect())
}

#[tauri::command]
pub async fn get_slash_command_categories() -> Result<Vec<String>, String> {
    let registry = crate::slash_commands::SlashCommandRegistry::new();
    Ok(registry.get_categories())
}

#[derive(Serialize)]
pub struct CostSummaryResult {
    pub total_input_tokens: u64,
    pub total_output_tokens: u64,
    pub total_tokens: u64,
    pub estimated_total_cost: f64,
    pub model_breakdown: std::collections::HashMap<String, crate::cost_tracker::TokenUsage>,
}

#[tauri::command]
pub async fn get_cost_summary(conversation_id: String) -> Result<CostSummaryResult, String> {
    let data_dir = std::env::var("APPDATA")
        .or_else(|_| std::env::var("HOME").map(|h| format!("{}/.local/share", h)))
        .unwrap_or_else(|_| ".".to_string());
    let store_dir = std::path::PathBuf::from(data_dir).join("Claude Desktop").join("costs");
    let tracker = crate::cost_tracker::CostTracker::new(store_dir);
    let summary = tracker.get_conversation_cost(&conversation_id).await;
    Ok(CostSummaryResult {
        total_input_tokens: summary.total_input_tokens,
        total_output_tokens: summary.total_output_tokens,
        total_tokens: summary.total_tokens,
        estimated_total_cost: summary.estimated_total_cost,
        model_breakdown: summary.model_breakdown,
    })
}

#[derive(Serialize)]
pub struct SessionCostResult {
    pub session_id: String,
    pub model: String,
    pub total_input_tokens: u64,
    pub total_output_tokens: u64,
    pub estimated_cost: f64,
    pub started_at: String,
}

#[tauri::command]
pub async fn get_all_session_costs() -> Result<Vec<SessionCostResult>, String> {
    let data_dir = std::env::var("APPDATA")
        .or_else(|_| std::env::var("HOME").map(|h| format!("{}/.local/share", h)))
        .unwrap_or_else(|_| ".".to_string());
    let store_dir = std::path::PathBuf::from(data_dir).join("Claude Desktop").join("costs");
    let tracker = crate::cost_tracker::CostTracker::new(store_dir);
    let sessions: Vec<crate::cost_tracker::SessionCost> = tracker.get_all_sessions().await;
    Ok(sessions.into_iter().map(|s| SessionCostResult {
        session_id: s.session_id,
        model: s.model,
        total_input_tokens: s.total_input_tokens,
        total_output_tokens: s.total_output_tokens,
        estimated_cost: s.estimated_cost,
        started_at: s.started_at,
    }).collect())
}

use crate::db::DbManager;
use crate::native_engine::engine_core::{ChatRequest, NativeEngine};
use crate::native_engine::provider_manager::ProviderManager;
use crate::native_engine::tool_loop::EngineEvent;
use crate::mcp::{McpServerManager, McpToolRegistry};
use crate::permissions::{AuditLogger, PermissionManager};
use std::sync::Arc;
use tokio::sync::Mutex;

#[derive(Serialize)]
pub struct NativeEngineState {
    pub initialized: bool,
    pub provider_count: usize,
    pub conversation_count: usize,
}

#[tauri::command]
pub async fn native_engine_init(app: AppHandle) -> Result<NativeEngineState, String> {
    let data_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    let claude_dir = data_dir.join("Claude Desktop");
    
    let providers_path = claude_dir.join("providers.json");
    let workspaces_dir = claude_dir.join("workspaces");
    
    if !workspaces_dir.exists() {
        std::fs::create_dir_all(&workspaces_dir).map_err(|e| e.to_string())?;
    }

    let db_path = claude_dir.join("claude_desktop.db");
    let db_manager = Arc::new(DbManager::new(db_path).map_err(|e: anyhow::Error| e.to_string())?);
    db_manager.init().map_err(|e: anyhow::Error| e.to_string())?;
    {
        let db_mgr = db_manager.clone();
        let claude_dir_clone = claude_dir.clone();
        tokio::task::spawn_blocking(move || {
            db_mgr.with_conn(|conn| {
                if let Err(e) = crate::db::migration::check_and_migrate(&claude_dir_clone, conn) {
                    eprintln!("[NativeEngine] Migration warning: {}", e);
                }
            })
        }).await.map_err(|e| e.to_string())?;
    }
    
    let provider_manager = Arc::new(Mutex::new(ProviderManager::new(providers_path)));
    
    let pm_state = app.state::<Arc<Mutex<Option<Arc<Mutex<ProviderManager>>>>>>();
    let mut pm_guard = pm_state.lock().await;
    *pm_guard = Some(provider_manager.clone());

    let audit_logger = Arc::new(AuditLogger::new(1000));
    let permission_manager = Arc::new(PermissionManager::new(audit_logger));

    let mcp_state = app.state::<Arc<Mutex<Option<Arc<Mutex<McpServerManager>>>>>>();
    let mcp_guard = mcp_state.lock().await;
    
    let mut engine = NativeEngine::new(
        provider_manager.clone(),
        db_manager.clone(),
        workspaces_dir,
        permission_manager,
    );
    
    if let Some(mcp_manager) = mcp_guard.as_ref() {
        let registry = Arc::new(McpToolRegistry::new(mcp_manager.clone()));
        engine = engine.with_mcp_registry(registry);
    }
    
    let state = app.state::<Arc<Mutex<Option<NativeEngine>>>>();
    let mut guard = state.lock().await;
    *guard = Some(engine);
    
    let provider_count = provider_manager.lock().await.list_providers().len();
    let db = db_manager.clone();
    let conversation_count: usize = tokio::task::spawn_blocking(move || {
        db.with_conn(|conn| crate::db::conversation_repo::list_conversations(conn).map(|c: Vec<_>| c.len()).unwrap_or(0)).unwrap_or(0)
    }).await.unwrap_or(0);
    
    Ok(NativeEngineState {
        initialized: true,
        provider_count,
        conversation_count,
    })
}

#[derive(Deserialize)]
pub struct NativeChatRequest {
    pub conversation_id: String,
    pub messages: Vec<serde_json::Value>,
    pub model: String,
    pub system_prompt: Option<String>,
    pub max_tokens: Option<u32>,
}

#[derive(Serialize)]
pub struct NativeChatResponse {
    pub conversation_id: String,
    pub status: String,
}

#[tauri::command]
pub async fn native_chat(
    app: AppHandle,
    request: NativeChatRequest,
) -> Result<NativeChatResponse, String> {
    let state = app.state::<Arc<Mutex<Option<NativeEngine>>>>();
    let engine_guard = state.lock().await;
    let engine = engine_guard.as_ref().ok_or("Native engine not initialized")?;
    
    let conv_id = request.conversation_id.clone();
    
    let chat_request = ChatRequest {
        conversation_id: request.conversation_id,
        messages: request.messages,
        model: request.model,
        system_prompt: request.system_prompt,
        max_tokens: request.max_tokens,
        workspace_path: None,
        temperature: None,
        top_p: None,
    };
    
    let mut rx = engine.send_message(chat_request).await.map_err(|e| e.to_string())?;
    
    let app_clone = app.clone();
    let conv_id_clone = conv_id.clone();
    
    tokio::spawn(async move {
        while let Some(event) = rx.recv().await {
            let event_name = match &event {
                EngineEvent::Text(_) => "engine:text",
                EngineEvent::Thinking(_) => "engine:thinking",
                EngineEvent::ToolUseStart { .. } => "engine:tool_use_start",
                EngineEvent::ToolArgDelta { .. } => "engine:tool_arg_delta",
                EngineEvent::ToolUseDone { .. } => "engine:tool_use_done",
                EngineEvent::MessageStart { .. } => "engine:message_start",
                EngineEvent::MessageDelta { .. } => "engine:message_delta",
                EngineEvent::MessageStop { .. } => "engine:message_stop",
                EngineEvent::Error(_) => "engine:error",
                EngineEvent::Usage(_) => "engine:usage",
                EngineEvent::AskUser { .. } => "engine:ask_user",
            };
            
            let payload = serde_json::json!({
                "conversation_id": conv_id_clone,
                "event": match event {
                    EngineEvent::Text(text) => serde_json::json!({ "type": "text", "text": text }),
                    EngineEvent::Thinking(text) => serde_json::json!({ "type": "thinking", "text": text }),
                    EngineEvent::ToolUseStart { tool_use_id, tool_name, tool_input, text_before } => {
                        serde_json::json!({ "type": "tool_use_start", "tool_use_id": tool_use_id, "tool_name": tool_name, "tool_input": tool_input, "textBefore": text_before })
                    }
                    EngineEvent::ToolArgDelta { tool_use_id, delta } => {
                        serde_json::json!({ "type": "tool_arg_delta", "tool_use_id": tool_use_id, "delta": delta })
                    }
                    EngineEvent::ToolUseDone { tool_use_id, tool_name, tool_input, output, is_error } => {
                        serde_json::json!({ "type": "tool_use_done", "tool_use_id": tool_use_id, "tool_name": tool_name, "tool_input": tool_input, "output": output, "is_error": is_error })
                    }
                    EngineEvent::MessageStart { model } => serde_json::json!({ "type": "message_start", "model": model }),
                    EngineEvent::MessageDelta { stop_reason } => serde_json::json!({ "type": "message_delta", "stop_reason": stop_reason }),
                    EngineEvent::MessageStop { full_text, stop_reason } => serde_json::json!({ "type": "message_stop", "full_text": full_text, "stop_reason": stop_reason }),
                    EngineEvent::Error(error) => serde_json::json!({ "type": "error", "error": error }),
                    EngineEvent::Usage(usage) => serde_json::json!({ "type": "usage", "usage": usage }),
                    EngineEvent::AskUser { question, options } => serde_json::json!({ "type": "ask_user", "question": question, "options": options }),
                }
            });
            
            let _ = app_clone.emit(event_name, payload);
        }
    });
    
    Ok(NativeChatResponse {
        conversation_id: conv_id,
        status: "streaming_started".to_string(),
    })
}

#[derive(Deserialize)]
pub struct CreateConversationRequest {
    pub model: String,
    pub title: Option<String>,
    pub research_mode: Option<bool>,
}

#[derive(Serialize)]
pub struct ConversationInfo {
    pub id: String,
    pub title: Option<String>,
    pub model: String,
    pub workspace_path: String,
    pub created_at: String,
    pub updated_at: String,
}

#[tauri::command]
pub async fn native_create_conversation(
    _app: AppHandle,
    request: CreateConversationRequest,
) -> Result<ConversationInfo, String> {
    let client = reqwest::Client::new();
    let resp = client.post("http://localhost:30080/api/conversations")
        .json(&serde_json::json!({}))
        .send().await.map_err(|e| e.to_string())?;
    let data: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
    let id = data.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let now = chrono::Utc::now().to_rfc3339();
    Ok(ConversationInfo {
        id: id.clone(),
        title: request.title.clone(),
        model: request.model.clone(),
        workspace_path: String::new(),
        created_at: now.clone(),
        updated_at: now,
    })
}

#[tauri::command]
pub async fn native_list_conversations(_app: AppHandle) -> Result<Vec<ConversationInfo>, String> {
    let client = reqwest::Client::new();
    let resp = client.get("http://localhost:30080/api/conversations")
        .send().await.map_err(|e| e.to_string())?;
    let data: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
    let convs = data.get("conversations")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    Ok(convs.iter().map(|c| ConversationInfo {
        id: c.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string(),
        title: c.get("title").and_then(|v| v.as_str()).map(String::from),
        model: c.get("model").and_then(|v| v.as_str()).unwrap_or("").to_string(),
        workspace_path: c.get("workspace_path").and_then(|v| v.as_str()).unwrap_or("").to_string(),
        created_at: c.get("created_at").and_then(|v| v.as_str()).unwrap_or("").to_string(),
        updated_at: c.get("updated_at").and_then(|v| v.as_str()).unwrap_or("").to_string(),
    }).collect())
}

#[tauri::command]
pub async fn native_delete_conversation(
    app: AppHandle,
    conversation_id: String,
) -> Result<(), String> {
    let client = reqwest::Client::new();
    client.delete(&format!("http://localhost:30080/api/conversations/{}", conversation_id))
        .send().await.map_err(|e| e.to_string())?;
    
    if let Some(engine_state) = app.state::<Arc<Mutex<Option<NativeEngine>>>>().lock().await.as_ref() {
        engine_state.cancel_turn(&conversation_id).await;
    }
    
    Ok(())
}

#[derive(Serialize)]
pub struct MessageInfo {
    pub id: String,
    pub conversation_id: String,
    pub role: String,
    pub content: String,
    pub created_at: String,
    pub tool_calls: Option<Vec<ToolCallInfo>>,
}

#[derive(Serialize)]
pub struct ToolCallInfo {
    pub id: String,
    pub name: String,
    pub input: serde_json::Value,
    pub output: Option<String>,
    pub is_error: Option<bool>,
}

#[tauri::command]
pub async fn native_get_messages(
    _app: AppHandle,
    conversation_id: String,
) -> Result<Vec<MessageInfo>, String> {
    let client = reqwest::Client::new();
    let resp = client.get(&format!("http://localhost:30080/api/conversations/{}/messages", conversation_id))
        .send().await.map_err(|e| e.to_string())?;
    let data: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
    let msgs = data.get("messages")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    Ok(msgs.iter().map(|m| MessageInfo {
        id: m.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string(),
        conversation_id: conversation_id.clone(),
        role: m.get("role").and_then(|v| v.as_str()).unwrap_or("").to_string(),
        content: m.get("content").and_then(|v| v.as_str()).unwrap_or("").to_string(),
        created_at: m.get("created_at").and_then(|v| v.as_str()).unwrap_or("").to_string(),
        tool_calls: None,
    }).collect())
}

#[derive(Serialize)]
pub struct ProviderInfo {
    pub id: String,
    pub name: String,
    pub base_url: String,
    pub api_format: String,
    pub enabled: bool,
    pub models: Vec<ModelInfo>,
}

#[derive(Serialize)]
pub struct ModelInfo {
    pub id: String,
    pub name: String,
    pub enabled: bool,
}

#[tauri::command]
pub async fn native_list_providers(app: AppHandle) -> Result<Vec<ProviderInfo>, String> {
    let state = app.state::<Arc<Mutex<Option<NativeEngine>>>>();
    let engine_guard = state.lock().await;
    let _engine = engine_guard.as_ref().ok_or("Native engine not initialized")?;
    
    let pm_state = app.state::<Arc<Mutex<Option<Arc<Mutex<ProviderManager>>>>>>();
    let pm_guard = pm_state.lock().await;
    let pm = pm_guard.as_ref().ok_or("Provider manager not initialized")?;
    
    let pm = pm.lock().await;
    let providers = pm.list_providers();
    
    Ok(providers.iter().map(|p| ProviderInfo {
        id: p.id.clone(),
        name: p.name.clone(),
        base_url: p.base_url.clone(),
        api_format: match p.api_format {
            crate::native_engine::provider_manager::ApiFormat::Anthropic => "anthropic".to_string(),
            crate::native_engine::provider_manager::ApiFormat::OpenAI => "openai".to_string(),
        },
        enabled: p.enabled,
        models: p.models.iter().map(|m| ModelInfo {
            id: m.id.clone(),
            name: m.name.clone(),
            enabled: m.enabled,
        }).collect(),
    }).collect())
}

#[derive(Deserialize)]
pub struct UpdateProviderRequest {
    pub id: String,
    pub name: String,
    pub base_url: String,
    pub api_key: String,
    pub api_format: String,
    pub models: Vec<ModelConfigInput>,
    pub enabled: bool,
}

#[derive(Deserialize, Clone)]
pub struct ModelConfigInput {
    pub id: String,
    pub name: String,
    pub enabled: bool,
}

#[tauri::command]
pub async fn native_update_provider(
    app: AppHandle,
    request: UpdateProviderRequest,
) -> Result<ProviderInfo, String> {
    let pm_state = app.state::<Arc<Mutex<Option<Arc<Mutex<ProviderManager>>>>>>();
    let pm_guard = pm_state.lock().await;
    let pm = pm_guard.as_ref().ok_or("Provider manager not initialized")?;

    let mut pm = pm.lock().await;

    let id = if request.id.is_empty() {
        uuid::Uuid::new_v4().to_string()
    } else {
        request.id.clone()
    };

    let api_format = match request.api_format.as_str() {
        "anthropic" => crate::native_engine::provider_manager::ApiFormat::Anthropic,
        "openai" => crate::native_engine::provider_manager::ApiFormat::OpenAI,
        _ => return Err("Invalid API format".to_string()),
    };

    let api_format_str = request.api_format.clone();

    let name = request.name.clone();
    let base_url = request.base_url.clone();
    let models = request.models.clone();
    let enabled = request.enabled;

    let provider = crate::native_engine::provider_manager::Provider {
        id: id.clone(),
        name: request.name,
        base_url: request.base_url,
        api_key: request.api_key,
        api_format: api_format.clone(),
        models: request.models.into_iter().map(|m| {
            crate::native_engine::provider_manager::ModelConfig {
                id: m.id,
                name: m.name,
                enabled: m.enabled,
                max_tokens: None,
                supports_vision: false,
                supports_web_search: false,
            }
        }).collect(),
        enabled: request.enabled,
        web_search_strategy: None,
    };

    pm.update_provider(&id, provider);

    Ok(ProviderInfo {
        id,
        name,
        base_url,
        api_format: api_format_str,
        enabled,
        models: models.into_iter().map(|m| ModelInfo {
            id: m.id,
            name: m.name,
            enabled: m.enabled,
        }).collect(),
    })
}

#[tauri::command]
pub async fn native_delete_provider(
    app: AppHandle,
    id: String,
) -> Result<(), String> {
    let pm_state = app.state::<Arc<Mutex<Option<Arc<Mutex<ProviderManager>>>>>>();
    let pm_guard = pm_state.lock().await;
    let pm = pm_guard.as_ref().ok_or("Provider manager not initialized")?;

    let mut pm = pm.lock().await;
    pm.delete_provider(&id);
    
    Ok(())
}

#[derive(Serialize)]
pub struct McpServerStatusInfo {
    pub id: String,
    pub name: String,
    pub command: String,
    pub args: Vec<String>,
    pub env: Option<HashMap<String, String>>,
    pub enabled: bool,
    pub running: bool,
    pub pid: Option<u32>,
    pub tools_count: usize,
    pub resources_count: usize,
    pub error: Option<String>,
    pub transport_type: String,
}

#[derive(Deserialize)]
pub struct McpServerConfigInput {
    pub name: String,
    pub command: String,
    pub args: Vec<String>,
    pub env: Option<HashMap<String, String>>,
    pub enabled: bool,
    pub transport_type: Option<String>,
}

#[tauri::command]
pub async fn mcp_list_servers(app: AppHandle) -> Result<Vec<McpServerStatusInfo>, String> {
    let state = app.state::<Arc<Mutex<Option<Arc<Mutex<crate::mcp::McpServerManager>>>>>>();
    let guard = state.lock().await;
    let manager = guard.as_ref().ok_or("MCP manager not initialized")?;
    
    let manager = manager.lock().await;
    let servers = manager.list_servers().await;
    
    Ok(servers.into_iter().map(|s| McpServerStatusInfo {
        id: s.id,
        name: s.name,
        command: s.command,
        args: s.args,
        env: s.env,
        enabled: s.enabled,
        running: s.running,
        pid: s.pid,
        tools_count: s.tools_count,
        resources_count: s.resources_count,
        error: s.error,
        transport_type: s.transport_type,
    }).collect())
}

#[tauri::command]
pub async fn mcp_start_server(app: AppHandle, id: String) -> Result<(), String> {
    let state = app.state::<Arc<Mutex<Option<Arc<Mutex<crate::mcp::McpServerManager>>>>>>();
    let guard = state.lock().await;
    let manager = guard.as_ref().ok_or("MCP manager not initialized")?;
    
    let manager = manager.lock().await;
    manager.start_server(&id).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn mcp_stop_server(app: AppHandle, id: String) -> Result<(), String> {
    let state = app.state::<Arc<Mutex<Option<Arc<Mutex<crate::mcp::McpServerManager>>>>>>();
    let guard = state.lock().await;
    let manager = guard.as_ref().ok_or("MCP manager not initialized")?;
    
    let manager = manager.lock().await;
    manager.stop_server(&id).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn mcp_restart_server(app: AppHandle, id: String) -> Result<(), String> {
    let state = app.state::<Arc<Mutex<Option<Arc<Mutex<crate::mcp::McpServerManager>>>>>>();
    let guard = state.lock().await;
    let manager = guard.as_ref().ok_or("MCP manager not initialized")?;
    
    let manager = manager.lock().await;
    manager.restart_server(&id).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn mcp_add_server(app: AppHandle, config: McpServerConfigInput) -> Result<(), String> {
    let state = app.state::<Arc<Mutex<Option<Arc<Mutex<crate::mcp::McpServerManager>>>>>>();
    let guard = state.lock().await;
    let manager = guard.as_ref().ok_or("MCP manager not initialized")?;
    
    let server_config = crate::mcp::McpServerConfig {
        id: config.name.clone().to_lowercase().replace(' ', "-"),
        name: config.name,
        command: config.command,
        args: config.args,
        env: config.env.unwrap_or_default(),
        enabled: config.enabled,
    };
    
    let manager = manager.lock().await;
    manager.add_server(server_config).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn mcp_update_server(app: AppHandle, id: String, config: McpServerConfigInput) -> Result<(), String> {
    let state = app.state::<Arc<Mutex<Option<Arc<Mutex<crate::mcp::McpServerManager>>>>>>();
    let guard = state.lock().await;
    let manager = guard.as_ref().ok_or("MCP manager not initialized")?;
    
    let server_config = crate::mcp::McpServerConfig {
        id: id.clone(),
        name: config.name,
        command: config.command,
        args: config.args,
        env: config.env.unwrap_or_default(),
        enabled: config.enabled,
    };
    
    let manager = manager.lock().await;
    manager.update_server(&id, server_config).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn mcp_remove_server(app: AppHandle, id: String) -> Result<(), String> {
    let state = app.state::<Arc<Mutex<Option<Arc<Mutex<crate::mcp::McpServerManager>>>>>>();
    let guard = state.lock().await;
    let manager = guard.as_ref().ok_or("MCP manager not initialized")?;
    
    let manager = manager.lock().await;
    manager.remove_server(&id).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn mcp_toggle_server(app: AppHandle, id: String, enabled: bool) -> Result<(), String> {
    let state = app.state::<Arc<Mutex<Option<Arc<Mutex<crate::mcp::McpServerManager>>>>>>();
    let guard = state.lock().await;
    let manager = guard.as_ref().ok_or("MCP manager not initialized")?;
    
    let manager = manager.lock().await;
    manager.set_server_enabled(&id, enabled).await.map_err(|e| e.to_string())
}

#[derive(Serialize)]
pub struct McpToolInfo {
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
    pub server_name: String,
}

#[tauri::command]
pub async fn mcp_list_tools(app: AppHandle) -> Result<Vec<McpToolInfo>, String> {
    let state = app.state::<Arc<Mutex<Option<Arc<Mutex<crate::mcp::McpServerManager>>>>>>();
    let guard = state.lock().await;
    let manager = guard.as_ref().ok_or("MCP manager not initialized")?;
    
    let manager = manager.lock().await;
    let tools = manager.get_all_tools().await;
    
    Ok(tools.into_iter().map(|t| McpToolInfo {
        name: t.name,
        description: t.description,
        input_schema: t.input_schema,
        server_name: t.server_name,
    }).collect())
}
