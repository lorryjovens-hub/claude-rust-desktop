//! Worktree Tool - Git worktree management
//!
//! Provides worktree operations for managing multiple working trees in a git repository.

use crate::tools::{Tool, ToolOutput, ToolError};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Command;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Worktree {
    pub path: String,
    pub branch: String,
    pub is_main: bool,
    pub commit: Option<String>,
}

pub struct WorktreeTool {
    worktrees: Vec<Worktree>,
}

impl Default for WorktreeTool {
    fn default() -> Self {
        Self::new()
    }
}

impl WorktreeTool {
    pub fn new() -> Self {
        Self {
            worktrees: Vec::new(),
        }
    }

    fn get_repo_root() -> Result<PathBuf, ToolError> {
        let output = Command::new("git")
            .args(["rev-parse", "--show-toplevel"])
            .output()
            .map_err(|e| ToolError {
                message: format!("Failed to execute git: {}", e),
                code: Some("git_error".to_string()),
            })?;

        if !output.status.success() {
            return Err(ToolError {
                message: "Not in a git repository".to_string(),
                code: Some("not_git_repo".to_string()),
            });
        }

        let root = String::from_utf8_lossy(&output.stdout).trim().to_string();
        Ok(PathBuf::from(root))
    }

    fn parse_worktree_output(output: &str) -> Vec<Worktree> {
        let mut worktrees = Vec::new();
        let lines = output.lines();

        for line in lines {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 3 {
                let path = parts[0];
                let branch_or_commit = parts[1];

                worktrees.push(Worktree {
                    path: path.to_string(),
                    branch: branch_or_commit.to_string(),
                    is_main: path.contains("(detached"),
                    commit: None,
                });
            }
        }
        worktrees
    }

    async fn handle_list(&self) -> Result<Vec<Worktree>, ToolError> {
        let output = Command::new("git")
            .args(["worktree", "list", "--porcelain"])
            .output()
            .map_err(|e| ToolError {
                message: format!("Failed to execute git worktree list: {}", e),
                code: Some("git_error".to_string()),
            })?;

        if !output.status.success() {
            return Err(ToolError {
                message: "Failed to list worktrees".to_string(),
                code: Some("git_error".to_string()),
            });
        }

        let output_str = String::from_utf8_lossy(&output.stdout);
        let mut worktrees = Vec::new();

        let mut current_path = String::new();
        let mut current_branch = String::new();

        for line in output_str.lines() {
            if line.starts_with("worktree ") {
                current_path = line.trim_start_matches("worktree ").to_string();
            } else if line.starts_with("branch refs/heads/") {
                current_branch = line.trim_start_matches("branch refs/heads/").to_string();
            } else if line.is_empty() && !current_path.is_empty() {
                worktrees.push(Worktree {
                    path: current_path.clone(),
                    branch: current_branch.clone(),
                    is_main: current_branch.is_empty(),
                    commit: None,
                });
                current_path.clear();
                current_branch.clear();
            }
        }

        Ok(worktrees)
    }

    async fn handle_enter(&self, name: &str) -> Result<String, ToolError> {
        Self::get_repo_root()?;

        let worktree_path = PathBuf::from(name);
        if !worktree_path.exists() {
            return Err(ToolError {
                message: format!("Worktree does not exist: {}", name),
                code: Some("worktree_not_found".to_string()),
            });
        }

        Ok(format!("Switched to worktree: {}", name))
    }

    async fn handle_create(&self, input: &serde_json::Value) -> Result<Worktree, ToolError> {
        let path = input["path"].as_str()
            .ok_or_else(|| ToolError {
                message: "path is required".to_string(),
                code: Some("missing_path".to_string()),
            })?;

        let branch = input["branch"].as_str()
            .ok_or_else(|| ToolError {
                message: "branch is required".to_string(),
                code: Some("missing_branch".to_string()),
            })?;

        Self::get_repo_root()?;

        let output = Command::new("git")
            .args(["worktree", "add", "-b", branch, path])
            .output()
            .map_err(|e| ToolError {
                message: format!("Failed to create worktree: {}", e),
                code: Some("git_error".to_string()),
            })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(ToolError {
                message: format!("Failed to create worktree: {}", stderr),
                code: Some("git_error".to_string()),
            });
        }

        let worktree = Worktree {
            path: path.to_string(),
            branch: branch.to_string(),
            is_main: false,
            commit: None,
        };

        Ok(worktree)
    }

    async fn handle_remove(&self, path: &str) -> Result<String, ToolError> {
        Self::get_repo_root()?;

        let output = Command::new("git")
            .args(["worktree", "remove", path, "--force"])
            .output()
            .map_err(|e| ToolError {
                message: format!("Failed to remove worktree: {}", e),
                code: Some("git_error".to_string()),
            })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(ToolError {
                message: format!("Failed to remove worktree: {}", stderr),
                code: Some("git_error".to_string()),
            });
        }

        Ok(format!("Worktree removed: {}", path))
    }

    async fn handle_prune(&self) -> Result<String, ToolError> {
        Self::get_repo_root()?;

        let output = Command::new("git")
            .args(["worktree", "prune"])
            .output()
            .map_err(|e| ToolError {
                message: format!("Failed to prune worktrees: {}", e),
                code: Some("git_error".to_string()),
            })?;

        if !output.status.success() {
            return Err(ToolError {
                message: "Failed to prune worktrees".to_string(),
                code: Some("git_error".to_string()),
            });
        }

        Ok("Worktrees pruned".to_string())
    }

    async fn handle_lock(&self, path: &str) -> Result<String, ToolError> {
        Self::get_repo_root()?;

        let output = Command::new("git")
            .args(["worktree", "lock", path])
            .output()
            .map_err(|e| ToolError {
                message: format!("Failed to lock worktree: {}", e),
                code: Some("git_error".to_string()),
            })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(ToolError {
                message: format!("Failed to lock worktree: {}", stderr),
                code: Some("git_error".to_string()),
            });
        }

        Ok(format!("Worktree locked: {}", path))
    }

    async fn handle_unlock(&self, path: &str) -> Result<String, ToolError> {
        Self::get_repo_root()?;

        let output = Command::new("git")
            .args(["worktree", "unlock", path])
            .output()
            .map_err(|e| ToolError {
                message: format!("Failed to unlock worktree: {}", e),
                code: Some("git_error".to_string()),
            })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(ToolError {
                message: format!("Failed to unlock worktree: {}", stderr),
                code: Some("git_error".to_string()),
            });
        }

        Ok(format!("Worktree unlocked: {}", path))
    }
}

#[async_trait]
impl Tool for WorktreeTool {
    fn name(&self) -> &str {
        "worktree"
    }

    fn description(&self) -> &str {
        "Manage git worktrees for working on multiple branches simultaneously"
    }

    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "operation": {
                    "type": "string",
                    "enum": ["list", "enter", "create", "remove", "prune", "lock", "unlock"]
                },
                "path": { "type": "string", "description": "Worktree path" },
                "branch": { "type": "string", "description": "Branch name" },
                "name": { "type": "string", "description": "Worktree name or path" }
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
            "list" => {
                let worktrees = self.handle_list().await?;
                serde_json::json!({
                    "success": true,
                    "worktrees": worktrees,
                    "count": worktrees.len()
                }).to_string()
            }
            "enter" => {
                let name = input["name"].as_str()
                    .ok_or_else(|| ToolError {
                        message: "name is required".to_string(),
                        code: Some("missing_name".to_string()),
                    })?;
                let result = self.handle_enter(name).await?;
                serde_json::json!({
                    "success": true,
                    "result": result
                }).to_string()
            }
            "create" => {
                let worktree = self.handle_create(&input).await?;
                serde_json::json!({
                    "success": true,
                    "worktree": worktree
                }).to_string()
            }
            "remove" => {
                let path = input["path"].as_str()
                    .ok_or_else(|| ToolError {
                        message: "path is required".to_string(),
                        code: Some("missing_path".to_string()),
                    })?;
                let result = self.handle_remove(path).await?;
                serde_json::json!({
                    "success": true,
                    "result": result
                }).to_string()
            }
            "prune" => {
                let result = self.handle_prune().await?;
                serde_json::json!({
                    "success": true,
                    "result": result
                }).to_string()
            }
            "lock" => {
                let path = input["path"].as_str()
                    .ok_or_else(|| ToolError {
                        message: "path is required".to_string(),
                        code: Some("missing_path".to_string()),
                    })?;
                let result = self.handle_lock(path).await?;
                serde_json::json!({
                    "success": true,
                    "result": result
                }).to_string()
            }
            "unlock" => {
                let path = input["path"].as_str()
                    .ok_or_else(|| ToolError {
                        message: "path is required".to_string(),
                        code: Some("missing_path".to_string()),
                    })?;
                let result = self.handle_unlock(path).await?;
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