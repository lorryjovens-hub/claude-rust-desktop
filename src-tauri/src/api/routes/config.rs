//! Configuration endpoints.

use crate::api::state::AppState;
use crate::config::AppConfig;
use axum::{Router, routing::get, extract::State, Json};
use serde_json::json;

async fn config_get(State(state): State<AppState>) -> Json<serde_json::Value> {
    let manager = state.config_manager.lock().await;
    if let Some(m) = manager.as_ref() {
        let mut value = serde_json::to_value(m.get_config()).unwrap_or_default();
        if let Some(search) = value["deepseek_search"].as_object_mut() {
            search.remove("api_key");
            search.insert("has_api_key".into(), json!(!m.get_config().deepseek_search.api_key.trim().is_empty()));
        }
        return Json(value);
    }
    Json(json!({}))
}

async fn config_update(State(state): State<AppState>, Json(config): Json<AppConfig>) -> Json<serde_json::Value> {
    let mut manager = state.config_manager.lock().await;
    if let Some(m) = manager.as_mut() {
        let _ = m.update_config(|c| {
            // Dedicated search settings own this field; generic saves preserve it.
            let search = c.deepseek_search.clone();
            *c = config;
            c.deepseek_search = search;
        });
    }
    Json(json!({"ok": true}))
}

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/config", get(config_get).put(config_update))
}
