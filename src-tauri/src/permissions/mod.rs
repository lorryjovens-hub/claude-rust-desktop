pub mod manager;
pub mod rules;
pub mod audit;

pub use manager::{PermissionManager, PermissionMode};
pub use rules::{PermissionRule, PermissionAction, PermissionScope, PermissionLevel};
pub use audit::{AuditEntry, AuditLogger};

use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum PermissionResult {
    Granted,
    Denied(String),
    RequiresConfirmation(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolPermission {
    pub tool_name: String,
    pub allowed: bool,
    pub requires_confirmation: bool,
    pub scope: PermissionScope,
    pub level: PermissionLevel,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PermissionContext {
    pub tool_name: String,
    pub tool_input: serde_json::Value,
    pub conversation_id: String,
    pub user_id: Option<String>,
    pub workspace_path: Option<String>,
}

pub trait PermissionChecker {
    fn check_permission(&self, context: &PermissionContext) -> PermissionResult;
}

pub type PermissionCheckerRef = Arc<dyn PermissionChecker + Send + Sync>;
