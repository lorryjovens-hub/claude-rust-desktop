use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::Mutex;
use tokio::time::{timeout, Duration};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerConfig {
    pub name: String,
    pub command: String,
    pub args: Vec<String>,
    pub env: HashMap<String, String>,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpTool {
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpResource {
    pub uri: String,
    pub name: String,
    pub mime_type: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpConnectorStatus {
    pub name: String,
    pub server_installed: bool,
    pub tools: Vec<McpTool>,
    pub resources: Vec<McpResource>,
    pub error: Option<String>,
}

pub struct McpConnector {
    config: McpServerConfig,
    process: Option<Child>,
    request_id: Mutex<u64>,
    stdin: Mutex<Option<tokio::process::ChildStdin>>,
    stdout: Mutex<Option<tokio::process::ChildStdout>>,
}

impl McpConnector {
    pub fn new(config: McpServerConfig) -> Self {
        Self {
            config,
            process: None,
            request_id: Mutex::new(0),
            stdin: Mutex::new(None),
            stdout: Mutex::new(None),
        }
    }

    pub async fn start(&mut self) -> Result<()> {
        let mut cmd = Command::new(&self.config.command);
        cmd.args(&self.config.args)
            .envs(&self.config.env)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());

        let mut child = cmd.spawn()?;

        let stdin = child.stdin.take().ok_or_else(|| anyhow!("Failed to take stdin"))?;
        let stdout = child.stdout.take().ok_or_else(|| anyhow!("Failed to take stdout"))?;

        self.stdin = Mutex::new(Some(stdin));
        self.stdout = Mutex::new(Some(stdout));
        self.process = Some(child);

        self.initialize().await?;

        Ok(())
    }

    async fn initialize(&self) -> Result<()> {
        let request = McpJsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: 1,
            method: "initialize".to_string(),
            params: Some(serde_json::json!({
                "protocolVersion": "2024-11-05",
                "capabilities": {
                    "roots": {"listChanged": true},
                    "sampling": {}
                },
                "clientInfo": {
                    "name": "claude-desktop-tauri",
                    "version": "1.6.12"
                }
            })),
        };

        self.send_json_rpc(request).await?;
        Ok(())
    }

    pub async fn list_tools(&self) -> Result<Vec<McpTool>> {
        let request = McpJsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: self.next_id().await,
            method: "tools/list".to_string(),
            params: None,
        };

        let response = self.send_json_rpc(request).await?;

        let tools = response
            .result
            .as_ref()
            .and_then(|r| r.get("tools"))
            .and_then(|t| t.as_array())
            .map(|arr| {
                arr.iter().filter_map(|t| {
                    Some(McpTool {
                        name: t.get("name")?.as_str()?.to_string(),
                        description: t.get("description")
                            .and_then(|d| d.as_str())
                            .unwrap_or("")
                            .to_string(),
                        input_schema: t.get("inputSchema").cloned().unwrap_or(serde_json::json!({})),
                    })
                }).collect()
            })
            .unwrap_or_default();

        Ok(tools)
    }

    pub async fn call_tool(&self, name: &str, arguments: serde_json::Value) -> Result<serde_json::Value> {
        let request = McpJsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: self.next_id().await,
            method: "tools/call".to_string(),
            params: Some(serde_json::json!({
                "name": name,
                "arguments": arguments
            })),
        };

        let response = self.send_json_rpc(request).await?;

        response.result.ok_or_else(|| anyhow!("No result in tool call response"))
    }

    pub async fn list_resources(&self) -> Result<Vec<McpResource>> {
        let request = McpJsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: self.next_id().await,
            method: "resources/list".to_string(),
            params: None,
        };

        let response = self.send_json_rpc(request).await?;

        let resources = response
            .result
            .as_ref()
            .and_then(|r| r.get("resources"))
            .and_then(|t| t.as_array())
            .map(|arr| {
                arr.iter().filter_map(|r| {
                    Some(McpResource {
                        uri: r.get("uri")?.as_str()?.to_string(),
                        name: r.get("name")
                            .and_then(|n| n.as_str())
                            .unwrap_or("")
                            .to_string(),
                        mime_type: r.get("mimeType").and_then(|m| m.as_str()).map(String::from),
                    })
                }).collect()
            })
            .unwrap_or_default();

        Ok(resources)
    }

    async fn next_id(&self) -> u64 {
        let mut counter = self.request_id.lock().await;
        *counter += 1;
        *counter
    }

    async fn send_json_rpc(&self, request: McpJsonRpcRequest) -> Result<McpJsonRpcResponse> {
        let mut stdin = self.stdin.lock().await;
        let stdin = stdin.as_mut().ok_or_else(|| anyhow!("MCP stdin not available"))?;

        let json = serde_json::to_string(&request)?;
        stdin.write_all(format!("{}\n", json).as_bytes()).await?;

        drop(stdin);

        let mut stdout = self.stdout.lock().await;
        let stdout = stdout.as_mut().ok_or_else(|| anyhow!("MCP stdout not available"))?;

        let mut reader = BufReader::new(stdout);
        let mut line = String::new();

        timeout(Duration::from_secs(30), reader.read_line(&mut line)).await??;

        let response: McpJsonRpcResponse = serde_json::from_str(&line.trim())?;

        Ok(response)
    }

    pub async fn stop(&mut self) -> Result<()> {
        if let Some(mut child) = self.process.take() {
            child.kill().await?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpJsonRpcRequest {
    pub jsonrpc: String,
    pub id: u64,
    pub method: String,
    pub params: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpJsonRpcResponse {
    pub jsonrpc: String,
    #[serde(default)]
    pub id: Option<u64>,
    #[serde(default)]
    pub result: Option<serde_json::Value>,
    #[serde(default)]
    pub error: Option<McpJsonRpcError>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpJsonRpcError {
    pub code: i32,
    pub message: String,
    pub data: Option<serde_json::Value>,
}

#[derive(Default)]
pub struct McpConfigManager {
    pub servers: HashMap<String, McpServerConfig>,
}

impl McpConfigManager {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn load_from_file(&mut self, config_path: &PathBuf) -> Result<()> {
        if !config_path.exists() {
            return Ok(());
        }

        let content = std::fs::read_to_string(config_path)?;
        let config: serde_json::Value = serde_json::from_str(&content)?;

        if let Some(servers) = config.get("mcpServers").and_then(|s| s.as_object()) {
            for (name, value) in servers {
                let server = McpServerConfig {
                    name: name.clone(),
                    command: value.get("command")
                        .and_then(|c| c.as_str())
                        .unwrap_or("")
                        .to_string(),
                    args: value.get("args")
                        .and_then(|a| a.as_array())
                        .map(|arr| {
                            arr.iter()
                                .filter_map(|s| s.as_str().map(String::from))
                                .collect()
                        })
                        .unwrap_or_default(),
                    env: value.get("env")
                        .and_then(|e| e.as_object())
                        .map(|obj| {
                            obj.iter()
                                .filter_map(|(k, v)| {
                                    v.as_str().map(|s| (k.clone(), s.to_string()))
                                })
                                .collect()
                        })
                        .unwrap_or_default(),
                    enabled: value.get("enabled")
                        .and_then(|e| e.as_bool())
                        .unwrap_or(true),
                };
                self.servers.insert(name.clone(), server);
            }
        }

        Ok(())
    }

    pub fn save_to_file(&self, config_path: &PathBuf) -> Result<()> {
        let mut servers = serde_json::Map::new();

        for (name, server) in &self.servers {
            let mut server_obj = serde_json::Map::new();
            server_obj.insert("command".to_string(), serde_json::json!(server.command));
            server_obj.insert("args".to_string(), serde_json::json!(server.args));
            server_obj.insert("env".to_string(), serde_json::json!(server.env));
            server_obj.insert("enabled".to_string(), serde_json::json!(server.enabled));
            servers.insert(name.clone(), serde_json::json!(server_obj));
        }

        let config = serde_json::json!({
            "mcpServers": servers
        });

        std::fs::write(config_path, serde_json::to_string_pretty(&config)?)?;
        Ok(())
    }

    pub fn add_server(&mut self, name: String, server: McpServerConfig) {
        self.servers.insert(name, server);
    }

    pub fn remove_server(&mut self, name: &str) -> Option<McpServerConfig> {
        self.servers.remove(name)
    }

    pub fn get_server(&self, name: &str) -> Option<&McpServerConfig> {
        self.servers.get(name)
    }

    pub fn list_servers(&self) -> Vec<&McpServerConfig> {
        self.servers.values().collect()
    }
}
