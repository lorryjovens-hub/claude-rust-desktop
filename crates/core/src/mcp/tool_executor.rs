use crate::mcp::{McpConnector, McpServerManager};
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpToolDefinition {
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
    pub server_name: String,
}

pub struct McpToolRegistry {
    manager: Arc<McpServerManager>,
    #[allow(dead_code)]
    active_connectors: Arc<Mutex<HashMap<String, Arc<Mutex<McpConnector>>>>>,
}

impl McpToolRegistry {
    pub fn new(manager: Arc<McpServerManager>) -> Self {
        Self {
            manager,
            active_connectors: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub async fn get_all_tools(&self) -> Result<Vec<McpToolDefinition>> {
        let manager = &self.manager;
        let servers = manager.list_servers_full().await;
        let mut all_tools = vec![
            McpToolDefinition {
                name: "ListMcpResources".into(),
                description:
                    "List live MCP resources from an enabled running server; server_id is required."
                        .into(),
                input_schema: serde_json::json!({"type":"object","properties":{"server_id":{"type":"string"}},"required":["server_id"]}),
                server_name: "MCP resources".into(),
            },
            McpToolDefinition {
                name: "ReadMcpResource".into(),
                description: "Read actual MCP resource contents from an enabled running server."
                    .into(),
                input_schema: serde_json::json!({"type":"object","properties":{"server_id":{"type":"string"},"uri":{"type":"string"}},"required":["server_id","uri"]}),
                server_name: "MCP resources".into(),
            },
        ];

        for server in servers {
            if !server.config.enabled || !server.status.running {
                continue;
            }

            if let Some(connector_arc) = &server.connector {
                let connector = connector_arc.lock().await;
                let tools = connector.list_tools().await.unwrap_or_default();

                for tool in tools {
                    all_tools.push(McpToolDefinition {
                        name: tool.name.clone(),
                        description: tool.description.clone(),
                        input_schema: tool.input_schema.clone(),
                        server_name: server.config.name.clone(),
                    });
                }
            }
        }

        Ok(all_tools)
    }

    pub async fn execute_tool(
        &self,
        name: &str,
        arguments: serde_json::Value,
    ) -> Result<serde_json::Value> {
        let manager = &self.manager;
        let servers = manager.list_servers_full().await;

        if matches!(name, "ListMcpResources" | "ReadMcpResource") {
            let id = arguments["server_id"]
                .as_str()
                .ok_or_else(|| anyhow::anyhow!("server_id is required"))?;
            let server = servers
                .iter()
                .find(|server| server.config.id == id)
                .ok_or_else(|| anyhow::anyhow!("MCP server not found: {id}"))?;
            anyhow::ensure!(
                server.config.enabled && server.status.running,
                "MCP server is disabled or stopped"
            );
            let connector = server
                .connector
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("MCP connector unavailable"))?
                .lock()
                .await;
            return if name == "ListMcpResources" {
                Ok(serde_json::to_value(connector.list_resources().await?)?)
            } else {
                let uri = arguments["uri"]
                    .as_str()
                    .ok_or_else(|| anyhow::anyhow!("uri is required"))?;
                Ok(serde_json::to_value(
                    connector.read_resource(uri, None).await?,
                )?)
            };
        }

        for server in servers {
            if !server.config.enabled || !server.status.running {
                continue;
            }

            if let Some(connector_arc) = &server.connector {
                let connector = connector_arc.lock().await;
                let tools = connector.list_tools().await.unwrap_or_default();

                if tools.iter().any(|t| t.name == name) {
                    return connector.call_tool(name, arguments).await;
                }
            }
        }

        Err(anyhow::anyhow!(
            "MCP tool '{}' not found in any running server",
            name
        ))
    }

    pub async fn is_mcp_tool(&self, name: &str) -> bool {
        if matches!(name, "ListMcpResources" | "ReadMcpResource") {
            return true;
        }
        match self.get_all_tools().await {
            Ok(tools) => tools.iter().any(|t| t.name == name),
            Err(_) => false,
        }
    }
}
