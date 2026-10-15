//! PowerShell Tool - Windows PowerShell execution
//!
//! Provides comprehensive PowerShell command execution with security checks.

use crate::tools::{Tool, ToolOutput, ToolError};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PowerShellResult {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
    pub duration_ms: u64,
}

#[derive(Debug, Clone)]
pub struct PowerShellTool {
    workspace_path: Option<String>,
    read_only: bool,
    allowed_paths: Vec<String>,
    blocked_paths: Vec<String>,
}

impl Default for PowerShellTool {
    fn default() -> Self {
        Self::new()
    }
}

impl PowerShellTool {
    pub fn new() -> Self {
        Self {
            workspace_path: None,
            read_only: false,
            allowed_paths: Vec::new(),
            blocked_paths: vec![
                "C:\\Windows\\System32\\config".to_string(),
                "C:\\Windows\\System32\\winevt".to_string(),
                "C:\\Windows\\System32\\drivers".to_string(),
            ],
        }
    }

    pub fn with_workspace(mut self, path: &str) -> Self {
        self.workspace_path = Some(path.to_string());
        self
    }

    pub fn with_read_only(mut self, read_only: bool) -> Self {
        self.read_only = read_only;
        self
    }

    fn is_path_safe(&self, path: &str) -> bool {
        let path_obj = Path::new(path);

        for blocked in &self.blocked_paths {
            if path_obj.starts_with(blocked) {
                return false;
            }
        }

        if let Some(ref workspace) = self.workspace_path {
            if !path.starts_with(workspace) && !path.starts_with('.') {
                return false;
            }
        }

        true
    }

    fn check_dangerous_command(&self, command: &str) -> Option<String> {
        let cmd_lower = command.to_lowercase();

        let dangerous_patterns = [
            ("format-volume", "Format-Volume - permanently erases disk"),
            ("clear-disk", "Clear-Disk - permanently erases disk"),
            ("remove-item -recurse -force /", "Recursive force delete of root"),
            ("stop-computer", "Shuts down the computer"),
            ("restart-computer", "Restarts the computer"),
            ("shutdown-computer", "Shuts down the computer"),
            ("remove-module", "Removes a PowerShell module"),
            ("uninstall-module", "Uninstalls a PowerShell module"),
            ("set-executionpolicy", "Modifies execution policy (security risk)"),
            ("invoke-expression", "Dynamic code execution (security risk)"),
            ("iex", "Invoke-Expression alias - dynamic code execution"),
            ("invoke-webrequest", "Web request (potential security risk)"),
            ("iwr", "Invoke-WebRequest alias"),
            ("downloadstring", "Downloads content from web"),
            ("downloadfile", "Downloads files from web"),
            ("new-service", "Creates a new Windows service"),
            ("stop-service", "Stops a Windows service"),
            ("set-service", "Modifies a Windows service"),
            ("remove-service", "Removes a Windows service"),
            ("reg add", "Adds to registry"),
            ("reg delete", "Deletes from registry"),
            ("reg import", "Imports registry files"),
            ("bcdedit", "Modifies boot configuration"),
            ("diskpart", "Disk partitioning utility"),
            ("cipher /w:", "Securely deletes files"),
            ("takeown", "Takes ownership of files/folders"),
            ("icacls", "Modifies file permissions"),
        ];

        for (pattern, description) in &dangerous_patterns {
            if cmd_lower.contains(*pattern) {
                return Some(description.to_string());
            }
        }

        None
    }

    fn check_git_dangerous(&self, command: &str) -> Option<String> {
        let cmd_lower = command.to_lowercase();

        let git_dangerous = [
            ("git filter-branch", "Rewrites git history"),
            ("git reset --hard", "Permanently discards changes"),
            ("git push --force", "Force pushes can overwrite history"),
            ("git push --delete", "Deletes remote branches"),
            ("rm -rf .git", "Deletes entire git repository"),
            ("git clean -fd", "Force removes untracked files"),
            ("git reflog expire", "Expires reflog entries"),
        ];

        for (pattern, description) in &git_dangerous {
            if cmd_lower.contains(*pattern) {
                return Some(description.to_string());
            }
        }

        None
    }

    async fn execute_internal(&self, command: &str, cwd: Option<&str>) -> Result<PowerShellResult, ToolError> {
        if self.read_only {
            let write_patterns = ["> ", ">> ", "| ", "&& ", "; ", "2> "];
            for pattern in &write_patterns {
                if command.contains(pattern) {
                    return Err(ToolError {
                        message: format!("Write operation '{}' not allowed in read-only mode", pattern),
                        code: Some("read_only_violation".to_string()),
                    });
                }
            }
        }

        if let Some(danger) = self.check_dangerous_command(command) {
            return Err(ToolError {
                message: format!("Dangerous command detected: {}", danger),
                code: Some("dangerous_command".to_string()),
            });
        }

        if let Some(git_danger) = self.check_git_dangerous(command) {
            return Err(ToolError {
                message: format!("Potentially dangerous git operation: {}", git_danger),
                code: Some("dangerous_git_operation".to_string()),
            });
        }

        let ps_command = format!(
            "Set-Location -Path '{}'; {}",
            cwd.unwrap_or("."),
            command
        );

        let start = std::time::Instant::now();

        let output = match tokio::process::Command::new("pwsh")
            .args(["-NoProfile", "-NonInteractive", "-Command", &ps_command])
            .output()
            .await
        {
            Ok(out) => out,
            Err(_) => {
                tokio::process::Command::new("powershell")
                    .args(["-NoProfile", "-NonInteractive", "-Command", &ps_command])
                    .output()
                    .await
                    .map_err(|e| ToolError {
                        message: format!("Failed to execute PowerShell: {}", e),
                        code: Some("execution_failed".to_string()),
                    })?
            }
        };

        let duration = start.elapsed().as_millis() as u64;

        Ok(PowerShellResult {
            stdout: String::from_utf8_lossy(&output.stdout).to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
            exit_code: output.status.code().unwrap_or(-1),
            duration_ms: duration,
        })
    }
}

#[derive(Debug, Deserialize)]
struct ExecuteInput {
    operation: String,
    command: String,
    #[serde(default)]
    cwd: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ValidateInput {
    operation: String,
    command: String,
}

#[async_trait]
impl Tool for PowerShellTool {
    fn name(&self) -> &str {
        "PowerShell"
    }

    fn description(&self) -> &str {
        "Execute PowerShell commands on Windows with security checks. Supports pwsh and powershell.exe. Operations: execute, validate, security_check"
    }

    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "operation": {
                    "type": "string",
                    "enum": ["execute", "validate", "security_check"],
                    "description": "Operation to perform"
                },
                "command": {
                    "type": "string",
                    "description": "PowerShell command to execute"
                },
                "cwd": {
                    "type": "string",
                    "description": "Working directory for the command"
                }
            },
            "required": ["operation", "command"]
        })
    }

    async fn execute(&self, input: serde_json::Value) -> Result<ToolOutput, ToolError> {
        let input: ExecuteInput = serde_json::from_value(input)
            .map_err(|e| ToolError {
                message: format!("Invalid input: {}", e),
                code: Some("invalid_input".to_string()),
            })?;

        match input.operation.as_str() {
            "execute" => {
                let result = self.execute_internal(&input.command, input.cwd.as_deref()).await?;

                let result_json = serde_json::json!({
                    "stdout": result.stdout,
                    "stderr": result.stderr,
                    "exit_code": result.exit_code,
                    "duration_ms": result.duration_ms,
                    "success": result.exit_code == 0,
                });

                Ok(ToolOutput {
                    output_type: "json".to_string(),
                    content: serde_json::to_string_pretty(&result_json).unwrap(),
                    metadata: HashMap::new(),
                })
            }
            "validate" => {
                self.validate_command(&input.command)
            }
            "security_check" => {
                self.security_check(&input.command)
            }
            _ => Err(ToolError {
                message: format!("Unknown operation: {}", input.operation),
                code: Some("unknown_operation".to_string()),
            }),
        }
    }
}

impl PowerShellTool {
    fn validate_command(&self, command: &str) -> Result<ToolOutput, ToolError> {
        let mut issues = Vec::new();

        if self.read_only {
            let write_patterns = ["> ", ">> ", "| ", "&& ", "; ", "2> "];
            for pattern in &write_patterns {
                if command.contains(pattern) {
                    issues.push(format!("Write operation '{}' blocked in read-only mode", pattern));
                }
            }
        }

        if let Some(danger) = self.check_dangerous_command(command) {
            issues.push(format!("Dangerous command: {}", danger));
        }

        if let Some(git_danger) = self.check_git_dangerous(command) {
            issues.push(format!("Potentially dangerous git operation: {}", git_danger));
        }

        if issues.is_empty() {
            Ok(ToolOutput {
                output_type: "json".to_string(),
                content: serde_json::to_string_pretty(&serde_json::json!({
                    "valid": true,
                    "message": "Command is safe to execute"
                })).unwrap(),
                metadata: HashMap::new(),
            })
        } else {
            Ok(ToolOutput {
                output_type: "json".to_string(),
                content: serde_json::to_string_pretty(&serde_json::json!({
                    "valid": false,
                    "issues": issues
                })).unwrap(),
                metadata: HashMap::new(),
            })
        }
    }

    fn security_check(&self, command: &str) -> Result<ToolOutput, ToolError> {
        let checks = serde_json::json!({
            "dangerous_check": {
                "passed": self.check_dangerous_command(command).is_none(),
                "details": self.check_dangerous_command(command)
            },
            "git_dangerous_check": {
                "passed": self.check_git_dangerous(command).is_none(),
                "details": self.check_git_dangerous(command)
            },
            "read_only_check": {
                "passed": !self.read_only || !Self::contains_write_operation(command),
                "enabled": self.read_only
            }
        });

        Ok(ToolOutput {
            output_type: "json".to_string(),
            content: serde_json::to_string_pretty(&checks).unwrap(),
            metadata: HashMap::new(),
        })
    }

    fn contains_write_operation(command: &str) -> bool {
        let write_patterns = ["> ", ">> ", "| ", "&& ", "; ", "2> "];
        write_patterns.iter().any(|p| command.contains(*p))
    }
}