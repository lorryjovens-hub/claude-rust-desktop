//! Permissions Module
//!
//! Manages permission checking and enforcement for operations.

use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum Permission {
    Read,
    Write,
    Execute,
    Network,
    EnvAccess,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum PermissionMode {
    Normal,
    ReadOnly,
    WorkspaceWrite,
    DangerFullAccess,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PermissionContext {
    pub mode: PermissionMode,
    pub workspace_path: Option<String>,
    pub allowed_paths: HashSet<String>,
    pub blocked_paths: HashSet<String>,
}

impl Default for PermissionContext {
    fn default() -> Self {
        Self {
            mode: PermissionMode::Normal,
            workspace_path: None,
            allowed_paths: HashSet::new(),
            blocked_paths: HashSet::from([
                "/etc".to_string(),
                "/usr".to_string(),
                "/bin".to_string(),
                "/sbin".to_string(),
                "/var".to_string(),
                "/root".to_string(),
                "C:\\Windows".to_string(),
                "C:\\Program Files".to_string(),
            ]),
        }
    }
}

impl PermissionContext {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_workspace(mut self, path: &str) -> Self {
        self.workspace_path = Some(path.to_string());
        self
    }

    pub fn with_mode(mut self, mode: PermissionMode) -> Self {
        self.mode = mode;
        self
    }

    pub fn check_permission(&self, permission: &Permission, path: Option<&str>) -> bool {
        match self.mode {
            PermissionMode::DangerFullAccess => true,
            PermissionMode::Normal => {
                if let Some(p) = path {
                    if self.blocked_paths.iter().any(|bp| p.starts_with(bp)) {
                        return false;
                    }
                    if let Some(ref wp) = self.workspace_path {
                        if !p.starts_with(wp) {
                            return false;
                        }
                    }
                }
                match permission {
                    Permission::Write => false,
                    _ => true,
                }
            }
            PermissionMode::ReadOnly => permission == &Permission::Read,
            PermissionMode::WorkspaceWrite => {
                if let Some(p) = path {
                    if let Some(ref wp) = self.workspace_path {
                        if !p.starts_with(wp) {
                            return false;
                        }
                    }
                }
                true
            }
        }
    }

    pub fn is_path_allowed(&self, path: &str) -> bool {
        for blocked in &self.blocked_paths {
            if path.starts_with(blocked) {
                return false;
            }
        }
        if let Some(ref wp) = self.workspace_path {
            if !path.starts_with(wp) {
                return false;
            }
        }
        true
    }
}