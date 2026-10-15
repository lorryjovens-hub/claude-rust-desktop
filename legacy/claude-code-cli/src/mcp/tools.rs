//! MCP Tools - Tool registration and execution

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use async_trait::async_trait;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpTool {
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
    pub server_name: Option<String>,
}

impl McpTool {
    pub fn new(name: &str, description: &str, input_schema: serde_json::Value) -> Self {
        Self {
            name: name.to_string(),
            description: description.to_string(),
            input_schema,
            server_name: None,
        }
    }
    
    pub fn with_server(mut self, server_name: &str) -> Self {
        self.server_name = Some(server_name.to_string());
        self
    }
}

#[async_trait]
pub trait ToolExecutor: Send + Sync {
    async fn execute(&self, params: serde_json::Value) -> anyhow::Result<serde_json::Value>;
}

pub type ToolExecutorFn = Box<dyn Fn(serde_json::Value) -> std::pin::Pin<Box<dyn std::future::Future<Output = anyhow::Result<serde_json::Value>> + Send>> + Send + Sync>;

pub struct ToolRegistry {
    tools: Arc<RwLock<HashMap<String, McpTool>>>,
    executors: Arc<RwLock<HashMap<String, Arc<dyn ToolExecutor>>>>,
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self {
            tools: Arc::new(RwLock::new(HashMap::new())),
            executors: Arc::new(RwLock::new(HashMap::new())),
        }
    }
    
    pub async fn register(&self, tool: McpTool, executor: Arc<dyn ToolExecutor>) {
        let name = tool.name.clone();
        let mut tools = self.tools.write().await;
        let mut executors = self.executors.write().await;
        tools.insert(name.clone(), tool);
        executors.insert(name, executor);
    }
    
    pub async fn unregister(&self, name: &str) {
        let mut tools = self.tools.write().await;
        let mut executors = self.executors.write().await;
        tools.remove(name);
        executors.remove(name);
    }
    
    pub async fn get(&self, name: &str) -> Option<McpTool> {
        let tools = self.tools.read().await;
        tools.get(name).cloned()
    }
    
    pub async fn list(&self) -> Vec<McpTool> {
        let tools = self.tools.read().await;
        tools.values().cloned().collect()
    }
    
    pub async fn execute(&self, name: &str, params: serde_json::Value) -> anyhow::Result<serde_json::Value> {
        let executors = self.executors.read().await;
        let executor = executors.get(name)
            .ok_or_else(|| anyhow::anyhow!("Tool not found: {}", name))?;
        executor.execute(params).await
    }
    
    pub async fn register_builtin_tools(&self) {
        self.register(
            McpTool::new(
                "file_read",
                "Read file contents",
                serde_json::json!({
                    "type": "object",
                    "properties": {
                        "path": {"type": "string", "description": "File path to read"}
                    },
                    "required": ["path"]
                })
            ),
            Arc::new(BuiltinFileReadExecutor),
        ).await;
        
        self.register(
            McpTool::new(
                "file_write",
                "Write content to a file",
                serde_json::json!({
                    "type": "object",
                    "properties": {
                        "path": {"type": "string", "description": "File path to write"},
                        "content": {"type": "string", "description": "Content to write"}
                    },
                    "required": ["path", "content"]
                })
            ),
            Arc::new(BuiltinFileWriteExecutor),
        ).await;
        
        self.register(
            McpTool::new(
                "execute_command",
                "Execute a shell command",
                serde_json::json!({
                    "type": "object",
                    "properties": {
                        "command": {"type": "string", "description": "Command to execute"},
                        "cwd": {"type": "string", "description": "Working directory"}
                    },
                    "required": ["command"]
                })
            ),
            Arc::new(BuiltinCommandExecutor),
        ).await;
        
        self.register(
            McpTool::new(
                "search",
                "Search for pattern in files",
                serde_json::json!({
                    "type": "object",
                    "properties": {
                        "pattern": {"type": "string", "description": "Search pattern"},
                        "path": {"type": "string", "description": "Directory to search"}
                    },
                    "required": ["pattern"]
                })
            ),
            Arc::new(BuiltinSearchExecutor),
        ).await;

        self.register(
            McpTool::new(
                "git_operations",
                "Execute Git version control operations: status, add, commit, push, pull, log, diff, branch, checkout",
                serde_json::json!({
                    "type": "object",
                    "properties": {
                        "operation": {
                            "type": "string",
                            "enum": ["status", "add", "commit", "push", "pull", "log", "diff", "branch", "checkout"],
                            "description": "Git operation to perform"
                        },
                        "path": {"type": "string", "description": "Path to the git repository"},
                        "message": {"type": "string", "description": "Commit message (for commit)"},
                        "files": {"type": "array", "items": {"type": "string"}, "description": "Files to add (for add)"},
                        "branch": {"type": "string", "description": "Branch name"}
                    },
                    "required": ["operation"]
                })
            ),
            Arc::new(BuiltinGitExecutor),
        ).await;

        self.register(
            McpTool::new(
                "smart_edit",
                "Advanced code editing with fuzzy matching, multi-line replacements, and diff preview",
                serde_json::json!({
                    "type": "object",
                    "properties": {
                        "operation": {"type": "string", "enum": ["replace", "insert", "delete", "preview"]},
                        "file_path": {"type": "string"},
                        "old_content": {"type": "string"},
                        "new_content": {"type": "string"},
                        "line_number": {"type": "integer"},
                        "start_line": {"type": "integer"},
                        "end_line": {"type": "integer"}
                    },
                    "required": ["operation", "file_path"]
                })
            ),
            Arc::new(BuiltinSmartEditExecutor),
        ).await;

        self.register(
            McpTool::new(
                "list_files",
                "List files in a directory",
                serde_json::json!({
                    "type": "object",
                    "properties": {
                        "path": {"type": "string", "description": "Directory path"},
                        "recursive": {"type": "boolean", "description": "List recursively"}
                    },
                    "required": ["path"]
                })
            ),
            Arc::new(BuiltinListFilesExecutor),
        ).await;
    }
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}

struct BuiltinFileReadExecutor;

#[async_trait]
impl ToolExecutor for BuiltinFileReadExecutor {
    async fn execute(&self, params: serde_json::Value) -> anyhow::Result<serde_json::Value> {
        let path = params["path"].as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing path parameter"))?;
        
        let content = tokio::fs::read_to_string(path).await
            .map_err(|e| anyhow::anyhow!("Failed to read file: {}", e))?;
        
        Ok(serde_json::json!({
            "success": true,
            "content": content
        }))
    }
}

struct BuiltinFileWriteExecutor;

#[async_trait]
impl ToolExecutor for BuiltinFileWriteExecutor {
    async fn execute(&self, params: serde_json::Value) -> anyhow::Result<serde_json::Value> {
        let path = params["path"].as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing path parameter"))?;
        let content = params["content"].as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing content parameter"))?;
        
        if let Some(parent) = std::path::Path::new(path).parent() {
            tokio::fs::create_dir_all(parent).await
                .map_err(|e| anyhow::anyhow!("Failed to create directory: {}", e))?;
        }
        
        tokio::fs::write(path, content).await
            .map_err(|e| anyhow::anyhow!("Failed to write file: {}", e))?;
        
        Ok(serde_json::json!({
            "success": true,
            "path": path
        }))
    }
}

struct BuiltinCommandExecutor;

#[async_trait]
impl ToolExecutor for BuiltinCommandExecutor {
    async fn execute(&self, params: serde_json::Value) -> anyhow::Result<serde_json::Value> {
        let command = params["command"].as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing command parameter"))?;
        let cwd = params["cwd"].as_str();
        
        let output = if let Some(dir) = cwd {
            tokio::process::Command::new("sh")
                .arg("-c")
                .arg(command)
                .current_dir(dir)
                .output()
                .await
        } else {
            tokio::process::Command::new("sh")
                .arg("-c")
                .arg(command)
                .output()
                .await
        };
        
        match output {
            Ok(output) => {
                let stdout = String::from_utf8_lossy(&output.stdout).to_string();
                let stderr = String::from_utf8_lossy(&output.stderr).to_string();
                Ok(serde_json::json!({
                    "success": output.status.success(),
                    "stdout": stdout,
                    "stderr": stderr,
                    "exit_code": output.status.code()
                }))
            }
            Err(e) => Ok(serde_json::json!({
                "success": false,
                "error": e.to_string()
            }))
        }
    }
}

struct BuiltinSearchExecutor;

#[async_trait]
impl ToolExecutor for BuiltinSearchExecutor {
    async fn execute(&self, params: serde_json::Value) -> anyhow::Result<serde_json::Value> {
        let pattern = params["pattern"].as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing pattern parameter"))?;
        let path = params["path"].as_str().unwrap_or(".");

        let output = tokio::process::Command::new("rg")
            .arg("-l")
            .arg(pattern)
            .arg(path)
            .output()
            .await;

        match output {
            Ok(output) => {
                let files = String::from_utf8_lossy(&output.stdout)
                    .lines()
                    .map(|s| s.to_string())
                    .collect::<Vec<_>>();
                Ok(serde_json::json!({
                    "success": true,
                    "files": files
                }))
            }
            Err(e) => Ok(serde_json::json!({
                "success": false,
                "error": e.to_string()
            }))
        }
    }
}

struct BuiltinGitExecutor;

#[async_trait]
impl ToolExecutor for BuiltinGitExecutor {
    async fn execute(&self, params: serde_json::Value) -> anyhow::Result<serde_json::Value> {
        let operation = params["operation"].as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing operation parameter"))?;
        let path = params["path"].as_str().unwrap_or(".");

        let mut args = vec![operation.to_string()];

        match operation {
            "commit" => {
                if let Some(msg) = params["message"].as_str() {
                    args.push("-m".to_string());
                    args.push(msg.to_string());
                }
            }
            "add" => {
                if let Some(files) = params["files"].as_array() {
                    for file in files {
                        if let Some(f) = file.as_str() {
                            args.push(f.to_string());
                        }
                    }
                }
            }
            "checkout" | "branch" => {
                if let Some(branch) = params["branch"].as_str() {
                    args.push(branch.to_string());
                }
            }
            _ => {}
        }

        let output = tokio::process::Command::new("git")
            .current_dir(path)
            .args(&args)
            .output()
            .await;

        match output {
            Ok(output) => {
                let stdout = String::from_utf8_lossy(&output.stdout).to_string();
                let stderr = String::from_utf8_lossy(&output.stderr).to_string();
                Ok(serde_json::json!({
                    "success": output.status.success(),
                    "stdout": stdout,
                    "stderr": stderr
                }))
            }
            Err(e) => Ok(serde_json::json!({
                "success": false,
                "error": e.to_string()
            }))
        }
    }
}

struct BuiltinSmartEditExecutor;

#[async_trait]
impl ToolExecutor for BuiltinSmartEditExecutor {
    async fn execute(&self, params: serde_json::Value) -> anyhow::Result<serde_json::Value> {
        let operation = params["operation"].as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing operation parameter"))?;
        let file_path = params["file_path"].as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing file_path parameter"))?;

        match operation {
            "replace" => {
                let old_content = params["old_content"].as_str()
                    .ok_or_else(|| anyhow::anyhow!("Missing old_content"))?;
                let new_content = params["new_content"].as_str()
                    .ok_or_else(|| anyhow::anyhow!("Missing new_content"))?;

                let content = tokio::fs::read_to_string(file_path).await
                    .map_err(|e| anyhow::anyhow!("Failed to read file: {}", e))?;

                if !content.contains(old_content) {
                    return Ok(serde_json::json!({
                        "success": false,
                        "error": "old_content not found in file"
                    }));
                }

                let new_file_content = content.replace(old_content, new_content);
                tokio::fs::write(file_path, new_file_content).await
                    .map_err(|e| anyhow::anyhow!("Failed to write file: {}", e))?;

                Ok(serde_json::json!({
                    "success": true,
                    "message": format!("Successfully edited {}", file_path)
                }))
            }
            "preview" => {
                let old_content = params["old_content"].as_str()
                    .ok_or_else(|| anyhow::anyhow!("Missing old_content"))?;
                let new_content = params["new_content"].as_str()
                    .ok_or_else(|| anyhow::anyhow!("Missing new_content"))?;

                let file_content = tokio::fs::read_to_string(file_path).await
                    .map_err(|e| anyhow::anyhow!("Failed to read file: {}", e))?;

                let can_apply = file_content.contains(old_content);

                Ok(serde_json::json!({
                    "success": true,
                    "can_apply": can_apply,
                    "preview": {
                        "old": old_content,
                        "new": new_content
                    }
                }))
            }
            _ => Ok(serde_json::json!({
                "success": false,
                "error": format!("Operation '{}' not fully implemented", operation)
            }))
        }
    }
}

struct BuiltinListFilesExecutor;

#[async_trait]
impl ToolExecutor for BuiltinListFilesExecutor {
    async fn execute(&self, params: serde_json::Value) -> anyhow::Result<serde_json::Value> {
        let path = params["path"].as_str().unwrap_or(".");
        let recursive = params["recursive"].as_bool().unwrap_or(false);

        let mut entries = Vec::new();

        if recursive {
            let mut dir = tokio::fs::read_dir(path).await
                .map_err(|e| anyhow::anyhow!("Failed to read directory: {}", e))?;

            while let Some(entry) = dir.next_entry().await? {
                let path = entry.path();
                let metadata = entry.metadata().await?;
                entries.push(serde_json::json!({
                    "path": path.to_string_lossy().to_string(),
                    "is_file": metadata.is_file(),
                    "is_dir": metadata.is_dir()
                }));
            }
        } else {
            let mut dir = tokio::fs::read_dir(path).await
                .map_err(|e| anyhow::anyhow!("Failed to read directory: {}", e))?;

            while let Some(entry) = dir.next_entry().await? {
                let file_name = entry.file_name().to_string_lossy().to_string();
                let metadata = entry.metadata().await?;
                entries.push(serde_json::json!({
                    "name": file_name,
                    "is_file": metadata.is_file(),
                    "is_dir": metadata.is_dir()
                }));
            }
        }

        Ok(serde_json::json!({
            "success": true,
            "entries": entries
        }))
    }
}
