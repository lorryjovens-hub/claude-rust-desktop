use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::{broadcast, Mutex, RwLock};
use serde::{Deserialize, Serialize};
use serde_json;
use axum::{
    extract::{ws::{Message as WsMessage, WebSocket, WebSocketUpgrade}, ConnectInfo, State},
    response::IntoResponse,
    routing::get,
    Router,
};
use futures::{sink::SinkExt, stream::StreamExt};

/// Remote message types exchanged between desktop and mobile
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum RemoteMessage {
    #[serde(rename = "ping")]
    Ping,
    #[serde(rename = "pong")]
    Pong,
    #[serde(rename = "auth_request")]
    AuthRequest { device_name: String, device_id: String },
    #[serde(rename = "auth_response")]
    AuthResponse { approved: bool, token: String },
    #[serde(rename = "chat_request")]
    ChatRequest { conversation_id: String, message: String, model: String },
    #[serde(rename = "chat_response")]
    ChatResponse { conversation_id: String, content: String, done: bool },
    #[serde(rename = "conversation_list")]
    ConversationList { conversations: Vec<serde_json::Value> },
    #[serde(rename = "get_conversations")]
    GetConversations,
    #[serde(rename = "new_conversation")]
    NewConversation { title: Option<String> },
    #[serde(rename = "conversation_created")]
    ConversationCreated { id: String, title: String },
    #[serde(rename = "error")]
    Error { message: String },
    #[serde(rename = "voice_data")]
    VoiceData { data: String, format: String },
    #[serde(rename = "notification")]
    Notification { title: String, body: String },
    #[serde(rename = "sync_settings")]
    SyncSettings { settings: serde_json::Value },
    #[serde(rename = "get_settings")]
    GetSettings,
    #[serde(rename = "settings_response")]
    SettingsResponse { settings: serde_json::Value },
}

/// Connected device info
#[derive(Debug, Clone, Serialize)]
pub struct ConnectedDevice {
    pub id: String,
    pub name: String,
    pub addr: String,
    pub connected_at: String,
}

/// Shared state for the remote server
#[derive(Clone)]
pub struct RemoteServerState {
    /// Broadcast channel for messages to all connected clients
    pub tx: broadcast::Sender<RemoteMessage>,
    /// List of connected devices
    pub devices: Arc<RwLock<HashMap<String, ConnectedDevice>>>,
    /// Auth tokens for approved devices
    pub auth_tokens: Arc<RwLock<HashMap<String, String>>>,
    /// Pending auth requests
    pub pending_auth: Arc<Mutex<HashMap<String, (String, String)>>>, // device_id -> (name, addr)
}

impl RemoteServerState {
    pub fn new() -> Self {
        let (tx, _rx) = broadcast::channel::<RemoteMessage>(256);
        Self {
            tx,
            devices: Arc::new(RwLock::new(HashMap::new())),
            auth_tokens: Arc::new(RwLock::new(HashMap::new())),
            pending_auth: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub async fn add_device(&self, id: String, name: String, addr: String) {
        let device = ConnectedDevice {
            id: id.clone(),
            name,
            addr,
            connected_at: chrono::Utc::now().to_rfc3339(),
        };
        let mut devices = self.devices.write().await;
        devices.insert(id, device);
    }

    pub async fn remove_device(&self, id: &str) {
        let mut devices = self.devices.write().await;
        devices.remove(id);
        let mut tokens = self.auth_tokens.write().await;
        tokens.remove(id);
    }

    pub async fn approve_device(&self, device_id: String, token: String) {
        let mut tokens = self.auth_tokens.write().await;
        tokens.insert(device_id, token);
    }

    pub async fn is_authorized(&self, device_id: &str, token: &str) -> bool {
        let tokens = self.auth_tokens.read().await;
        tokens.get(device_id).map(|t| t == token).unwrap_or(false)
    }

    pub async fn get_devices(&self) -> Vec<ConnectedDevice> {
        let devices = self.devices.read().await;
        devices.values().cloned().collect()
    }
}

/// Start the remote WebSocket server
pub async fn start_remote_server(port: u16) -> anyhow::Result<()> {
    let state = RemoteServerState::new();

    let app = Router::new()
        .route("/ws", get(ws_handler))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{}", port)).await?;
    println!("[Remote] WebSocket server running on ws://0.0.0.0:{}", port);
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await?;
    Ok(())
}

/// WebSocket upgrade handler
async fn ws_handler(
    ws: WebSocketUpgrade,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    State(state): State<RemoteServerState>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, addr, state))
}

/// Handle an individual WebSocket connection
async fn handle_socket(socket: WebSocket, addr: SocketAddr, state: RemoteServerState) {
    let (mut sender, mut receiver) = socket.split();
    let mut rx = state.tx.subscribe();

    let mut device_id: Option<String> = None;
    let mut device_name: Option<String> = None;
    let mut is_authenticated = false;

    println!("[Remote] New connection from {}", addr);

    // Send a welcome ping
    let ping_msg = serde_json::to_string(&RemoteMessage::Ping).unwrap_or_default();
    let _ = sender.send(WsMessage::Text(ping_msg.into())).await;

    loop {
        tokio::select! {
            // Receive from client
            recv_result = receiver.next() => {
                match recv_result {
                    Some(Ok(msg)) => {
                        match msg {
                            WsMessage::Text(text) => {
                                match serde_json::from_str::<RemoteMessage>(&text) {
                                    Ok(RemoteMessage::Ping) => {
                                        let pong_msg = serde_json::to_string(&RemoteMessage::Pong).unwrap_or_default();
                                        let _ = sender.send(WsMessage::Text(pong_msg.into())).await;
                                    }
                                    Ok(RemoteMessage::AuthRequest { device_name: name, device_id: id }) => {
                                        device_id = Some(id.clone());
                                        device_name = Some(name.clone());

                                        // Add to pending auth
                                        let mut pending = state.pending_auth.lock().await;
                                        pending.insert(id.clone(), (name.clone(), addr.to_string()));
                                        drop(pending);

                                        // Notify desktop UI about auth request
                                        let auth_msg = RemoteMessage::AuthRequest {
                                            device_name: name,
                                            device_id: id,
                                        };
                                        let _ = state.tx.send(auth_msg);

                                        // Send response waiting for approval
                                        let resp_msg = serde_json::to_string(&RemoteMessage::AuthResponse {
                                            approved: false,
                                            token: String::new(),
                                        }).unwrap_or_default();
                                        let _ = sender.send(WsMessage::Text(resp_msg.into())).await;
                                    }
                                    Ok(RemoteMessage::AuthResponse { approved, token }) => {
                                        // Desktop UI sends approval
                                        if approved {
                                            if let Some(ref id) = device_id {
                                                state.approve_device(id.clone(), token.clone()).await;
                                                is_authenticated = true;
                                                if let Some(ref name) = device_name {
                                                    state.add_device(id.clone(), name.clone(), addr.to_string()).await;
                                                }
                                            }
                                        }
                                    }
                                    Ok(msg) if is_authenticated => {
                                        // Forward authenticated messages to other handlers
                                        let _ = state.tx.send(msg);
                                    }
                                    Ok(_) => {
                                        let err_msg = serde_json::to_string(&RemoteMessage::Error {
                                            message: "Unauthorized. Please authenticate first.".to_string()
                                        }).unwrap_or_default();
                                        let _ = sender.send(WsMessage::Text(err_msg.into())).await;
                                    }
                                    Err(e) => {
                                        eprintln!("[Remote] Failed to parse message: {}", e);
                                    }
                                }
                            }
                            WsMessage::Close(_) => {
                                println!("[Remote] Connection closed from {}", addr);
                                break;
                            }
                            _ => {}
                        }
                    }
                    Some(Err(e)) => {
                        eprintln!("[Remote] WebSocket error from {}: {}", addr, e);
                        break;
                    }
                    None => {
                        println!("[Remote] Connection ended from {}", addr);
                        break;
                    }
                }
            }
            // Broadcast messages to client
            Ok(msg) = rx.recv() => {
                if let Ok(json) = serde_json::to_string(&msg) {
                    let _ = sender.send(WsMessage::Text(json.into())).await;
                }
            }
        }
    }

    // Cleanup on disconnect
    if let Some(ref id) = device_id {
        state.remove_device(id).await;
    }
    println!("[Remote] Client disconnected from {}", addr);
}

/// Generate a QR code data URL for easy mobile connection
pub fn generate_qr_code_data_url(connection_url: &str) -> String {
    use qrcode::QrCode;
    use qrcode::render::svg;

    let code = match QrCode::new(connection_url) {
        Ok(c) => c,
        Err(_) => return String::new(),
    };

    let svg_data = code.render::<svg::Color>()
        .min_dimensions(200, 200)
        .dark_color(svg::Color("#000000"))
        .light_color(svg::Color("#ffffff"))
        .build();

    format!("data:image/svg+xml;base64,{}", base64_encode(svg_data))
}

// base64 encode helper
fn base64_encode(input: String) -> String {
    const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut result = String::new();
    let bytes = input.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let b1 = bytes[i];
        let b2 = if i + 1 < bytes.len() { bytes[i + 1] } else { 0 };
        let b3 = if i + 2 < bytes.len() { bytes[i + 2] } else { 0 };

        result.push(CHARS[(b1 >> 2) as usize] as char);
        result.push(CHARS[(((b1 & 0x3) << 4) | (b2 >> 4)) as usize] as char);
        result.push(if i + 1 < bytes.len() { CHARS[(((b2 & 0xF) << 2) | (b3 >> 6)) as usize] as char } else { '=' });
        result.push(if i + 2 < bytes.len() { CHARS[(b3 & 0x3F) as usize] as char } else { '=' });

        i += 3;
    }
    result
}

/// Get local IP address for connection
pub fn get_local_ip() -> Option<String> {
    local_ip_address::local_ip()
        .ok()
        .map(|ip| ip.to_string())
}
