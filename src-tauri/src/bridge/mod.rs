use anyhow::Result;
use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::sse::{Event, KeepAlive, Sse},
    routing::{delete, get, post, patch},
    Json, Router,
};
use futures::stream::{Stream, StreamExt};
use serde::{Deserialize, Serialize};
use std::convert::Infallible;
use std::sync::Arc;
use tokio::sync::Mutex;
use tower_http::cors::{Any, CorsLayer};

use crate::engine::EnginePool;
use crate::mcp::McpServerManager;
use crate::skills::SkillsManager;
use crate::tools::{execute_tool, get_tool_definitions, ToolDefinition};

type AppState = (Arc<Mutex<EnginePool>>, Arc<Mutex<SkillsManager>>, Arc<Mutex<McpServerManager>>);

#[derive(Clone)]
pub struct BridgeServer;

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
    pub attachments: Option<Vec<serde_json::Value>>,
    pub provider_format: Option<String>,
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
    pub fn new() -> Self {
        Self
    }

    pub async fn start(&self, port: u16) -> Result<()> {
        let engine_pool = Arc::new(Mutex::new(EnginePool::new()));
        let skills_manager = Arc::new(Mutex::new(SkillsManager::new()));
        let mcp_manager = Arc::new(Mutex::new(McpServerManager::new()));

        let cors = CorsLayer::new()
            .allow_origin(Any)
            .allow_methods(Any)
            .allow_headers(Any);

        let app = Router::new()
            .route("/api/system-status", get(system_status))
            .route("/api/chat", post(chat_handler))
            .route("/api/chat/stream", get(chat_stream_handler))
            .route("/api/tools", post(tools_handler))
            .route("/api/tools/list", get(tools_list_handler))
            .route("/api/conversations", get(conversations_list))
            .route("/api/conversations", post(conversations_create))
            .route("/api/conversations/{id}", get(conversation_get))
            .route("/api/conversations/{id}", post(conversation_update))
            .route("/api/conversations/{id}", delete(conversation_delete))
            .route("/api/projects", get(projects_list))
            .route("/api/projects", post(projects_create))
            .route("/api/upload", post(upload_handler))
            // Skills API
            .route("/api/skills", get(skills_list))
            .route("/api/skills", post(skills_create))
            .route("/api/skills/{id}", get(skills_get))
            .route("/api/skills/{id}", patch(skills_update))
            .route("/api/skills/{id}", delete(skills_delete))
            .route("/api/skills/{id}/toggle", patch(skills_toggle))
            .route("/api/skills/{id}/file", get(skills_file))
            // Skills Marketplace API
            .route("/api/marketplace/skills", get(marketplace_skills_list))
            .route("/api/marketplace/skills/{id}", get(marketplace_skill_detail))
            .route("/api/marketplace/import", post(marketplace_import_skill))
            // Artifacts API
            .route("/api/artifacts", get(artifacts_list))
            .route("/api/artifacts/content", get(artifacts_content))
            // GitHub Connector API
            .route("/api/github/status", get(github_status))
            .route("/api/github/auth-url", get(github_auth_url))
            .route("/api/github/disconnect", post(github_disconnect))
            .route("/api/github/repos", get(github_repos))
            .route("/api/github/repos/{owner}/{repo}/tree", get(github_tree))
            .route("/api/github/repos/{owner}/{repo}/contents", get(github_contents))
            // Providers API
            .route("/api/providers", get(providers_list))
            .route("/api/providers", post(providers_create))
            .route("/api/providers/{id}", get(providers_get))
            .route("/api/providers/{id}", patch(providers_update))
            .route("/api/providers/{id}", delete(providers_delete))
            .route("/api/providers/{id}/test-websearch", post(providers_test_websearch))
            .route("/api/providers/models", get(providers_models))
            // MCP API
            .route("/api/mcp/servers", get(mcp_servers_list))
            .route("/api/mcp/servers", post(mcp_servers_create))
            .route("/api/mcp/servers/{id}", get(mcp_servers_get))
            .route("/api/mcp/servers/{id}", patch(mcp_servers_update))
            .route("/api/mcp/servers/{id}", delete(mcp_servers_delete))
            .route("/api/mcp/servers/{id}/start", post(mcp_servers_start))
            .route("/api/mcp/servers/{id}/stop", post(mcp_servers_stop))
            .route("/api/mcp/servers/status", get(mcp_servers_status))
            .layer(cors)
            .with_state((engine_pool, skills_manager, mcp_manager));

        let listener = tokio::net::TcpListener::bind(format!("127.0.0.1:{}", port)).await?;
        println!("[Bridge] Server running on http://127.0.0.1:{}", port);
        axum::serve(listener, app).await?;
        Ok(())
    }
}

async fn system_status() -> Json<SystemStatus> {
    let platform = std::env::consts::OS.to_string();
    let git_bash_path = which_git_bash();

    Json(SystemStatus {
        platform,
        git_bash: GitBashStatus {
            required: cfg!(target_os = "windows"),
            found: git_bash_path.is_some(),
            path: git_bash_path,
        },
    })
}

fn which_git_bash() -> Option<String> {
    let candidates = if cfg!(target_os = "windows") {
        vec![
            r"C:\Program Files\Git\bin\bash.exe",
            r"C:\Program Files (x86)\Git\bin\bash.exe",
        ]
    } else {
        vec!["/usr/bin/bash", "/bin/bash"]
    };

    for path in &candidates {
        if std::path::Path::new(path).exists() {
            return Some(path.to_string());
        }
    }

    if let Ok(output) = std::process::Command::new("which").arg("bash").output() {
        if output.status.success() {
            return Some(String::from_utf8_lossy(&output.stdout).trim().to_string());
        }
    }

    None
}

async fn chat_handler(
    State((pool, _, _)): State<AppState>,
    Json(req): Json<ChatRequest>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let mut pool = pool.lock().await;
    match pool.send_message(&req.conversation_id, &req).await {
        Ok(response) => Ok(Json(response)),
        Err(e) => {
            eprintln!("[Chat] Error: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

async fn chat_stream_handler(
    State((_pool, _, _)): State<AppState>,
    Query(query): Query<StreamQuery>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let (tx, mut rx) = tokio::sync::mpsc::channel::<Result<Event, Infallible>>(100);

    tokio::spawn(async move {
        let _ = tx.send(Ok(Event::default()
            .event("start")
            .data(serde_json::json!({"status": "connected", "conversation_id": &query.conversation_id}).to_string()))).await;

        let messages: Vec<serde_json::Value> = query
            .messages
            .and_then(|m| serde_json::from_str(&m).ok())
            .unwrap_or_default();

        let req = ChatRequest {
            conversation_id: query.conversation_id.clone(),
            messages: Some(messages),
            message: None,
            model: query.model.clone(),
            user_mode: query.user_mode.clone(),
            env_token: query.env_token.clone(),
            env_base_url: query.env_base_url.clone(),
            research_mode: query.research_mode,
            attachments: None,
            provider_format: None,
        };

        match stream_api_response_realtime(&req, tx.clone()).await {
            Ok(()) => {
                let _ = tx.send(Ok(Event::default().event("done").data("{}"))).await;
            }
            Err(e) => {
                let _ = tx.send(Ok(Event::default()
                    .event("error")
                    .data(serde_json::json!({"error": e.to_string()}).to_string()))).await;
            }
        }
    });

    let stream = async_stream::stream! {
        while let Some(item) = rx.recv().await {
            yield item;
        }
    };

    Sse::new(stream).keep_alive(KeepAlive::default())
}

async fn stream_api_response_realtime(
    req: &ChatRequest,
    tx: tokio::sync::mpsc::Sender<Result<Event, Infallible>>,
) -> Result<()> {
    let env_token = req.env_token.clone().unwrap_or_default();
    let env_base_url = req.env_base_url.clone().unwrap_or_default();
    let provider_format = req.provider_format.clone().unwrap_or_default();

    if env_token.is_empty() {
        anyhow::bail!("No API token provided");
    }

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(120))
        .build()?;

    // Determine API URL: if env_base_url is provided, use it directly; otherwise resolve from user_mode
    // Kimi/Moonshot/MiniMax use /v1/chat/completions, others use /chat/completions
    let api_url = if !env_base_url.is_empty() {
        let base = env_base_url.trim_end_matches('/');
        let path = if base.contains("kimi") || base.contains("moonshot") || base.contains("minimax") {
            "/v1/chat/completions"
        } else {
            "/chat/completions"
        };
        format!("{}{}", base, path)
    } else {
        resolve_api_url(&req.user_mode, &env_base_url)
    };

    // Build request body based on provider format
    let body = if provider_format == "anthropic" || api_url.contains("anthropic") {
        serde_json::json!({
            "model": req.model,
            "messages": req.get_messages(),
            "max_tokens": 8192,
            "stream": true,
        })
    } else {
        // OpenAI-compatible format (default)
        serde_json::json!({
            "model": req.model,
            "messages": req.get_messages(),
            "max_tokens": 8192,
            "stream": true,
        })
    };

    let mut request_builder = client
        .post(&api_url)
        .header("Content-Type", "application/json")
        .json(&body);

    // Set auth header based on provider format or URL
    if provider_format == "anthropic" || api_url.contains("anthropic") {
        request_builder = request_builder
            .header("x-api-key", &env_token)
            .header("anthropic-version", "2023-06-01");
    } else {
        request_builder = request_builder.bearer_auth(&env_token);
    }

    let response = request_builder.send().await?;

    if !response.status().is_success() {
        let status = response.status();
        let text = response.text().await.unwrap_or_default();
        anyhow::bail!("API error {}: {}", status, text);
    }

    let mut stream = response.bytes_stream();
    let mut buffer = String::new();

    while let Some(chunk) = stream.next().await {
        match chunk {
            Ok(bytes) => {
                buffer.push_str(&String::from_utf8_lossy(&bytes));
                while let Some(pos) = buffer.find('\n') {
                    let line = buffer[..pos].to_string();
                    buffer = buffer[pos + 1..].to_string();

                    if line.starts_with("data: ") {
                        let data = &line[6..];
                        if data == "[DONE]" {
                            return Ok(());
                        }
                        let event = Event::default().event("message").data(data.to_string());
                        if tx.send(Ok(event)).await.is_err() {
                            return Ok(());
                        }
                    }
                }
            }
            Err(e) => {
                anyhow::bail!("Stream error: {}", e);
            }
        }
    }

    Ok(())
}

async fn tools_handler(
    Json(req): Json<ToolRequest>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let cwd = req.cwd.unwrap_or_else(|| ".".to_string());
    match execute_tool(&req.name, req.input, &cwd) {
        Ok(result) => Ok(Json(result)),
        Err(e) => {
            eprintln!("[Tools] Error: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

async fn tools_list_handler() -> Json<Vec<ToolDefinition>> {
    Json(get_tool_definitions())
}

async fn conversations_list() -> Json<serde_json::Value> {
    Json(serde_json::json!({ "conversations": [] }))
}

async fn conversations_create() -> Json<serde_json::Value> {
    Json(serde_json::json!({ "id": uuid::Uuid::new_v4().to_string() }))
}

async fn conversation_get() -> Json<serde_json::Value> {
    Json(serde_json::json!({}))
}

async fn conversation_update() -> Json<serde_json::Value> {
    Json(serde_json::json!({ "ok": true }))
}

async fn conversation_delete() -> Json<serde_json::Value> {
    Json(serde_json::json!({ "ok": true }))
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

// ═════════════════════════════════════════════════════════════════
// Skills API handlers
// ═════════════════════════════════════════════════════════════════

use crate::skills::Skill;
use std::collections::HashMap;

async fn skills_list(State((_, skills_manager, _)): State<AppState>) -> Json<serde_json::Value> {
    let manager = skills_manager.lock().await;
    match manager.load_skills().await {
        Ok(skills) => {
            let bundled: Vec<Skill> = skills.iter().filter(|s| s.source == crate::skills::SkillSource::Bundled).cloned().collect();
            let user: Vec<Skill> = skills.iter().filter(|s| s.source != crate::skills::SkillSource::Bundled).cloned().collect();
            Json(serde_json::json!({
                "examples": bundled,
                "my_skills": user
            }))
        }
        Err(_) => Json(serde_json::json!({
            "examples": [],
            "my_skills": []
        })),
    }
}

#[derive(Deserialize)]
struct CreateSkillRequest {
    name: String,
    description: String,
    content: Option<String>,
}

async fn skills_create(
    State((_, skills_manager, _)): State<AppState>,
    Json(req): Json<CreateSkillRequest>,
) -> Json<serde_json::Value> {
    let manager = skills_manager.lock().await;
    match manager.create_skill(&req.name, &req.description, req.content.as_deref().unwrap_or("")) {
        Ok(skill) => Json(serde_json::json!({
            "id": skill.id,
            "name": skill.name,
            "description": skill.description,
            "enabled": skill.enabled,
            "source": skill.source,
            "created_at": skill.created_at
        })),
        Err(e) => Json(serde_json::json!({
            "error": e.to_string()
        })),
    }
}

async fn skills_get(
    State((_, skills_manager, _)): State<AppState>,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let manager = skills_manager.lock().await;
    match manager.get_skill_by_id(&id).await {
        Ok(Some(skill)) => Ok(Json(serde_json::json!({
            "id": skill.id,
            "name": skill.name,
            "description": skill.description,
            "content": skill.content,
            "when_to_use": skill.when_to_use,
            "enabled": skill.enabled,
            "source": skill.source,
            "loaded_from": skill.loaded_from,
            "source_dir": skill.source_dir,
            "is_example": skill.is_example,
            "files": skill.files,
            "created_at": skill.created_at
        }))),
        Ok(None) => Err(StatusCode::NOT_FOUND),
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

async fn skills_update(
    State((_, skills_manager, _)): State<AppState>,
    axum::extract::Path(id): axum::extract::Path<String>,
    Json(req): Json<HashMap<String, serde_json::Value>>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let manager = skills_manager.lock().await;
    match manager.update_skill(&id, req) {
        Ok(_) => {
            match manager.get_skill_by_id(&id).await {
                Ok(Some(skill)) => Ok(Json(serde_json::json!({
                    "id": skill.id,
                    "name": skill.name,
                    "description": skill.description,
                    "enabled": skill.enabled
                }))),
                _ => Err(StatusCode::NOT_FOUND),
            }
        }
        Err(_) => Err(StatusCode::NOT_FOUND),
    }
}

async fn skills_delete(
    State((_, skills_manager, _)): State<AppState>,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Json<serde_json::Value> {
    let manager = skills_manager.lock().await;
    match manager.delete_skill(&id) {
        Ok(_) => Json(serde_json::json!({ "ok": true })),
        Err(_) => Json(serde_json::json!({ "ok": false })),
    }
}

async fn skills_toggle(
    State((_, _, _)): State<AppState>,
    axum::extract::Path(_id): axum::extract::Path<String>,
    Json(req): Json<HashMap<String, serde_json::Value>>,
) -> Json<serde_json::Value> {
    let enabled = req.get("enabled").and_then(|v| v.as_bool()).unwrap_or(true);
    Json(serde_json::json!({
        "enabled": enabled
    }))
}

async fn skills_file(
    State((_, skills_manager, _)): State<AppState>,
    axum::extract::Path(id): axum::extract::Path<String>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let manager = skills_manager.lock().await;
    let path = params.get("path").cloned().unwrap_or_default();
    match manager.get_skill_content(&id, &path) {
        Ok(content) => Ok(Json(serde_json::json!({ "content": content }))),
        Err(_) => Err(StatusCode::NOT_FOUND),
    }
}

// ═════════════════════════════════════════════════════════════════
// Skills Marketplace API handlers
// ═════════════════════════════════════════════════════════════════

async fn marketplace_skills_list(Query(params): Query<HashMap<String, String>>) -> Json<serde_json::Value> {
    let query = params.get("q").cloned().unwrap_or_default();
    let category = params.get("category").cloned().unwrap_or_default();

    let skills = fetch_marketplace_skills(&query, &category).await.unwrap_or_default();
    Json(serde_json::json!({
        "skills": skills,
        "total": skills.len(),
        "query": query
    }))
}

async fn marketplace_skill_detail(
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    match fetch_marketplace_skill_detail(&id).await {
        Ok(skill) => Ok(Json(skill)),
        Err(_) => Err(StatusCode::NOT_FOUND),
    }
}

#[derive(Deserialize)]
struct ImportRequest {
    id: String,
    name: String,
    #[allow(dead_code)]
    repo_url: Option<String>,
}

async fn marketplace_import_skill(
    State((_, skills_manager, _)): State<AppState>,
    Json(req): Json<ImportRequest>,
) -> Json<serde_json::Value> {
    let manager = skills_manager.lock().await;

    let skill_content = fetch_skill_from_github(&req.id).await;

    let description = format!("Imported from marketplace: {}", req.id);
    let content = skill_content.unwrap_or_default();

    match manager.create_skill(&req.name, &description, &content) {
        Ok(s) => Json(serde_json::json!({
            "id": s.id,
            "name": s.name,
            "description": s.description,
            "imported": true
        })),
        Err(_) => Json(serde_json::json!({
            "error": "Failed to import skill"
        })),
    }
}

async fn fetch_skill_from_github(skill_id: &str) -> Option<String> {
    let client = reqwest::Client::new();
    let url = format!("https://raw.githubusercontent.com/anthropics/skills/main/{}/SKILL.md", skill_id);

    let response = client
        .get(&url)
        .header("User-Agent", "Claude-Desktop-Tauri")
        .send()
        .await
        .ok()?;

    if !response.status().is_success() {
        return None;
    }

    response.text().await.ok()
}

async fn fetch_marketplace_skills(query: &str, _category: &str) -> Result<Vec<serde_json::Value>, reqwest::Error> {
    let client = reqwest::Client::new();

    let github_url = if query.is_empty() {
        "https://api.github.com/repos/anthropics/skills/contents".to_string()
    } else {
        format!("https://api.github.com/search/code?q={}+repo:anthropics/skills", query)
    };

    let response = client
        .get(&github_url)
        .header("User-Agent", "Claude-Desktop-Tauri")
        .header("Accept", "application/vnd.github.v3+json")
        .send()
        .await?;

    if response.status() == 200 {
        if query.is_empty() {
            let contents: Vec<serde_json::Value> = response.json().await.unwrap_or_default();
            let skills: Vec<serde_json::Value> = contents.into_iter()
                .filter(|c| c["type"].as_str() == Some("dir"))
                .map(|c| serde_json::json!({
                    "id": c["name"],
                    "name": c["name"],
                    "description": format!("Skill from anthropics/skills: {}", c["name"].as_str().unwrap_or("")),
                    "source": "marketplace",
                    "url": c["url"]
                }))
                .collect();
            Ok(skills)
        } else {
            let search: serde_json::Value = response.json().await.unwrap_or_default();
            let items = search["items"].as_array().cloned().unwrap_or_default();
            let skills: Vec<serde_json::Value> = items.into_iter()
                .take(20)
                .map(|item| serde_json::json!({
                    "id": item["name"],
                    "name": item["name"],
                    "description": format!("Search result: {}", item["name"].as_str().unwrap_or("")),
                    "source": "marketplace"
                }))
                .collect();
            Ok(skills)
        }
    } else {
        Ok(vec![])
    }
}

async fn fetch_marketplace_skill_detail(id: &str) -> Result<serde_json::Value, anyhow::Error> {
    let client = reqwest::Client::new();
    let url = format!("https://raw.githubusercontent.com/anthropics/skills/main/{}/SKILL.md", id);

    let response = client
        .get(&url)
        .header("User-Agent", "Claude-Desktop-Tauri")
        .send()
        .await?;

    if response.status() == 200 {
        let content = response.text().await.unwrap_or_default();
        Ok(serde_json::json!({
            "id": id,
            "name": id,
            "description": format!("Skill: {}", id),
            "content": content,
            "source": "marketplace",
            "readme_url": format!("https://github.com/anthropics/skills/tree/main/{}", id)
        }))
    } else {
        Err(anyhow::anyhow!("Skill not found: {}", id).into())
    }
}

// ═════════════════════════════════════════════════════════════════
// Artifacts API handlers
// ═════════════════════════════════════════════════════════════════

async fn artifacts_list() -> Json<serde_json::Value> {
    Json(serde_json::json!({ "artifacts": [] }))
}

async fn artifacts_content(Query(params): Query<HashMap<String, String>>) -> Json<serde_json::Value> {
    let path = params.get("path").cloned().unwrap_or_default();
    Json(serde_json::json!({
        "content": format!("<!-- Artifact content from {} -->", path)
    }))
}

// ═════════════════════════════════════════════════════════════════
// GitHub Connector API handlers
// ═════════════════════════════════════════════════════════════════

async fn github_status() -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "connected": false,
        "user": null
    }))
}

async fn github_auth_url() -> Json<serde_json::Value> {
    // Return a placeholder URL - user needs to set up GitHub OAuth app
    Json(serde_json::json!({
        "url": "https://github.com/login/oauth/authorize?client_id=YOUR_CLIENT_ID&scope=repo"
    }))
}

async fn github_disconnect() -> Json<serde_json::Value> {
    Json(serde_json::json!({ "ok": true }))
}

async fn github_repos() -> Json<serde_json::Value> {
    Json(serde_json::json!({ "repos": [] }))
}

async fn github_tree(
    axum::extract::Path((owner, repo)): axum::extract::Path<(String, String)>,
    Query(params): Query<HashMap<String, String>>,
) -> Json<serde_json::Value> {
    let _ = (owner, repo, params);
    Json(serde_json::json!({ "tree": [] }))
}

async fn github_contents(
    axum::extract::Path((owner, repo)): axum::extract::Path<(String, String)>,
    Query(params): Query<HashMap<String, String>>,
) -> Json<serde_json::Value> {
    let _ = (owner, repo, params);
    Json(serde_json::json!({ "content": "" }))
}

// ═════════════════════════════════════════════════════════════════
// Providers API handlers
// ═════════════════════════════════════════════════════════════════

static PROVIDERS_STORE: once_cell::sync::Lazy<tokio::sync::Mutex<HashMap<String, serde_json::Value>>> =
    once_cell::sync::Lazy::new(|| tokio::sync::Mutex::new(HashMap::new()));

async fn providers_list() -> Json<serde_json::Value> {
    let store = PROVIDERS_STORE.lock().await;
    let providers: Vec<serde_json::Value> = store.values().cloned().collect();
    Json(serde_json::json!(providers))
}

async fn providers_create(Json(req): Json<serde_json::Value>) -> Json<serde_json::Value> {
    let id = uuid::Uuid::new_v4().to_string();
    let mut provider = req.clone();
    if let Some(obj) = provider.as_object_mut() {
        obj.insert("id".to_string(), serde_json::json!(id));
        if obj.get("enabled").is_none() {
            obj.insert("enabled".to_string(), serde_json::json!(true));
        }
        if obj.get("models").is_none() {
            obj.insert("models".to_string(), serde_json::json!([]));
        }
    }
    let mut store = PROVIDERS_STORE.lock().await;
    store.insert(id.clone(), provider.clone());
    Json(provider)
}

async fn providers_get(axum::extract::Path(id): axum::extract::Path<String>) -> Result<Json<serde_json::Value>, StatusCode> {
    let store = PROVIDERS_STORE.lock().await;
    match store.get(&id) {
        Some(p) => Ok(Json(p.clone())),
        None => Err(StatusCode::NOT_FOUND),
    }
}

async fn providers_update(
    axum::extract::Path(id): axum::extract::Path<String>,
    Json(req): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let mut store = PROVIDERS_STORE.lock().await;
    match store.get_mut(&id) {
        Some(provider) => {
            if let Some(obj) = provider.as_object_mut() {
                if let Some(name) = req.get("name") {
                    obj.insert("name".to_string(), name.clone());
                }
                if let Some(base_url) = req.get("baseUrl") {
                    obj.insert("baseUrl".to_string(), base_url.clone());
                }
                if let Some(api_key) = req.get("apiKey") {
                    obj.insert("apiKey".to_string(), api_key.clone());
                }
                if let Some(format) = req.get("format") {
                    obj.insert("format".to_string(), format.clone());
                }
                if let Some(models) = req.get("models") {
                    obj.insert("models".to_string(), models.clone());
                }
                if let Some(enabled) = req.get("enabled") {
                    obj.insert("enabled".to_string(), enabled.clone());
                }
                if let Some(supports_web_search) = req.get("supportsWebSearch") {
                    obj.insert("supportsWebSearch".to_string(), supports_web_search.clone());
                }
                if let Some(web_search_strategy) = req.get("webSearchStrategy") {
                    obj.insert("webSearchStrategy".to_string(), web_search_strategy.clone());
                }
            }
            Ok(Json(provider.clone()))
        }
        None => Err(StatusCode::NOT_FOUND),
    }
}

async fn providers_delete(axum::extract::Path(id): axum::extract::Path<String>) -> Json<serde_json::Value> {
    let mut store = PROVIDERS_STORE.lock().await;
    store.remove(&id);
    Json(serde_json::json!({ "ok": true }))
}

async fn providers_test_websearch(axum::extract::Path(id): axum::extract::Path<String>) -> Json<serde_json::Value> {
    let store = PROVIDERS_STORE.lock().await;
    let provider = match store.get(&id) {
        Some(p) => p.clone(),
        None => {
            return Json(serde_json::json!({
                "ok": false,
                "reason": "Provider not found"
            }));
        }
    };
    drop(store);

    // Extract provider config
    let base_url = provider.get("baseUrl").and_then(|v| v.as_str()).unwrap_or("");
    let api_key = provider.get("apiKey").and_then(|v| v.as_str()).unwrap_or("");
    let format = provider.get("format").and_then(|v| v.as_str()).unwrap_or("openai");
    let models = provider.get("models").and_then(|v| v.as_array()).unwrap_or(&vec![]).clone();

    if base_url.is_empty() || api_key.is_empty() {
        return Json(serde_json::json!({
            "ok": false,
            "reason": "Missing base URL or API key"
        }));
    }

    // Get first enabled model for testing, or use default if none configured
    let test_model = models.iter()
        .filter_map(|m| m.as_object())
        .find(|m| m.get("enabled").and_then(|v| v.as_bool()).unwrap_or(true))
        .and_then(|m| m.get("id").and_then(|v| v.as_str().map(|s| s.to_string())));

    // Default model IDs for common providers (used when no model is configured)
    let default_model = if base_url.contains("kimi") || base_url.contains("moonshot") {
        "moonshot-v1-8k"
    } else if base_url.contains("deepseek") {
        "deepseek-chat"
    } else if base_url.contains("minimax") {
        "MiniMax-M1"
    } else {
        "gpt-4o-mini"
    };

    let model_id = test_model.unwrap_or_else(|| default_model.to_string());

    // Test the connection by making a simple API call
    let client = match reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build() {
        Ok(c) => c,
        Err(e) => {
            return Json(serde_json::json!({
                "ok": false,
                "reason": format!("Failed to create HTTP client: {}", e)
            }));
        }
    };

    // Build test request based on provider format
    let test_body = if format == "anthropic" {
        serde_json::json!({
            "model": model_id,
            "messages": [{"role": "user", "content": "Hi"}],
            "max_tokens": 10
        })
    } else {
        serde_json::json!({
            "model": model_id,
            "messages": [{"role": "user", "content": "Hi"}],
            "max_tokens": 10
        })
    };

    // Build API URL based on provider pattern
    // Kimi/Moonshot APIs use /v1/chat/completions, others use /chat/completions
    let api_path = if base_url.contains("kimi") || base_url.contains("moonshot") || base_url.contains("minimax") {
        "/v1/chat/completions"
    } else {
        "/chat/completions"
    };

    let mut request_builder = client
        .post(format!("{}{}", base_url.trim_end_matches('/'), api_path))
        .header("Content-Type", "application/json")
        .json(&test_body);

    if format == "anthropic" {
        request_builder = request_builder
            .header("x-api-key", api_key)
            .header("anthropic-version", "2023-06-01");
    } else {
        request_builder = request_builder.bearer_auth(api_key);
    }

    match request_builder.send().await {
        Ok(response) => {
            let status = response.status();
            if status.is_success() {
                Json(serde_json::json!({
                    "ok": true,
                    "reason": "Connection successful"
                }))
            } else {
                let text = response.text().await.unwrap_or_default();
                let error_msg = if text.len() > 200 {
                    format!("{}...", &text[..200])
                } else {
                    text
                };
                Json(serde_json::json!({
                    "ok": false,
                    "reason": format!("API returned {}: {}", status, error_msg)
                }))
            }
        }
        Err(e) => {
            Json(serde_json::json!({
                "ok": false,
                "reason": format!("Connection failed: {}", e)
            }))
        }
    }
}

async fn providers_models() -> Json<serde_json::Value> {
    let store = PROVIDERS_STORE.lock().await;
    let mut models = Vec::new();
    for provider in store.values() {
        if let Some(enabled) = provider.get("enabled") {
            if !enabled.as_bool().unwrap_or(true) {
                continue;
            }
        }
        let provider_id = provider.get("id").and_then(|v| v.as_str()).unwrap_or("");
        let provider_name = provider.get("name").and_then(|v| v.as_str()).unwrap_or("");
        if let Some(provider_models) = provider.get("models").and_then(|v| v.as_array()) {
            for m in provider_models {
                if let Some(model_obj) = m.as_object() {
                    if model_obj.get("enabled").and_then(|v| v.as_bool()).unwrap_or(true) {
                        models.push(serde_json::json!({
                            "id": model_obj.get("id").and_then(|v| v.as_str()).unwrap_or(""),
                            "name": model_obj.get("name").and_then(|v| v.as_str()).unwrap_or(""),
                            "providerId": provider_id,
                            "providerName": provider_name,
                        }));
                    }
                }
            }
        }
    }
    Json(serde_json::json!(models))
}

// MCP Server API handlers
// ═════════════════════════════════════════════════════════════════

async fn mcp_servers_list(State((_, _, mcp_manager)): State<AppState>) -> Json<serde_json::Value> {
    let servers = mcp_manager.lock().await.list_servers().await;
    Json(serde_json::json!(servers))
}

async fn mcp_servers_create(
    State((_, _, mcp_manager)): State<AppState>,
    Json(req): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let config = crate::mcp::McpServerConfig {
        id: uuid::Uuid::new_v4().to_string(),
        name: req.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string(),
        command: req.get("command").and_then(|v| v.as_str()).unwrap_or("").to_string(),
        args: req.get("args").and_then(|v| v.as_array())
            .map(|arr| arr.iter().filter_map(|v| v.as_str()).map(|s| s.to_string()).collect())
            .unwrap_or_default(),
        env: req.get("env").and_then(|v| v.as_object())
            .map(|obj| obj.iter().filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string()))).collect())
            .unwrap_or_default(),
        enabled: req.get("enabled").and_then(|v| v.as_bool()).unwrap_or(true),
    };

    match mcp_manager.lock().await.add_server(config).await {
        Ok(server) => Ok(Json(serde_json::json!(server))),
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

async fn mcp_servers_get(
    State((_, _, mcp_manager)): State<AppState>,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let servers = mcp_manager.lock().await.list_servers().await;
    match servers.iter().find(|s| s.id == id) {
        Some(s) => Ok(Json(serde_json::json!(s))),
        None => Err(StatusCode::NOT_FOUND),
    }
}

async fn mcp_servers_update(
    State((_, _, mcp_manager)): State<AppState>,
    axum::extract::Path(id): axum::extract::Path<String>,
    Json(req): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let servers = mcp_manager.lock().await.list_servers().await;
    let existing = match servers.iter().find(|s| s.id == id) {
        Some(s) => s.clone(),
        None => return Err(StatusCode::NOT_FOUND),
    };

    let updated = crate::mcp::McpServerConfig {
        id: id.clone(),
        name: req.get("name").and_then(|v| v.as_str()).map(|s| s.to_string()).unwrap_or(existing.name),
        command: req.get("command").and_then(|v| v.as_str()).map(|s| s.to_string()).unwrap_or(existing.command),
        args: req.get("args").and_then(|v| v.as_array())
            .map(|arr| arr.iter().filter_map(|v| v.as_str()).map(|s| s.to_string()).collect())
            .unwrap_or(existing.args),
        env: req.get("env").and_then(|v| v.as_object())
            .map(|obj| obj.iter().filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string()))).collect())
            .unwrap_or(existing.env),
        enabled: req.get("enabled").and_then(|v| v.as_bool()).unwrap_or(existing.enabled),
    };

    match mcp_manager.lock().await.update_server(&id, updated).await {
        Ok(server) => Ok(Json(serde_json::json!(server))),
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

async fn mcp_servers_delete(
    State((_, _, mcp_manager)): State<AppState>,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    match mcp_manager.lock().await.remove_server(&id).await {
        Ok(_) => Ok(Json(serde_json::json!({ "ok": true }))),
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

async fn mcp_servers_start(
    State((_, _, mcp_manager)): State<AppState>,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    match mcp_manager.lock().await.start_server(&id).await {
        Ok(status) => Ok(Json(serde_json::json!(status))),
        Err(e) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

async fn mcp_servers_stop(
    State((_, _, mcp_manager)): State<AppState>,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    match mcp_manager.lock().await.stop_server(&id).await {
        Ok(_) => Ok(Json(serde_json::json!({ "ok": true }))),
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

async fn mcp_servers_status(
    State((_, _, mcp_manager)): State<AppState>,
) -> Json<serde_json::Value> {
    let status = mcp_manager.lock().await.get_server_status().await;
    Json(serde_json::json!(status))
}

fn resolve_api_url(user_mode: &Option<String>, env_base_url: &str) -> String {
    if !env_base_url.is_empty() {
        if env_base_url.contains("/v1/messages") || env_base_url.contains("/v1/chat/completions") {
            return env_base_url.to_string();
        }
        if env_base_url.contains("anthropic") || env_base_url.contains("claude") {
            return format!("{}/v1/messages", env_base_url.trim_end_matches('/'));
        }
        return format!("{}/v1/chat/completions", env_base_url.trim_end_matches('/'));
    }

    match user_mode.as_deref() {
        Some("clawparrot") => "http://127.0.0.1:30090/api/v1/messages".to_string(),
        _ => "https://api.anthropic.com/v1/messages".to_string(),
    }
}
