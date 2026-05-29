use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::browser::BrowserEngine;
use crate::desktop::DesktopEngine;
use crate::ActionRequest;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SidecarCommand {
    pub id: String,
    pub command: SidecarCommandType,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum SidecarCommandType {
    #[serde(rename = "execute_action")]
    ExecuteAction {
        action_type: String,
        coordinate: Option<[i32; 2]>,
        button: Option<String>,
        key: Option<String>,
        text: Option<String>,
        scroll_y: Option<i32>,
        scroll_x: Option<i32>,
        duration_ms: Option<u64>,
    },
    #[serde(rename = "open_browser")]
    OpenBrowser { url: Option<String> },
    #[serde(rename = "screenshot")]
    Screenshot,
    #[serde(rename = "ping")]
    Ping,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SidecarResponse {
    pub id: String,
    pub success: bool,
    pub screenshot: Option<String>,
    pub error: Option<String>,
    pub duration_ms: u64,
}

pub struct SidecarServer {
    desktop: DesktopEngine,
    browser: BrowserEngine,
}

impl SidecarServer {
    pub fn new() -> Self {
        Self {
            desktop: DesktopEngine::new(),
            browser: BrowserEngine::new(),
        }
    }

    pub async fn handle_command(&self, cmd: SidecarCommand) -> SidecarResponse {
        match cmd.command {
            SidecarCommandType::ExecuteAction {
                action_type,
                coordinate,
                button,
                key,
                text,
                scroll_y,
                scroll_x,
                duration_ms,
            } => {
                let req = ActionRequest {
                    action_type,
                    coordinate,
                    button,
                    key,
                    text,
                    scroll_y,
                    scroll_x,
                    duration_ms,
                };
                let start = std::time::Instant::now();
                let result = self.desktop.execute(&req).await;
                let duration = start.elapsed();

                let (success, error) = match result {
                    Ok(_) => (true, None),
                    Err(e) => (false, Some(e.to_string())),
                };

                let screenshot = if success {
                    self.desktop.take_screenshot().await.ok()
                } else {
                    None
                };

                SidecarResponse {
                    id: cmd.id,
                    success,
                    screenshot,
                    error,
                    duration_ms: duration.as_millis() as u64,
                }
            }
            SidecarCommandType::OpenBrowser { url } => {
                let start = std::time::Instant::now();
                let mut result = Ok(());
                if let Some(ref url) = url {
                    result = self.browser.open_browser_app(url);
                }
                let duration = start.elapsed();

                let (success, error) = match result {
                    Ok(()) => (true, None),
                    Err(e) => (false, Some(e.to_string())),
                };

                SidecarResponse {
                    id: cmd.id,
                    success,
                    screenshot: None,
                    error,
                    duration_ms: duration.as_millis() as u64,
                }
            }
            SidecarCommandType::Screenshot => {
                let start = std::time::Instant::now();
                let screenshot = self.desktop.take_screenshot().await.ok();
                let duration = start.elapsed();

                SidecarResponse {
                    id: cmd.id,
                    success: screenshot.is_some(),
                    screenshot,
                    error: if screenshot.is_none() {
                        Some("Screenshot failed".to_string())
                    } else {
                        None
                    },
                    duration_ms: duration.as_millis() as u64,
                }
            }
            SidecarCommandType::Ping => SidecarResponse {
                id: cmd.id,
                success: true,
                screenshot: None,
                error: None,
                duration_ms: 0,
            },
        }
    }

    #[cfg(unix)]
    pub async fn run_unix_socket(&self, socket_path: &str) -> Result<()> {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        use tokio::net::UnixListener;

        let _ = std::fs::remove_file(socket_path);

        let listener = UnixListener::bind(socket_path)
            .map_err(|e| anyhow!("Failed to bind Unix socket: {}", e))?;

        tracing::info!("Sidecar Unix socket listener started on {}", socket_path);

        loop {
            let (mut stream, _) = listener
                .accept()
                .await
                .map_err(|e| anyhow!("Failed to accept Unix connection: {}", e))?;

            tokio::spawn(async move {
                let mut buf = vec![0u8; 65536];
                match stream.read(&mut buf).await {
                    Ok(n) if n > 0 => {
                        let cmd: Result<SidecarCommand, _> = serde_json::from_slice(&buf[..n]);
                        match cmd {
                            Ok(cmd) => {
                                let server = SidecarServer::new();
                                let resp = server.handle_command(cmd).await;
                                let json = serde_json::to_vec(&resp).unwrap_or_default();
                                let _ = stream.write_all(&json).await;
                            }
                            Err(e) => {
                                tracing::warn!("Failed to parse sidecar command: {}", e);
                            }
                        }
                    }
                    _ => {}
                }
            });
        }
    }

    #[cfg(windows)]
    pub async fn run_named_pipe(&self, pipe_name: &str) -> Result<()> {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        use tokio::net::windows::named_pipe::ServerOptions as PipeServerOptions;

        let server = PipeServerOptions::new()
            .max_instances(10)
            .create(pipe_name)
            .map_err(|e| anyhow!("Failed to create named pipe: {}", e))?;

        tracing::info!("Sidecar named pipe listener started on {}", pipe_name);

        loop {
            server.connect().await?;

            let mut buf = vec![0u8; 65536];
            match server.try_read(&mut buf) {
                Ok(n) if n > 0 => {
                    let cmd: Result<SidecarCommand, _> = serde_json::from_slice(&buf[..n]);
                    match cmd {
                        Ok(cmd) => {
                            let srv = SidecarServer::new();
                            let resp = srv.handle_command(cmd).await;
                            let json = serde_json::to_vec(&resp).unwrap_or_default();
                            let _ = server.try_write(&json);
                        }
                        Err(e) => {
                            tracing::warn!("Failed to parse sidecar command: {}", e);
                        }
                    }
                }
                _ => {}
            }

            server.disconnect()?;
        }
    }
}