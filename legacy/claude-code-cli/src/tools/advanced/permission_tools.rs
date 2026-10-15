//! Permission Tool - Permission enforcement testing
//!
//! Test-only tool for verifying permission enforcement behavior.

use crate::tools::{Tool, ToolOutput, ToolError};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PermissionMode {
    ReadOnly,
    WorkspaceWrite,
    DangerFullAccess,
}

impl PermissionMode {
    fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "read_only" => Some(PermissionMode::ReadOnly),
            "workspace_write" => Some(PermissionMode::WorkspaceWrite),
            "danger_full_access" => Some(PermissionMode::DangerFullAccess),
            _ => None,
        }
    }
}

pub struct PermissionTool;

impl PermissionTool {
    pub fn new() -> Self {
        Self
    }
}

impl Default for PermissionTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for PermissionTool {
    fn name(&self) -> &str {
        "testing_permission"
    }

    fn description(&self) -> &str {
        "Test-only tool for verifying permission enforcement behavior"
    }

    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "description": "Action to test permission for"
                }
            },
            "required": ["action"]
        })
    }

    async fn execute(&self, input: serde_json::Value) -> Result<ToolOutput, ToolError> {
        let action = input["action"].as_str()
            .ok_or_else(|| ToolError {
                message: "action is required".to_string(),
                code: Some("missing_action".to_string()),
            })?;

        let result = format!("Permission test for action: {}\n\nNote: This is a test-only tool. Configure real permission enforcement in production.", action);

        Ok(ToolOutput {
            output_type: "text".to_string(),
            content: result,
            metadata: HashMap::new(),
        })
    }
}