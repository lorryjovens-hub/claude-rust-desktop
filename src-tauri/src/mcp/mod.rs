use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::process::{Child, Command, Stdio};
use std::sync::Arc;
use tokio::sync::{Mutex, RwLock};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerConfig {
    pub id: String,
    pub name: String,
    pub command: String,
    pub args: Vec<String>,
    pub env: HashMap<String, String>,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerStatus {
    pub id: String,
    pub running: bool,
    pub pid: Option<u32>,
    pub error: Option<String>,
}

pub struct McpServerManager {
    configs: Arc<RwLock<HashMap<String, McpServerConfig>>>,
    processes: Arc<Mutex<HashMap<String, Child>>>,
}

impl McpServerManager {
    pub fn new() -> Self {
        Self {
            configs: Arc::new(RwLock::new(HashMap::new())),
            processes: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub async fn add_server(&self, config: McpServerConfig) -> Result<McpServerConfig> {
        let mut configs = self.configs.write().await;
        configs.insert(config.id.clone(), config.clone());
        Ok(config)
    }

    pub async fn remove_server(&self, id: &str) -> Result<()> {
        self.stop_server(id).await?;
        let mut configs = self.configs.write().await;
        configs.remove(id);
        Ok(())
    }

    pub async fn update_server(&self, id: &str, updates: McpServerConfig) -> Result<McpServerConfig> {
        let mut configs = self.configs.write().await;
        if let Some(config) = configs.get_mut(id) {
            config.name = updates.name;
            config.command = updates.command;
            config.args = updates.args;
            config.env = updates.env;
            config.enabled = updates.enabled;
            Ok(config.clone())
        } else {
            anyhow::bail!("Server not found: {}", id)
        }
    }

    pub async fn list_servers(&self) -> Vec<McpServerConfig> {
        let configs = self.configs.read().await;
        configs.values().cloned().collect()
    }

    pub async fn get_server_status(&self) -> Vec<McpServerStatus> {
        let configs = self.configs.read().await;
        let mut processes = self.processes.lock().await;

        configs
            .values()
            .map(|config| {
                let running = processes.get_mut(&config.id).map(|p| {
                    match p.try_wait() {
                        Ok(Some(_)) => false,
                        Ok(None) => true,
                        Err(_) => false,
                    }
                }).unwrap_or(false);

                McpServerStatus {
                    id: config.id.clone(),
                    running,
                    pid: processes.get(&config.id).map(|p| p.id()),
                    error: None,
                }
            })
            .collect()
    }

    pub async fn start_server(&self, id: &str) -> Result<McpServerStatus> {
        let config = {
            let configs = self.configs.read().await;
            configs.get(id).cloned()
        };

        let config = match config {
            Some(c) => c,
            None => anyhow::bail!("Server not found: {}", id),
        };

        if !config.enabled {
            anyhow::bail!("Server is disabled: {}", id);
        }

        {
            let mut processes = self.processes.lock().await;
            if let Some(child) = processes.get_mut(id) {
                if let Ok(Some(_)) = child.try_wait() {
                    processes.remove(id);
                } else {
                    return Ok(McpServerStatus {
                        id: id.to_string(),
                        running: true,
                        pid: Some(child.id()),
                        error: None,
                    });
                }
            }
        }

        let mut cmd = Command::new(&config.command);
        cmd.args(&config.args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        for (key, value) in &config.env {
            cmd.env(key, value);
        }

        let child = cmd.spawn()?;

        let mut processes = self.processes.lock().await;
        let child_id = child.id();
        processes.insert(id.to_string(), child);

        Ok(McpServerStatus {
            id: id.to_string(),
            running: true,
            pid: Some(child_id),
            error: None,
        })
    }

    pub async fn stop_server(&self, id: &str) -> Result<()> {
        let mut processes = self.processes.lock().await;
        if let Some(mut child) = processes.remove(id) {
            child.kill()?;
        }
        Ok(())
    }

    pub async fn stop_all(&self) -> Result<()> {
        let mut processes = self.processes.lock().await;
        for (_id, mut child) in processes.drain() {
            let _ = child.kill();
        }
        Ok(())
    }

    pub async fn get_default_servers() -> Vec<McpServerConfig> {
        vec![
            McpServerConfig {
                id: "filesystem".to_string(),
                name: "Filesystem".to_string(),
                command: "npx".to_string(),
                args: vec![
                    "-y".to_string(),
                    "@anthropic/mcp-server-filesystem".to_string(),
                    "/tmp".to_string(),
                ],
                env: HashMap::new(),
                enabled: false,
            },
            McpServerConfig {
                id: "git".to_string(),
                name: "Git".to_string(),
                command: "npx".to_string(),
                args: vec![
                    "-y".to_string(),
                    "@anthropic/mcp-server-git".to_string(),
                ],
                env: HashMap::new(),
                enabled: false,
            },
            McpServerConfig {
                id: "brave-search".to_string(),
                name: "Brave Search".to_string(),
                command: "npx".to_string(),
                args: vec![
                    "-y".to_string(),
                    "@anthropic/mcp-server-brave-search".to_string(),
                ],
                env: HashMap::new(),
                enabled: false,
            },
        ]
    }
}

impl Default for McpServerManager {
    fn default() -> Self {
        Self::new()
    }
}