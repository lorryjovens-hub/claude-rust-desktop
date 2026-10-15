use super::super::AppState;
use axum::{
    routing::{get, post, put},
    Router,
};
pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/memory/export", get(export))
        .route("/api/memory/import", post(import))
        .route(
            "/api/memory/search",
            post(super::super::memory_search_handler),
        )
        .route(
            "/api/memory/ingest",
            post(super::super::memory_ingest_handler),
        )
        .route("/api/memory/stats", get(super::super::memory_stats_handler))
        .route(
            "/api/memory/config",
            get(super::super::memory_get_config_handler),
        )
        .route(
            "/api/memory/config",
            put(super::super::memory_update_config_handler),
        )
        .route(
            "/api/memory/clear",
            post(super::super::memory_clear_handler),
        )
}

async fn export(
    axum::extract::State(state): axum::extract::State<AppState>,
) -> Result<
    axum::Json<crate::memory::MemoryArchive>,
    (axum::http::StatusCode, axum::Json<serde_json::Value>),
> {
    state
        .memex_client
        .export_archive()
        .await
        .map(axum::Json)
        .map_err(error)
}

#[derive(serde::Deserialize)]
struct ImportRequest {
    archive: serde_json::Value,
    preview: bool,
}

async fn import(
    axum::extract::State(state): axum::extract::State<AppState>,
    axum::Json(request): axum::Json<ImportRequest>,
) -> Result<
    axum::Json<crate::memory::MemoryImportReport>,
    (axum::http::StatusCode, axum::Json<serde_json::Value>),
> {
    state
        .memex_client
        .import_archive(request.archive, request.preview)
        .await
        .map(axum::Json)
        .map_err(error)
}

fn error(error: anyhow::Error) -> (axum::http::StatusCode, axum::Json<serde_json::Value>) {
    (
        axum::http::StatusCode::BAD_REQUEST,
        axum::Json(serde_json::json!({"error":error.to_string()})),
    )
}
