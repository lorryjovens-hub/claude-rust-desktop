//! Bash Execution Module
//!
//! Provides secure bash command execution with permission checking.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum BashError {
    #[error("Permission denied: {0}")]
    PermissionDenied(String),
    #[error("Command not allowed: {0}")]
    CommandNotAllowed(String),
    #[error("Execution failed: {0}")]
    ExecutionFailed(String),
    #[error("Path outside workspace: {0}")]
    PathOutsideWorkspace(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BashCommand {
    pub command: String,
    pub cwd: Option<String>,
    pub env: HashMap<String, String>,
    pub timeout_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BashResult {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
    pub duration_ms: u64,
}

pub struct BashExecutor {
    read_only: bool,
    allowed_paths: Vec<String>,
    blocked_commands: Vec<String>,
}

impl Default for BashExecutor {
    fn default() -> Self {
        Self::new()
    }
}

impl BashExecutor {
    pub fn new() -> Self {
        Self {
            read_only: false,
            allowed_paths: vec![],
            blocked_commands: vec![
                "rm -rf /".to_string(),
                "dd if=".to_string(),
                "mkfs".to_string(),
                ":(){:|:&};:".to_string(),
            ],
        }
    }

    pub fn with_read_only(mut self, read_only: bool) -> Self {
        self.read_only = read_only;
        self
    }

    pub fn is_dangerous(&self, command: &str) -> bool {
        let cmd_lower = command.to_lowercase();
        for blocked in &self.blocked_commands {
            if cmd_lower.contains(&blocked.to_lowercase()) {
                return true;
            }
        }
        false
    }

    pub fn validate_command(&self, command: &str) -> Result<(), BashError> {
        if self.is_dangerous(command) {
            return Err(BashError::CommandNotAllowed(command.to_string()));
        }

        let write_patterns = [" > ", " >> ", "| ", "&& ", "; ", "2> "];
        if self.read_only {
            for pattern in &write_patterns {
                if command.contains(pattern) {
                    return Err(BashError::PermissionDenied(format!(
                        "Write operation '{}' not allowed in read-only mode",
                        pattern
                    )));
                }
            }
        }

        Ok(())
    }

    pub async fn execute(&self, cmd: &BashCommand) -> Result<BashResult, BashError> {
        self.validate_command(&cmd.command)?;

        let start = std::time::Instant::now();
        let output = tokio::process::Command::new("sh")
            .args(["-c", &cmd.command])
            .current_dir(cmd.cwd.as_deref().unwrap_or("."))
            .envs(&cmd.env)
            .output()
            .await
            .map_err(|e| BashError::ExecutionFailed(e.to_string()))?;

        let duration = start.elapsed().as_millis() as u64;

        Ok(BashResult {
            stdout: String::from_utf8_lossy(&output.stdout).to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
            exit_code: output.status.code().unwrap_or(-1),
            duration_ms: duration,
        })
    }
}