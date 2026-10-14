use axum::{extract::{Path, State}, http::StatusCode, routing::{get, post}, Json, Router};
use serde::Deserialize;
use serde_json::{json, Value};
use crate::db::{project_repo as projects, conversation_repo as conversations};
use super::super::AppState;

type ApiError = (StatusCode, Json<Value>);
type ApiResult = Result<Json<Value>, ApiError>;

fn invalid(error: impl std::fmt::Display) -> ApiError {
    (StatusCode::BAD_REQUEST, Json(json!({"error": error.to_string()})))
}

async fn database<T, F>(state: &AppState, operation: F) -> Result<T, ApiError>
where T: Send + 'static, F: FnOnce(&rusqlite::Connection) -> anyhow::Result<T> + Send + 'static {
    let db = state.db_manager.clone();
    tokio::task::spawn_blocking(move || db.with_conn(operation)).await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e.to_string()}))))?
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e.to_string()}))))?
        .map_err(invalid)
}

#[derive(Deserialize)]
struct ProjectInput {
    name: Option<String>,
    description: Option<String>,
    instructions: Option<String>,
    folders: Option<Vec<String>>,
    is_archived: Option<Value>,
}

async fn list(State(state): State<AppState>) -> ApiResult {
    let result = database(&state, projects::list_projects).await?;
    Ok(Json(json!({"projects": result})))
}

async fn create(State(state): State<AppState>, Json(input): Json<ProjectInput>) -> ApiResult {
    let name = input.name.unwrap_or_default().trim().to_string();
    if name.is_empty() { return Err(invalid("Project name is required")); }
    let folders = input.folders.unwrap_or_default();
    let result = database(&state, move |conn| {
        let folders = projects::normalize_folders(&folders)?;
        let tx = conn.unchecked_transaction()?;
        let id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now().to_rfc3339();
        projects::insert_project(&tx, &id, &name, input.description.as_deref(), input.instructions.as_deref(), Some(&folders[0]), false, &now, &now)?;
        projects::set_folders(&tx, &id, &folders)?;
        let project = projects::get_project(&tx, &id)?;
        tx.commit()?;
        Ok(project)
    }).await?;
    Ok(Json(json!(result)))
}

async fn detail(Path(id): Path<String>, State(state): State<AppState>) -> ApiResult {
    let result = database(&state, move |conn| {
        let project = projects::get_project(conn, &id)?.ok_or_else(|| anyhow::anyhow!("Project not found"))?;
        let mut value = serde_json::to_value(project)?;
        let chats = conversations::list_conversations(conn)?.into_iter().filter(|c| c.project_id.as_deref() == Some(&id)).collect::<Vec<_>>();
        value["conversations"] = json!(chats);
        value["files"] = json!(projects::list_project_files(conn, &id)?);
        Ok(value)
    }).await?;
    Ok(Json(result))
}

async fn update(Path(id): Path<String>, State(state): State<AppState>, Json(input): Json<ProjectInput>) -> ApiResult {
    let name = input.name.map(|s| s.trim().to_string());
    if name.as_deref() == Some("") { return Err(invalid("Project name is required")); }
    let result = database(&state, move |conn| {
        let tx = conn.unchecked_transaction()?;
        projects::update_project(&tx, &id, name.as_deref(), input.description.as_deref(), input.instructions.as_deref(), None,
            input.is_archived.map(|v| v.as_bool().unwrap_or_else(|| v.as_i64().unwrap_or(0) != 0)))?;
        if let Some(folders) = input.folders { projects::set_folders(&tx, &id, &folders)?; }
        let project = projects::get_project(&tx, &id)?;
        tx.commit()?;
        Ok(project)
    }).await?;
    Ok(Json(json!(result)))
}

#[derive(Deserialize)]
struct TrustInput { folders: Vec<String> }

async fn trust(Path(id): Path<String>, State(state): State<AppState>, Json(input): Json<TrustInput>) -> ApiResult {
    let result = database(&state, move |conn| {
        let tx = conn.unchecked_transaction()?;
        projects::trust_folders(&tx, &id, &input.folders)?;
        let project = projects::get_project(&tx, &id)?;
        tx.commit()?;
        Ok(project)
    }).await?;
    Ok(Json(json!(result)))
}

async fn remove(Path(id): Path<String>, State(state): State<AppState>) -> ApiResult {
    database(&state, move |conn| {
        let tx = conn.unchecked_transaction()?;
        projects::delete_project(&tx, &id)?;
        tx.commit()?;
        Ok(())
    }).await?;
    Ok(Json(json!({"ok": true})))
}

async fn chats(Path(id): Path<String>, State(state): State<AppState>) -> ApiResult {
    let result = database(&state, move |conn| Ok(conversations::list_conversations(conn)?.into_iter().filter(|c| c.project_id.as_deref() == Some(&id)).collect::<Vec<_>>())).await?;
    Ok(Json(json!({"conversations": result})))
}

#[derive(Deserialize)]
struct ChatInput {
    title: Option<String>,
    model: Option<String>,
    #[serde(default)]
    workspace_mode: conversations::WorkspaceMode,
    branch: Option<String>,
    #[serde(default)]
    use_worktree: bool,
}

async fn git_branches(Path(id): Path<String>, State(state): State<AppState>) -> ApiResult {
    let result = database(&state, move |conn| {
        let project = projects::require_trusted(conn, &id)?;
        crate::worktree::session::branches(std::path::Path::new(project.workspace_path.as_deref().ok_or_else(|| anyhow::anyhow!("Project has no folder"))?))
    }).await?;
    Ok(Json(json!(result)))
}

async fn create_chat(Path(id): Path<String>, State(state): State<AppState>, Json(input): Json<ChatInput>) -> ApiResult {
    let result = database(&state, move |conn| {
        let project = projects::require_trusted(conn, &id)?;
        anyhow::ensure!(!project.is_archived, "Select an active project");
        let source = project.workspace_path.as_deref().ok_or_else(|| anyhow::anyhow!("Project has no folder"))?;
        let prepared = crate::worktree::session::prepare(std::path::Path::new(source), input.branch.as_deref(), input.use_worktree)?;
        let saved = (|| -> anyhow::Result<_> {
            let tx = conn.unchecked_transaction()?;
            let conversation_id = uuid::Uuid::new_v4().to_string();
            let now = chrono::Utc::now().to_rfc3339();
            conversations::insert_conversation(&tx, &conversation_id, input.title.as_deref(), input.model.as_deref(), None, Some(&prepared.path.to_string_lossy()), Some(&id), false, false, false, &now, &now, 0)?;
            prepared.save(&tx, &conversation_id, &id, source)?;
            conversations::set_workspace_mode(&tx, &conversation_id, input.workspace_mode.as_str())?;
            let conversation = conversations::get_conversation(&tx, &conversation_id)?;
            tx.commit()?;
            Ok(conversation)
        })();
        if saved.is_err() { prepared.cleanup(); }
        saved
    }).await?;
    Ok(Json(json!(result)))
}

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/projects", get(list).post(create))
        .route("/api/projects/{id}", get(detail).patch(update).delete(remove))
        .route("/api/projects/{id}/trust", post(trust))
        .route("/api/projects/{id}/git/branches", get(git_branches))
        .route("/api/projects/{id}/conversations", get(chats).post(create_chat))
}
