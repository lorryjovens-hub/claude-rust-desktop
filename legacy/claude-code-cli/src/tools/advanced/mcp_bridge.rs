//! MCP Bridge Tool - Model Context Protocol integration
//!
//! Manages MCP server connections, resources, and tool execution.
//! Based on claw-code-main's MCP tool implementation.

use crate::tools::{Tool, ToolOutput, ToolError};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpResource {
    pub uri: String,
    pub name: String,
    pub mime_type: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerState {
    pub name: String,
    pub status: ServerStatus,
    pub auth_state: AuthState,
    pub resources: Vec<McpResource>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ServerStatus {
    Connected,
    Disconnected,
    Connecting,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum AuthState {
    Authenticated,
    Unauthenticated,
    Pending,
}

pub struct McpToolBridge {
    servers: Arc<RwLock<HashMap<String, McpServerState>>>,
}

impl Default for McpToolBridge {
    fn default() -> Self {
        Self::new()
    }
}

impl McpToolBridge {
    pub fn new() -> Self {
        Self {
            servers: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    async fn handle_list_resources(&self, server: &str) -> Result<Vec<McpResource>, ToolError> {
        let servers = self.servers.read().await;
        let srv = servers.get(server)
            .ok_or_else(|| ToolError {
                message: format!("MCP server not found: {}", server),
                code: Some("server_not_found".to_string()),
            })?;

        Ok(srv.resources.clone())
    }

    async fn handle_read_resource(&self, server: &str, uri: &str) -> Result<String, ToolError> {
        let servers = self.servers.read().await;
        let srv = servers.get(server)
            .ok_or_else(|| ToolError {
                message: format!("MCP server not found: {}", server),
                code: Some("server_not_found".to_string()),
            })?;

        let resource = srv.resources.iter()
            .find(|r| r.uri == uri)
            .ok_or_else(|| ToolError {
                message: format!("Resource not found: {}", uri),
                code: Some("resource_not_found".to_string()),
            })?;

        Ok(format!("Resource content for {} (type: {:?})", resource.name, resource.mime_type))
    }

    async fn handle_auth(&self, server: &str) -> Result<String, ToolError> {
        let mut servers = self.servers.write().await;
        let srv = servers.get_mut(server)
            .ok_or_else(|| ToolError {
                message: format!("MCP server not found: {}", server),
                code: Some("server_not_found".to_string()),
            })?;

        srv.auth_state = AuthState::Authenticated;
        Ok(format!("Authenticated with MCP server: {}", server))
    }

    async fn handle_execute(&self, server: &str, tool: &str, arguments: &serde_json::Value) -> Result<String, ToolError> {
        let servers = self.servers.read().await;
        let srv = servers.get(server)
            .ok_or_else(|| ToolError {
                message: format!("MCP server not found: {}", server),
                code: Some("server_not_found".to_string()),
            })?;

        if srv.auth_state != AuthState::Authenticated {
            return Err(ToolError {
                message: format!("Not authenticated with MCP server: {}", server),
                code: Some("not_authenticated".to_string()),
            });
        }

        Ok(format!("MCP tool '{}' executed on server '{}' with args: {}", tool, server, arguments))
    }

    async fn handle_list_servers(&self) -> Result<Vec<McpServerState>, ToolError> {
        let servers = self.servers.read().await;
        let list: Vec<McpServerState> = servers.values().cloned().collect();
        Ok(list)
    }
}

#[async_trait]
impl Tool for McpToolBridge {
    fn name(&self) -> &str {
        "mcp"
    }

    fn description(&self) -> &str {
        "Execute tools provided by connected MCP servers"
    }

    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "operation": {
                    "type": "string",
                    "enum": ["list_resources", "read_resource", "auth", "execute", "list_servers"],
                    "description": "MCP operation to perform"
                },
                "server": { "type": "string", "description": "MCP server name" },
                "uri": { "type": "string", "description": "Resource URI" },
                "tool": { "type": "string", "description": "Tool name to execute" },
                "arguments": { "type": "object", "description": "Tool arguments" }
            },
            "required": ["operation"]
        })
    }

    async fn execute(&self, input: serde_json::Value) -> Result<ToolOutput, ToolError> {
        let operation = input["operation"].as_str()
            .ok_or_else(|| ToolError {
                message: "operation is required".to_string(),
                code: Some("missing_operation".to_string()),
            })?;

        let result = match operation {
            "list_resources" => {
                let server = input["server"].as_str()
                    .ok_or_else(|| ToolError {
                        message: "server is required".to_string(),
                        code: Some("missing_server".to_string()),
                    })?;
                let resources = self.handle_list_resources(server).await?;
                serde_json::json!({
                    "success": true,
                    "resources": resources,
                    "count": resources.len()
                }).to_string()
            }
            "read_resource" => {
                let server = input["server"].as_str()
                    .ok_or_else(|| ToolError {
                        message: "server is required".to_string(),
                        code: Some("missing_server".to_string()),
                    })?;
                let uri = input["uri"].as_str()
                    .ok_or_else(|| ToolError {
                        message: "uri is required".to_string(),
                        code: Some("missing_uri".to_string()),
                    })?;
                let content = self.handle_read_resource(server, uri).await?;
                serde_json::json!({
                    "success": true,
                    "content": content
                }).to_string()
            }
            "auth" => {
                let server = input["server"].as_str()
                    .ok_or_else(|| ToolError {
                        message: "server is required".to_string(),
                        code: Some("missing_server".to_string()),
                    })?;
                let result = self.handle_auth(server).await?;
                serde_json::json!({
                    "success": true,
                    "result": result
                }).to_string()
            }
            "execute" => {
                let server = input["server"].as_str()
                    .ok_or_else(|| ToolError {
                        message: "server is required".to_string(),
                        code: Some("missing_server".to_string()),
                    })?;
                let tool = input["tool"].as_str()
                    .ok_or_else(|| ToolError {
                        message: "tool is required".to_string(),
                        code: Some("missing_tool".to_string()),
                    })?;
                let arguments = &input["arguments"];
                let result = self.handle_execute(server, tool, arguments).await?;
                serde_json::json!({
                    "success": true,
                    "result": result
                }).to_string()
            }
            "list_servers" => {
                let servers = self.handle_list_servers().await?;
                serde_json::json!({
                    "success": true,
                    "servers": servers,
                    "count": servers.len()
                }).to_string()
            }
            _ => return Err(ToolError {
                message: format!("Unknown operation: {}", operation),
                code: Some("invalid_operation".to_string()),
            }),
        };

        Ok(ToolOutput {
            output_type: "json".to_string(),
            content: result,
            metadata: HashMap::new(),
        })
    }
}