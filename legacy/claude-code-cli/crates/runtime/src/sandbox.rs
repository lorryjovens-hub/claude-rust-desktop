//! Sandbox Module
//!
//! Provides sandboxed execution with restricted file system access.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxConfig {
    pub enabled: bool,
    pub allowed_paths: Vec<String>,
    pub blocked_paths: Vec<String>,
    pub workspace_path: Option<String>,
    pub max_file_size_bytes: u64,
    pub allow_network: bool,
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
                "C:\\Windows".to_string(),
                "C:\\Program Files".to_string(),
            ],
            workspace_path: None,
            max_file_size_bytes: 10 * 1024 * 1024,
            allow_network: false,
        }
    }
}

pub struct Sandbox {
    config: SandboxConfig,
}

impl Default for Sandbox {
    fn default() -> Self {
        Self::new()
    }
}

impl Sandbox {
    pub fn new() -> Self {
        Self {
            config: SandboxConfig::default(),
        }
    }

    pub fn with_config(mut self, config: SandboxConfig) -> Self {
        self.config = config;
        self
    }

    pub fn is_path_allowed(&self, path: &str) -> bool {
        let path = Path::new(path);

        for blocked in &self.config.blocked_paths {
            if path.starts_with(blocked) {
                return false;
            }
        }

        if let Some(ref workspace) = self.config.workspace_path {
            if !path.starts_with(workspace) {
                return false;
            }
        }

        if !self.config.allowed_paths.is_empty() {
            let mut allowed = false;
            for allowed_path in &self.config.allowed_paths {
                if path.starts_with(allowed_path) {
                    allowed = true;
                    break;
                }
            }
            if !allowed {
                return false;
            }
        }

        true
    }

    pub fn check_file_size(&self, path: &Path) -> Result<(), SandboxError> {
        if let Ok(metadata) = std::fs::metadata(path) {
            if metadata.len() > self.config.max_file_size_bytes {
                return Err(SandboxError::FileTooLarge {
                    size: metadata.len(),
                    max: self.config.max_file_size_bytes,
                });
            }
        }
        Ok(())
    }

    pub fn validate_operation(&self, operation: &str, path: Option<&str>) -> Result<(), SandboxError> {
        if !self.config.enabled {
            return Ok(());
        }

        if let Some(p) = path {
            if !self.is_path_allowed(p) {
                return Err(SandboxError::PathNotAllowed(p.to_string()));
            }
        }

        if operation == "write" || operation == "delete" {
            if !self.config.allowed_paths.is_empty() || self.config.workspace_path.is_some() {
                return Err(SandboxError::WriteNotAllowed(operation.to_string()));
            }
        }

        Ok(())
    }
}

#[derive(Debug, Clone, thiserror::Error)]
pub enum SandboxError {
    #[error("Path not allowed: {0}")]
    PathNotAllowed(String),
    #[error("File too large: {size} bytes (max: {max} bytes)")]
    FileTooLarge { size: u64, max: u64 },
    #[error("Write operation not allowed: {0}")]
    WriteNotAllowed(String),
}