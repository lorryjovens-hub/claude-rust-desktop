use axum::{
    extract::Json,
    routing::{get, post},
    Router,
};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::RwLock;
use tower_http::cors::{Any, CorsLayer};

mod browser;
mod desktop;
mod server;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionRequest {
    pub action_type: String,
    pub coordinate: Option<[i32; 2]>,
    pub button: Option<String>,
    pub key: Option<String>,
    pub text: Option<String>,
    pub scroll_y: Option<i32>,
    pub scroll_x: Option<i32>,
    pub duration_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionResponse {
    pub success: bool,
    pub screenshot: Option<String>,
    pub error: Option<String>,
    pub duration_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenUrlRequest {
    pub url: String,
    pub wait_until: Option<String>,
}

pub struct AppState {
    pub browser_engine: browser::BrowserEngine,
    pub desktop_engine: desktop::DesktopEngine,
    pub active_sessions: Arc<RwLock<Vec<String>>>,
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "agent_worker=info".into()),
        )
        .init();

    tracing::info!("Agent Worker starting on independent Tokio runtime (pid: {})", std::process::id());

    let state = Arc::new(AppState {
        browser_engine: browser::BrowserEngine::new(),
        desktop_engine: desktop::DesktopEngine::new(),
        active_sessions: Arc::new(RwLock::new(Vec::new())),
    });

    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    let app = Router::new()
        .route("/health", get(health_check))
        .route("/execute", post(execute_action))
        .route("/screenshot", get(take_screenshot))
        .route("/open_url", post(open_url))
        .route("/mouse_click", post(mouse_click))
        .route("/mouse_move", post(mouse_move))
        .route("/key_press", post(key_press))
        .route("/type_text", post(type_text))
        .layer(cors)
        .with_state(state);

    let addr = SocketAddr::from(([127, 0, 0, 1], 9527));
    tracing::info!("HTTP Bridge server listening on {}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

async fn health_check() -> &'static str {
    "Agent Worker OK"
}

async fn execute_action(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
    Json(req): Json<ActionRequest>,
) -> Json<ActionResponse> {
    let start = std::time::Instant::now();

    let result = state.desktop_engine.execute(&req).await;
    let duration = start.elapsed();

    let (success, error) = match result {
        Ok(_) => (true, None),
        Err(e) => (false, Some(e.to_string())),
    };

    let screenshot = if req.action_type == "screenshot" || success {
        state.desktop_engine.take_screenshot().await.ok()
    } else {
        None
    };

    Json(ActionResponse {
        success,
        screenshot,
        error,
        duration_ms: duration.as_millis() as u64,
    })
}

async fn take_screenshot(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
) -> Json<ActionResponse> {
    let start = std::time::Instant::now();
    let screenshot = state.desktop_engine.take_screenshot().await.ok();
    let duration = start.elapsed();

    Json(ActionResponse {
        success: screenshot.is_some(),
        screenshot,
        error: if screenshot.is_none() {
            Some("Screenshot failed".to_string())
        } else {
            None
        },
        duration_ms: duration.as_millis() as u64,
    })
}

async fn open_url(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
    Json(req): Json<OpenUrlRequest>,
) -> Json<ActionResponse> {
    let start = std::time::Instant::now();
    let result = state.browser_engine.open_url(&req.url).await;
    let duration = start.elapsed();

    let (success, error) = match &result {
        Ok(_) => (true, None),
        Err(e) => (false, Some(e.to_string())),
    };

    Json(ActionResponse {
        success,
        screenshot: result.ok(),
        error,
        duration_ms: duration.as_millis() as u64,
    })
}

async fn mouse_click(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
    Json(req): Json<ActionRequest>,
) -> Json<ActionResponse> {
    let start = std::time::Instant::now();
    let result = state.desktop_engine.execute(&req).await;
    let duration = start.elapsed();

    let (success, error) = match result {
        Ok(_) => (true, None),
        Err(e) => (false, Some(e.to_string())),
    };

    Json(ActionResponse {
        success,
        screenshot: None,
        error,
        duration_ms: duration.as_millis() as u64,
    })
}

async fn mouse_move(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
    Json(req): Json<ActionRequest>,
) -> Json<ActionResponse> {
    let start = std::time::Instant::now();
    let result = state.desktop_engine.execute(&req).await;
    let duration = start.elapsed();

    let (success, error) = match result {
        Ok(_) => (true, None),
        Err(e) => (false, Some(e.to_string())),
    };

    Json(ActionResponse {
        success,
        screenshot: None,
        error,
        duration_ms: duration.as_millis() as u64,
    })
}

async fn key_press(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
    Json(req): Json<ActionRequest>,
) -> Json<ActionResponse> {
    let start = std::time::Instant::now();
    let result = state.desktop_engine.execute(&req).await;
    let duration = start.elapsed();

    let (success, error) = match result {
        Ok(_) => (true, None),
        Err(e) => (false, Some(e.to_string())),
    };

    Json(ActionResponse {
        success,
        screenshot: None,
        error,
        duration_ms: duration.as_millis() as u64,
    })
}

async fn type_text(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
    Json(req): Json<ActionRequest>,
) -> Json<ActionResponse> {
    let start = std::time::Instant::now();
    let result = state.desktop_engine.execute(&req).await;
    let duration = start.elapsed();

    let (success, error) = match result {
        Ok(_) => (true, None),
        Err(e) => (false, Some(e.to_string())),
    };

    Json(ActionResponse {
        success,
        screenshot: None,
        error,
        duration_ms: duration.as_millis() as u64,
    })
}