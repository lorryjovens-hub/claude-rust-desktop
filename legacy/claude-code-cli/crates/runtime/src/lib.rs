//! Runtime Module - Core execution runtime
//!
//! Handles: Bash execution, File operations, Git context, Permissions,
//! Session management, LSP client, MCP lifecycle, Hooks, Sandbox.

pub mod bash;
pub mod permissions;
pub mod session;
pub mod hooks;
pub mod sandbox;
pub mod config;

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum RuntimeError {
    #[error("Permission denied: {0}")]
    PermissionDenied(String),
    #[error("Execution failed: {0}")]
    ExecutionFailed(String),
    #[error("Session not found: {0}")]
    SessionNotFound(String),
    #[error("Sandbox violation: {0}")]
    SandboxViolation(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeConfig {
    pub read_only: bool,
    pub allowdangerous: bool,
    pub sandbox_enabled: bool,
    pub max_file_size_bytes: u64,
    pub max_output_bytes: u64,
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        Self {
            read_only: false,
            allowdangerous: false,
            sandbox_enabled: true,
            max_file_size_bytes: 10 * 1024 * 1024,
            max_output_bytes: 100 * 1024,
        }
    }
}