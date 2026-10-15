//! MCP Tools - Model Context Protocol integration
//!
//! Provides tools for MCP server resource management and authentication.

use crate::tools::{Tool, ToolOutput, ToolError};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServer {
    pub id: String,
    pub name: String,
    pub url: String,
    pub auth_token: Option<String>,
    pub enabled: bool,
}

impl McpServer {
    pub fn new(id: String, name: String, url: String) -> Self {
        Self {
            id,
            name,
            url,
            auth_token: None,
            enabled: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpResource {
    pub uri: String,
    pub name: String,
    pub mime_type: Option<String>,
    pub size: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceEntry {
    pub uri: String,
    pub name: String,
    pub mime_type: Option<String>,
    pub content: String,
}

pub struct McpStore {
    servers: HashMap<String, McpServer>,
    resources: HashMap<String, Vec<McpResource>>,
}

impl Default for McpStore {
    fn default() -> Self {
        Self::new()
    }
}

impl McpStore {
    pub fn new() -> Self {
        Self {
            servers: HashMap::new(),
            resources: HashMap::new(),
        }
    }

    pub fn register_server(&mut self, server: McpServer) {
        self.servers.insert(server.id.clone(), server);
    }

    pub fn get_servers(&self) -> Vec<&McpServer> {
        self.servers.values().collect()
    }

    pub fn get_server(&self, id: &str) -> Option<&McpServer> {
        self.servers.get(id)
    }

    pub fn add_resource(&mut self, server_id: &str, resource: McpResource) {
        self.resources.entry(server_id.to_string()).or_default().push(resource);
    }

    pub fn get_resources(&self, server_id: &str) -> Option<&Vec<McpResource>> {
        self.resources.get(server_id)
    }

    pub fn set_auth_token(&mut self, server_id: &str, token: String) {
        if let Some(server) = self.servers.get_mut(server_id) {
            server.auth_token = Some(token);
        }
    }
}

pub type SharedMcpStore = Arc<RwLock<McpStore>>;

pub fn create_shared_mcp_store() -> SharedMcpStore {
    Arc::new(RwLock::new(McpStore::new()))
}

static MCP_STORE: std::sync::OnceLock<SharedMcpStore> = std::sync::OnceLock::new();

pub fn get_mcp_store() -> SharedMcpStore {
    MCP_STORE.get_or_init(create_shared_mcp_store).clone()
}

#[derive(Debug, Clone)]
pub struct ListMcpResourcesTool;

impl ListMcpResourcesTool {
    pub fn new() -> Self {
        Self
    }
}

impl Default for ListMcpResourcesTool {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Deserialize)]
struct ListMcpResourcesInput {
    server_id: String,
}

#[async_trait]
impl Tool for ListMcpResourcesTool {
    fn name(&self) -> &str {
        "ListMcpResources"
    }

    fn description(&self) -> &str {
        "List available MCP resources for a server"
    }

    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "server_id": {
                    "type": "string",
                    "description": "MCP server ID"
                }
            },
            "required": ["server_id"]
        })
    }

    async fn execute(&self, input: serde_json::Value) -> Result<ToolOutput, ToolError> {
        let input: ListMcpResourcesInput = serde_json::from_value(input)
            .map_err(|e| ToolError {
                message: format!("Invalid input: {}", e),
                code: Some("invalid_input".to_string()),
            })?;

        let store = get_mcp_store();
        let store = store.read().await;
        let resources = store.get_resources(&input.server_id)
            .map(|r| r.iter().map(|res| {
                serde_json::json!({
                    "uri": res.uri,
                    "name": res.name,
                    "mime_type": res.mime_type,
                    "size": res.size,
                })
            }).collect::<Vec<_>>())
            .unwrap_or_default();

        Ok(ToolOutput {
            output_type: "json".to_string(),
            content: serde_json::to_string_pretty(&resources).unwrap(),
            metadata: HashMap::new(),
        })
    }
}

#[derive(Debug, Clone)]
pub struct ReadMcpResourceTool;

impl ReadMcpResourceTool {
    pub fn new() -> Self {
        Self
    }
}

impl Default for ReadMcpResourceTool {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Deserialize)]
struct ReadMcpResourceInput {
    server_id: String,
    uri: String,
}

#[async_trait]
impl Tool for ReadMcpResourceTool {
    fn name(&self) -> &str {
        "ReadMcpResource"
    }

    fn description(&self) -> &str {
        "Read content of an MCP resource"
    }

    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "server_id": {
                    "type": "string",
                    "description": "MCP server ID"
                },
                "uri": {
                    "type": "string",
                    "description": "Resource URI"
                }
            },
            "required": ["server_id", "uri"]
        })
    }

    async fn execute(&self, input: serde_json::Value) -> Result<ToolOutput, ToolError> {
        let input: ReadMcpResourceInput = serde_json::from_value(input)
            .map_err(|e| ToolError {
                message: format!("Invalid input: {}", e),
                code: Some("invalid_input".to_string()),
            })?;

        let store = get_mcp_store();
        let store = store.read().await;
        let resources = store.get_resources(&input.server_id)
            .ok_or_else(|| ToolError {
                message: format!("Server not found: {}", input.server_id),
                code: Some("not_found".to_string()),
            })?;

        let resource = resources.iter()
            .find(|r| r.uri == input.uri)
            .ok_or_else(|| ToolError {
                message: format!("Resource not found: {}", input.uri),
                code: Some("not_found".to_string()),
            })?;

        Ok(ToolOutput {
            output_type: "text".to_string(),
            content: format!("Resource: {}\nMIME: {:?}\nSize: {:?}",
                resource.name, resource.mime_type, resource.size),
            metadata: HashMap::new(),
        })
    }
}

#[derive(Debug, Clone)]
pub struct McpAuthTool;

impl McpAuthTool {
    pub fn new() -> Self {
        Self
    }
}

impl Default for McpAuthTool {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Deserialize)]
struct McpAuthInput {
    server_id: String,
    operation: String,
    #[serde(default)]
    token: Option<String>,
}

#[async_trait]
impl Tool for McpAuthTool {
    fn name(&self) -> &str {
        "McpAuth"
    }

    fn description(&self) -> &str {
        "Manage MCP server authentication. Operations: set_token, get_status"
    }

    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "server_id": {
                    "type": "string",
                    "description": "MCP server ID"
                },
                "operation": {
                    "type": "string",
                    "enum": ["set_token", "get_status", "clear_token"],
                    "description": "Operation to perform"
                },
                "token": {
                    "type": "string",
                    "description": "Auth token (for set_token)"
                }
            },
            "required": ["server_id", "operation"]
        })
    }

    async fn execute(&self, input: serde_json::Value) -> Result<ToolOutput, ToolError> {
        let input: McpAuthInput = serde_json::from_value(input)
            .map_err(|e| ToolError {
                message: format!("Invalid input: {}", e),
                code: Some("invalid_input".to_string()),
            })?;

        match input.operation.as_str() {
            "set_token" => {
                let token = input.token.ok_or_else(|| ToolError {
                    message: "Token is required for set_token operation".to_string(),
                    code: Some("missing_field".to_string()),
                })?;

                let store = get_mcp_store();
                let mut store = store.write().await;
                store.set_auth_token(&input.server_id, token);

                Ok(ToolOutput {
                    output_type: "json".to_string(),
                    content: serde_json::to_string_pretty(&serde_json::json!({
                        "success": true,
                        "message": "Token set successfully"
                    })).unwrap(),
                    metadata: HashMap::new(),
                })
            }
            "get_status" => {
                let store = get_mcp_store();
                let store = store.read().await;
                let server = store.get_server(&input.server_id)
                    .ok_or_else(|| ToolError {
                        message: format!("Server not found: {}", input.server_id),
                        code: Some("not_found".to_string()),
                    })?;

                Ok(ToolOutput {
                    output_type: "json".to_string(),
                    content: serde_json::to_string_pretty(&serde_json::json!({
                        "server_id": server.id,
                        "server_name": server.name,
                        "has_token": server.auth_token.is_some(),
                        "enabled": server.enabled,
                    })).unwrap(),
                    metadata: HashMap::new(),
                })
            }
            "clear_token" => {
                let store = get_mcp_store();
                let mut store = store.write().await;
                if let Some(server) = store.servers.get_mut(&input.server_id) {
                    server.auth_token = None;
                }

                Ok(ToolOutput {
                    output_type: "json".to_string(),
                    content: serde_json::to_string_pretty(&serde_json::json!({
                        "success": true,
                        "message": "Token cleared"
                    })).unwrap(),
                    metadata: HashMap::new(),
                })
            }
            _ => Err(ToolError {
                message: format!("Unknown operation: {}", input.operation),
                code: Some("unknown_operation".to_string()),
            }),
        }
    }
}

#[derive(Debug, Clone)]
pub struct RemoteTriggerTool;

impl RemoteTriggerTool {
    pub fn new() -> Self {
        Self
    }
}

impl Default for RemoteTriggerTool {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Deserialize)]
struct RemoteTriggerInput {
    trigger_id: String,
    #[serde(default)]
    payload: Option<serde_json::Value>,
}

#[async_trait]
impl Tool for RemoteTriggerTool {
    fn name(&self) -> &str {
        "RemoteTrigger"
    }

    fn description(&self) -> &str {
        "Trigger a remote event or webhook"
    }

    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "trigger_id": {
                    "type": "string",
                    "description": "Trigger ID to invoke"
                },
                "payload": {
                    "type": "object",
                    "description": "Optional payload to send with trigger"
                }
            },
            "required": ["trigger_id"]
        })
    }

    async fn execute(&self, input: serde_json::Value) -> Result<ToolOutput, ToolError> {
        let input: RemoteTriggerInput = serde_json::from_value(input)
            .map_err(|e| ToolError {
                message: format!("Invalid input: {}", e),
                code: Some("invalid_input".to_string()),
            })?;

        Ok(ToolOutput {
            output_type: "json".to_string(),
            content: serde_json::to_string_pretty(&serde_json::json!({
                "trigger_id": input.trigger_id,
                "status": "triggered",
                "payload": input.payload,
            })).unwrap(),
            metadata: HashMap::new(),
        })
    }
}

#[derive(Debug, Clone)]
pub struct SyntheticOutputTool;

impl SyntheticOutputTool {
    pub fn new() -> Self {
        Self
    }
}

impl Default for SyntheticOutputTool {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Deserialize)]
struct SyntheticOutputInput {
    content: String,
    #[serde(default = "default_synthetic_type")]
    output_type: String,
    #[serde(default)]
    metadata: Option<HashMap<String, serde_json::Value>>,
}

fn default_synthetic_type() -> String {
    "text".to_string()
}

#[async_trait]
impl Tool for SyntheticOutputTool {
    fn name(&self) -> &str {
        "SyntheticOutput"
    }

    fn description(&self) -> &str {
        "Generate synthetic output for testing or simulation"
    }

    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "content": {
                    "type": "string",
                    "description": "Output content"
                },
                "output_type": {
                    "type": "string",
                    "enum": ["text", "json", "error"],
                    "description": "Output type"
                },
                "metadata": {
                    "type": "object",
                    "description": "Optional metadata"
                }
            },
            "required": ["content"]
        })
    }

    async fn execute(&self, input: serde_json::Value) -> Result<ToolOutput, ToolError> {
        let input: SyntheticOutputInput = serde_json::from_value(input)
            .map_err(|e| ToolError {
                message: format!("Invalid input: {}", e),
                code: Some("invalid_input".to_string()),
            })?;

        let mut metadata = input.metadata.unwrap_or_default();
        metadata.insert("synthetic".to_string(), serde_json::json!(true));

        Ok(ToolOutput {
            output_type: input.output_type,
            content: input.content,
            metadata,
        })
    }
}