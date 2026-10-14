use super::super::AppState;
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use axum::{
    routing::{get, post},
    Router,
};
use serde::Deserialize;
use serde_json::{json, Value};

async fn in_workspace<F>(
    state: AppState,
    id: String,
    operation: F,
) -> Result<Json<Value>, (StatusCode, Json<Value>)>
where
    F: FnOnce(&std::path::Path) -> anyhow::Result<Value> + Send + 'static,
{
    let result = tokio::task::spawn_blocking(move || {
        state.db_manager.with_conn(|conn| {
            let project = crate::native_engine::workspace::conversation_project(conn, &id)?
                .ok_or_else(|| anyhow::anyhow!("Select a project first"))?;
            let source = std::path::PathBuf::from(
                project
                    .workspace_path
                    .ok_or_else(|| anyhow::anyhow!("Project folder is unavailable"))?,
            );
            let cwd = crate::worktree::session::resolve(conn, &id, &project.id, &source)?
                .unwrap_or(source);
            operation(&cwd)
        })
    })
    .await;
    match result {
        Ok(Ok(Ok(value))) => Ok(Json(value)),
        error => Err((
            StatusCode::BAD_REQUEST,
            Json(json!({"error": match error {
                Ok(Ok(Err(e))) => e.to_string(), Ok(Err(e)) => e.to_string(), Err(e) => e.to_string(), _ => unreachable!()
            }})),
        )),
    }
}

async fn changes(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    in_workspace(state, id, |cwd| {
        Ok(serde_json::to_value(crate::worktree::changes::summary(
            cwd,
        )?)?)
    })
    .await
}

#[derive(Deserialize)]
struct DiffQuery {
    file: String,
}
async fn file_diff(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(query): Query<DiffQuery>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    in_workspace(state, id, move |cwd| {
        Ok(serde_json::to_value(crate::worktree::changes::diff(
            cwd,
            &query.file,
        )?)?)
    })
    .await
}

#[derive(Deserialize)]
struct Action {
    action: String,
}
async fn workspace_action(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(action): Json<Action>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    in_workspace(state, id, move |cwd| {
        crate::worktree::changes::open_workspace(cwd, &action.action)?;
        Ok(json!({"ok": true}))
    })
    .await
}
pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/conversations/{id}/git", get(changes))
        .route("/api/conversations/{id}/git/diff", get(file_diff))
        .route("/api/conversations/{id}/git/action", post(workspace_action))
        .route("/api/git/status", get(super::super::git_status_handler))
        .route("/api/git/log", get(super::super::git_log_handler))
        .route("/api/git/diff", get(super::super::git_diff_handler))
        .route("/api/git/commit", post(super::super::git_commit_handler))
        .route("/api/git/push", post(super::super::git_push_handler))
        .route("/api/git/pull", post(super::super::git_pull_handler))
}
