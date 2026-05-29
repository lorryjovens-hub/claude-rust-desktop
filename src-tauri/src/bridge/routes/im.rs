use axum::{
    extract::{Path, State, Query},
    http::StatusCode,
    response::sse::{Event, KeepAlive, Sse},
    routing::{get, post},
    Json, Router,
};
use axum::response::IntoResponse;
use serde::{Deserialize};
use serde_json::{self, json};
use std::convert::Infallible;
use std::sync::Arc;
use tokio::time::Duration;
use tracing;

use super::super::{AppState, set_sse_content_type};

#[derive(Deserialize)]
struct ImSendRequest {
    platform: String,
    chat_id: String,
    message: String,
}

async fn im_webhook_handler(
    Path(platform): Path<String>,
    State(state): State<AppState>,
    Json(payload): Json<serde_json::Value>,
) -> (StatusCode, Json<serde_json::Value>) {
    tracing::error!(module = "IM_Webhook", "Received webhook for platform: {}", platform);

    let db_manager = state.db_manager.clone();
    let im_manager = Arc::new(crate::im_integration::ImIntegrationManager::new(db_manager));

    match im_manager.receive_message(&platform, payload).await {
        Ok(msg) => {
            tracing::error!(module = "IM_Webhook", "Parsed message from {}: chat_id={}, content_len={}", msg.platform, msg.chat_id, msg.content.len());
            (StatusCode::OK, Json(json!({"status": "ok", "message": "received"})))
        }
        Err(e) => {
            tracing::error!(module = "IM_Webhook", "Failed to parse message: {}", e);
            (StatusCode::BAD_REQUEST, Json(json!({"status": "error", "message": e.to_string()})))
        }
    }
}

async fn im_connections_list(
    State(state): State<AppState>,
) -> Json<serde_json::Value> {
    let db_manager = state.db_manager.clone();
    let im_manager = Arc::new(crate::im_integration::ImIntegrationManager::new(db_manager));

    let connections: Vec<crate::im_integration::ImConnectionInfo> = im_manager.list_connections().await;
    let result: Vec<serde_json::Value> = connections.iter().map(|c| {
        json!({
            "id": c.id,
            "platform": c.platform,
            "status": c.status,
            "config": {
                "webhook_url": c.config.webhook_url,
                "has_token": !c.config.token.is_empty(),
            },
            "created_at": c.created_at,
            "updated_at": c.updated_at,
        })
    }).collect();

    Json(json!({"connections": result}))
}

async fn im_send_handler(
    State(state): State<AppState>,
    Json(req): Json<ImSendRequest>,
) -> (StatusCode, Json<serde_json::Value>) {
    tracing::error!(module = "IM_Send", "platform={}, chat_id={}", req.platform, req.chat_id);

    let db_manager = state.db_manager.clone();
    let im_manager = Arc::new(crate::im_integration::ImIntegrationManager::new(db_manager));

    match im_manager.send_message(&req.platform, &req.chat_id, &req.message).await {
        Ok(()) => {
            (StatusCode::OK, Json(json!({"status": "ok"})))
        }
        Err(e) => {
            tracing::error!(module = "IM_Send", "Failed: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"status": "error", "message": e.to_string()})))
        }
    }
}

#[derive(Deserialize)]
struct ImStatusQuery {
    platform: Option<String>,
}

async fn im_status_handler(
    State(state): State<AppState>,
    Query(query): Query<ImStatusQuery>,
) -> Json<serde_json::Value> {
    let db_manager = state.db_manager.clone();
    let im_manager = Arc::new(crate::im_integration::ImIntegrationManager::new(db_manager));

    if let Some(platform) = query.platform {
        match im_manager.get_connection_status(&platform).await {
            Ok(status) => Json(json!({
                "status": "ok",
                "platform": status.platform,
                "connected": status.connected,
                "connection_status": status.status,
                "last_connected_at": status.last_connected_at,
            })),
            Err(e) => Json(json!({
                "status": "error",
                "message": e.to_string(),
            })),
        }
    } else {
        match im_manager.get_all_connection_status().await {
            Ok(statuses) => {
                let results: Vec<serde_json::Value> = statuses.iter().map(|s| {
                    json!({
                        "platform": s.platform,
                        "connected": s.connected,
                        "connection_status": s.status,
                        "last_connected_at": s.last_connected_at,
                    })
                }).collect();
                Json(json!({
                    "status": "ok",
                    "connections": results,
                }))
            }
            Err(e) => Json(json!({
                "status": "error",
                "message": e.to_string(),
            })),
        }
    }
}

#[derive(Deserialize)]
struct ImStatsQuery {
    platform: Option<String>,
}

async fn im_stats_handler(
    State(state): State<AppState>,
    Query(query): Query<ImStatsQuery>,
) -> Json<serde_json::Value> {
    let db_manager = state.db_manager.clone();
    let im_manager = Arc::new(crate::im_integration::ImIntegrationManager::new(db_manager));

    match im_manager.get_message_stats(query.platform.as_deref()).await {
        Ok(stats) => Json(json!({
            "status": "ok",
            "platform": stats.platform,
            "total_messages": stats.total_messages,
            "total_sessions": stats.total_sessions,
            "active_today": stats.active_today,
            "avg_response_time_ms": stats.avg_response_time_ms,
        })),
        Err(e) => Json(json!({
            "status": "error",
            "message": e.to_string(),
        })),
    }
}

#[derive(Deserialize)]
struct ImPermissionsQuery {
    platform: String,
}

async fn im_permissions_handler(
    State(state): State<AppState>,
    Query(query): Query<ImPermissionsQuery>,
) -> Json<serde_json::Value> {
    let db_manager = state.db_manager.clone();
    let im_manager = Arc::new(crate::im_integration::ImIntegrationManager::new(db_manager));

    let mode = im_manager.get_permission_mode(&query.platform).await;

    match im_manager.get_permissions(&query.platform).await {
        Ok(permissions) => {
            let perms: Vec<serde_json::Value> = permissions.iter().map(|p| {
                json!({
                    "id": p.id,
                    "platform": p.platform,
                    "user_id": p.user_id,
                    "permission_mode": p.permission_mode.as_str(),
                    "is_allowed": p.is_allowed,
                    "paired_code": p.paired_code,
                    "created_at": p.created_at.to_rfc3339(),
                    "updated_at": p.updated_at.to_rfc3339(),
                })
            }).collect();
            Json(json!({
                "status": "ok",
                "platform": query.platform,
                "permission_mode": mode.as_str(),
                "permissions": perms,
            }))
        }
        Err(e) => Json(json!({
            "status": "error",
            "message": e.to_string(),
        })),
    }
}

#[derive(Deserialize)]
struct ImLogsQuery {
    platform: Option<String>,
}

async fn im_logs_handler(
    State(state): State<AppState>,
    Query(query): Query<ImLogsQuery>,
) -> Json<serde_json::Value> {
    let db_manager = state.db_manager.clone();
    let im_manager = Arc::new(crate::im_integration::ImIntegrationManager::new(db_manager));

    match im_manager.get_error_logs(query.platform.as_deref()).await {
        Ok(logs) => {
            let log_entries: Vec<serde_json::Value> = logs.iter().map(|l| {
                json!({
                    "id": l.id,
                    "platform": l.platform,
                    "error_type": l.error_type,
                    "error_message": l.error_message,
                    "stack_trace": l.stack_trace,
                    "created_at": l.created_at,
                })
            }).collect();
            Json(json!({
                "status": "ok",
                "logs": log_entries,
            }))
        }
        Err(e) => Json(json!({
            "status": "error",
            "message": e.to_string(),
        })),
    }
}

async fn im_status_stream_handler(
    State(state): State<AppState>,
) -> Result<impl IntoResponse, StatusCode> {
    let db_manager = state.db_manager.clone();
    let im_manager = Arc::new(crate::im_integration::ImIntegrationManager::new(db_manager));

    let stream = async_stream::stream! {
        let mut interval = tokio::time::interval(Duration::from_secs(5));
        loop {
            interval.tick().await;

            match im_manager.get_all_connection_status().await {
                Ok(statuses) => {
                    let data = serde_json::json!({
                        "type": "connection_status",
                        "timestamp": chrono::Utc::now().to_rfc3339(),
                        "connections": statuses.iter().map(|s| {
                            json!({
                                "platform": s.platform,
                                "connected": s.connected,
                                "status": s.status,
                                "last_connected_at": s.last_connected_at,
                            })
                        }).collect::<Vec<_>>(),
                    });
                    yield Ok::<Event, Infallible>(Event::default().data(data.to_string()));
                }
                Err(e) => {
                    let data = serde_json::json!({
                        "type": "error",
                        "message": e.to_string(),
                    });
                    yield Ok::<Event, Infallible>(Event::default().data(data.to_string()));
                }
            }
        }
    };

    let mut response = Sse::new(stream).keep_alive(KeepAlive::default()).into_response();
    set_sse_content_type(&mut response);
    Ok(response)
}

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/im/webhook/{platform}", post(im_webhook_handler))
        .route("/api/im/connections", get(im_connections_list))
        .route("/api/im/send", post(im_send_handler))
        .route("/api/im/status", get(im_status_handler))
        .route("/api/im/stats", get(im_stats_handler))
        .route("/api/im/permissions", get(im_permissions_handler))
        .route("/api/im/logs", get(im_logs_handler))
        .route("/api/im/status/stream", get(im_status_stream_handler))
}
