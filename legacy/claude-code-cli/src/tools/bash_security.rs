//! Bash Security Module
//!
//! Provides permission checking and security validation for bash commands.
//! Based on claw-code-main's bashPermissions, bashSecurity, pathValidation, etc.

use crate::tools::{Tool, ToolOutput, ToolError};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BashSecurityContext {
    pub mode: PermissionMode,
    pub workspace_path: Option<PathBuf>,
    pub is_read_only: bool,
    pub allowed_paths: Vec<String>,
    pub blocked_commands: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum PermissionMode {
    Normal,
    ReadOnly,
    WorkspaceWrite,
    DangerFullAccess,
}

impl std::str::FromStr for PermissionMode {
    type Err = ();
    
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "normal" => Ok(PermissionMode::Normal),
            "read_only" => Ok(PermissionMode::ReadOnly),
            "workspace_write" => Ok(PermissionMode::WorkspaceWrite),
            "danger_full_access" => Ok(PermissionMode::DangerFullAccess),
            _ => Err(()),
        }
    }
}

pub struct BashSecurityTool {
    context: BashSecurityContext,
}

impl Default for BashSecurityTool {
    fn default() -> Self {
        Self::new()
    }
}

impl BashSecurityTool {
    pub fn new() -> Self {
        Self {
            context: BashSecurityContext {
                mode: PermissionMode::Normal,
                workspace_path: None,
                is_read_only: false,
                allowed_paths: Vec::new(),
                blocked_commands: Vec::new(),
            }
        }
    }

    fn is_destructive_command(&self, command: &str) -> bool {
        let destructive_patterns = [
            r"(?i)\brm\b.*\b-rf?\b",
            r"(?i)\brm\b.*\b--recursive\b",
            r"(?i)\brm\b.*\b--force\b",
            r"(?i)\bdd\b.*\bif=\b",
            r"(?i)\bshred\b",
            r"(?i)\bwipe\b",
            r"(?i)\bformat\b",
            r"(?i)\bmkfs\b",
            r"(?i)\bdiskpart\b",
            r"(?i)\bchmod\b.*\b777\b",
            r"(?i)\bchown\b.*\broot\b",
            r"(?i)\bkill\b.*\b-9\b.*\b1\b",
            r"(?i)\bkillall\b",
            r"(?i)\breboot\b",
            r"(?i)\bshutdown\b",
            r"(?i)\bpoweroff\b",
            r"(?i)\bsu\b",
            r"(?i)\bsudo\b",
            r"(?i)\bpasswd\b",
        ];

        for pattern in &destructive_patterns {
            if regex::Regex::new(pattern).map(|re| re.is_match(command)).unwrap_or(false) {
                return true;
            }
        }
        false
    }

    fn validate_path(&self, path: &str) -> Result<bool, ToolError> {
        let path = Path::new(path);
        
        if let Some(workspace) = &self.context.workspace_path {
            if !path.starts_with(workspace) {
                return Err(ToolError {
                    message: format!("Path {} is outside workspace {}", path.display(), workspace.display()),
                    code: Some("path_outside_workspace".to_string()),
                });
            }
        }

        Ok(true)
    }

    fn is_path_safe(&self, path: &str) -> bool {
        let unsafe_paths = [
            "/etc",
            "/usr",
            "/bin",
            "/sbin",
            "/var",
            "/root",
            "C:\\Windows",
            "C:\\Program Files",
            "/home",
            "~",
        ];

        let path_lower = path.to_lowercase();
        for unsafe_path in unsafe_paths {
            if path_lower.starts_with(&unsafe_path.to_lowercase()) {
                return false;
            }
        }
        true
    }

    async fn handle_check_permission(&self, input: &serde_json::Value) -> Result<String, ToolError> {
        let command = input["command"].as_str()
            .ok_or_else(|| ToolError {
                message: "command is required".to_string(),
                code: Some("missing_command".to_string()),
            })?;

        let path = input["path"].as_str().unwrap_or("");

        let mut warnings = Vec::new();

        if self.context.is_read_only {
            if command.contains("write") || command.contains(">") || command.contains("|") {
                warnings.push("Command may modify files in read-only mode".to_string());
            }
        }

        if self.is_destructive_command(command) {
            warnings.push("Command appears to be destructive".to_string());
        }

        if !path.is_empty() {
            if !self.is_path_safe(path) {
                warnings.push(format!("Path {} may be unsafe", path));
            }

            match self.validate_path(path) {
                Ok(_) => {}
                Err(e) => warnings.push(e.message),
            }
        }

        if warnings.is_empty() {
            Ok("Permission granted - no security concerns detected".to_string())
        } else {
            Ok(format!("Warnings: {}", warnings.join("; ")))
        }
    }

    async fn handle_set_mode(&mut self, mode: &str) -> Result<String, ToolError> {
        self.context.mode = mode.parse()
            .map_err(|_| ToolError {
                message: "Invalid mode. Must be: normal, read_only, workspace_write, danger_full_access".to_string(),
                code: Some("invalid_mode".to_string()),
            })?;

        self.context.is_read_only = self.context.mode == PermissionMode::ReadOnly;

        Ok(format!("Permission mode set to: {:?}", self.context.mode))
    }

    async fn handle_validate_path(&self, path: &str) -> Result<String, ToolError> {
        if !self.is_path_safe(path) {
            return Err(ToolError {
                message: format!("Path {} is not safe", path),
                code: Some("unsafe_path".to_string()),
            });
        }

        match self.validate_path(path) {
            Ok(_) => Ok(format!("Path {} is valid", path)),
            Err(e) => Err(e),
        }
    }
}

#[async_trait]
impl Tool for BashSecurityTool {
    fn name(&self) -> &str {
        "bash_security"
    }

    fn description(&self) -> &str {
        "Bash command security validation and permission checking"
    }

    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "operation": {
                    "type": "string",
                    "enum": ["check_permission", "set_mode", "validate_path", "is_destructive"]
                },
                "command": {
                    "type": "string",
                    "description": "Command to check"
                },
                "path": {
                    "type": "string",
                    "description": "Path to validate"
                },
                "mode": {
                    "type": "string",
                    "enum": ["normal", "read_only", "workspace_write", "danger_full_access"]
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
            "check_permission" => {
                let result = self.handle_check_permission(&input).await?;
                serde_json::json!({
                    "success": true,
                    "result": result
                }).to_string()
            }
            "set_mode" => {
                let mode = input["mode"].as_str()
                    .ok_or_else(|| ToolError {
                        message: "mode is required".to_string(),
                        code: Some("missing_mode".to_string()),
                    })?;
                let mut tool = Self { context: self.context.clone() };
                let result = tool.handle_set_mode(mode).await?;
                serde_json::json!({
                    "success": true,
                    "result": result
                }).to_string()
            }
            "validate_path" => {
                let path = input["path"].as_str()
                    .ok_or_else(|| ToolError {
                        message: "path is required".to_string(),
                        code: Some("missing_path".to_string()),
                    })?;
                let result = self.handle_validate_path(path).await?;
                serde_json::json!({
                    "success": true,
                    "result": result
                }).to_string()
            }
            "is_destructive" => {
                let command = input["command"].as_str()
                    .ok_or_else(|| ToolError {
                        message: "command is required".to_string(),
                        code: Some("missing_command".to_string()),
                    })?;
                let is_destructive = self.is_destructive_command(command);
                serde_json::json!({
                    "success": true,
                    "is_destructive": is_destructive
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