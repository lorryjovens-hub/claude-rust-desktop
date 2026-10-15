//! Sandbox Module
//!
//! Provides sandboxed execution environment with restricted file system access.

use crate::tools::{Tool, ToolOutput, ToolError};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxConfig {
    pub enabled: bool,
    pub allowed_paths: Vec<String>,
    pub blocked_paths: Vec<String>,
    pub workspace_path: Option<String>,
    pub max_file_size_bytes: u64,
    pub allow_network: bool,
    pub allow_env_access: bool,
}

impl Default for SandboxConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            allowed_paths: Vec::new(),
            blocked_paths: vec![
                "/etc".to_string(),
                "/usr".to_string(),
                "/bin".to_string(),
                "/sbin".to_string(),
                "/var".to_string(),
                "/root".to_string(),
                "/home".to_string(),
                "C:\\Windows".to_string(),
                "C:\\Program Files".to_string(),
                "C:\\ProgramData".to_string(),
            ],
            workspace_path: None,
            max_file_size_bytes: 10 * 1024 * 1024,
            allow_network: false,
            allow_env_access: false,
        }
    }
}

pub struct SandboxTool {
    config: SandboxConfig,
}

impl Default for SandboxTool {
    fn default() -> Self {
        Self::new()
    }
}

impl SandboxTool {
    pub fn new() -> Self {
        Self {
            config: SandboxConfig::default(),
        }
    }

    fn is_path_allowed(&self, path: &str) -> Result<bool, ToolError> {
        let path = Path::new(path);
        
        for blocked in &self.config.blocked_paths {
            if path.starts_with(blocked) {
                return Err(ToolError {
                    message: format!("Path {} is blocked", path.display()),
                    code: Some("path_blocked".to_string()),
                });
            }
        }

        if let Some(workspace) = &self.config.workspace_path {
            if !path.starts_with(workspace) {
                return Err(ToolError {
                    message: format!("Path {} is outside workspace", path.display()),
                    code: Some("path_outside_workspace".to_string()),
                });
            }
        }

        for allowed in &self.config.allowed_paths {
            if path.starts_with(allowed) {
                return Ok(true);
            }
        }

        if self.config.workspace_path.is_none() {
            return Ok(true);
        }

        Err(ToolError {
            message: format!("Path {} is not in allowed paths", path.display()),
            code: Some("path_not_allowed".to_string()),
        })
    }

    fn check_file_size(&self, path: &Path) -> Result<bool, ToolError> {
        match std::fs::metadata(path) {
            Ok(metadata) => {
                if metadata.len() > self.config.max_file_size_bytes {
                    Err(ToolError {
                        message: format!(
                            "File size {} exceeds maximum allowed size {}",
                            metadata.len(),
                            self.config.max_file_size_bytes
                        ),
                        code: Some("file_too_large".to_string()),
                    })
                } else {
                    Ok(true)
                }
            }
            Err(_) => Ok(true),
        }
    }

    async fn handle_check_path(&self, path: &str) -> Result<String, ToolError> {
        self.is_path_allowed(path)?;
        Ok(format!("Path {} is allowed", path))
    }

    async fn handle_set_config(&mut self, config: &SandboxConfig) -> Result<String, ToolError> {
        self.config = config.clone();
        Ok("Sandbox configuration updated".to_string())
    }

    async fn handle_get_config(&self) -> Result<SandboxConfig, ToolError> {
        Ok(self.config.clone())
    }

    async fn handle_validate_file_operation(
        &self,
        path: &str,
        operation: &str,
    ) -> Result<String, ToolError> {
        self.is_path_allowed(path)?;

        if operation == "read" || operation == "write" {
            self.check_file_size(Path::new(path))?;
        }

        Ok(format!("File operation '{}' on '{}' is allowed", operation, path))
    }
}

#[async_trait]
impl Tool for SandboxTool {
    fn name(&self) -> &str {
        "sandbox"
    }

    fn description(&self) -> &str {
        "Sandbox execution environment with restricted file system access"
    }

    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "operation": {
                    "type": "string",
                    "enum": ["check_path", "set_config", "get_config", "validate_file_operation"]
                },
                "path": {
                    "type": "string",
                    "description": "Path to check or validate"
                },
                "operation_type": {
                    "type": "string",
                    "enum": ["read", "write", "execute", "delete"],
                    "description": "File operation type"
                },
                "config": {
                    "type": "object",
                    "properties": {
                        "enabled": { "type": "boolean" },
                        "allowed_paths": { "type": "array", "items": { "type": "string" } },
                        "blocked_paths": { "type": "array", "items": { "type": "string" } },
                        "workspace_path": { "type": "string" },
                        "max_file_size_bytes": { "type": "number" },
                        "allow_network": { "type": "boolean" },
                        "allow_env_access": { "type": "boolean" }
                    }
                }
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
            "check_path" => {
                let path = input["path"].as_str()
                    .ok_or_else(|| ToolError {
                        message: "path is required".to_string(),
                        code: Some("missing_path".to_string()),
                    })?;
                let result = self.handle_check_path(path).await?;
                serde_json::json!({
                    "success": true,
                    "result": result
                }).to_string()
            }
            "set_config" => {
                let config: SandboxConfig = serde_json::from_value(input["config"].clone())
                    .map_err(|_| ToolError {
                        message: "Invalid config format".to_string(),
                        code: Some("invalid_config".to_string()),
                    })?;
                let mut tool = Self { config: self.config.clone() };
                let result = tool.handle_set_config(&config).await?;
                serde_json::json!({
                    "success": true,
                    "result": result
                }).to_string()
            }
            "get_config" => {
                let config = self.handle_get_config().await?;
                serde_json::json!({
                    "success": true,
                    "config": config
                }).to_string()
            }
            "validate_file_operation" => {
                let path = input["path"].as_str()
                    .ok_or_else(|| ToolError {
                        message: "path is required".to_string(),
                        code: Some("missing_path".to_string()),
                    })?;
                let operation_type = input["operation_type"].as_str()
                    .ok_or_else(|| ToolError {
                        message: "operation_type is required".to_string(),
                        code: Some("missing_operation_type".to_string()),
                    })?;
                let result = self.handle_validate_file_operation(path, operation_type).await?;
                serde_json::json!({
                    "success": true,
                    "result": result
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