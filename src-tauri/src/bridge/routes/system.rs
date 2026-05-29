use axum::{
    routing::{get, post},
    Router,
};
use super::super::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/health", get(super::super::health_handler))
        .route("/metrics", get(super::super::metrics_handler))
        .route("/api/system-status", get(super::super::system_status))
        .route("/api/workspace-config", get(super::super::workspace_config_get))
        .route("/api/workspace-config", post(super::super::workspace_config_set))
}
