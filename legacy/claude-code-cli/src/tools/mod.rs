//! Tools Module - File operations, commands, search, etc.

pub mod file_read;
pub mod file_edit;
pub mod file_write;
pub mod execute_command;
pub mod search;
pub mod list_files;
pub mod git_operations;
pub mod task_management;
pub mod note_edit;
pub mod smart_edit;
pub mod glob_tool;
pub mod grep_tool;
pub mod advanced;
pub mod bash_security;
pub mod sandbox;

pub use file_read::FileReadTool;
pub use file_edit::FileEditTool;
pub use file_write::FileWriteTool;
pub use execute_command::ExecuteCommandTool;
pub use search::SearchTool;
pub use list_files::ListFilesTool;
pub use git_operations::GitOperationsTool;
pub use task_management::TaskManagementTool;
pub use note_edit::NoteEditTool;
pub use smart_edit::SmartEditTool;
pub use glob_tool::GlobTool;
pub use grep_tool::GrepTool;
pub use bash_security::BashSecurityTool;
pub use sandbox::SandboxTool;

pub use advanced::{
    WorkerTool, TeamTool, CronTool, LspTool, McpToolBridge,
    WebFetchTool, WebSearchTool, AskQuestionTool, PermissionTool,
    AgentTool, PlanModeTool, WorktreeTool, BriefTool,
    TodoWriteTool, ToolSearchTool, NotebookEditTool, PowerShellTool, BashTool,
    ConfigTool,
    TaskCreateTool, TaskGetTool, TaskListTool, TaskUpdateTool, TaskOutputTool, TaskStopTool,
    SendMessageTool,
    ListMcpResourcesTool, ReadMcpResourceTool, McpAuthTool, RemoteTriggerTool, SyntheticOutputTool,
    SkillTool,
};

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Tool trait for all tools
#[async_trait]
pub trait Tool: Send + Sync {
    /// Tool name
    fn name(&self) -> &str;

    /// Tool description
    fn description(&self) -> &str;

    /// Tool input schema
    fn input_schema(&self) -> serde_json::Value;

    /// Execute the tool
    async fn execute(&self, input: serde_json::Value) -> Result<ToolOutput, ToolError>;

    /// Convert to OpenAI-compatible function definition
    fn tool_definition(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "function",
            "function": {
                "name": self.name(),
                "description": self.description(),
                "parameters": self.input_schema()
            }
        })
    }
}

/// Tool output
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolOutput {
    /// Output type
    pub output_type: String,
    /// Output content
    pub content: String,
    /// Metadata
    pub metadata: HashMap<String, serde_json::Value>,
}

/// Tool error
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolError {
    /// Error message
    pub message: String,
    /// Error code
    pub code: Option<String>,
}

/// Tool registry
pub struct ToolRegistry {
    /// Registered tools
    tools: HashMap<String, Box<dyn Tool>>,
}

impl ToolRegistry {
    /// Create a new tool registry
    pub fn new() -> Self {
        let mut registry = Self {
            tools: HashMap::new(),
        };

        // Register built-in tools
        registry.register(Box::new(file_read::FileReadTool::new()));
        registry.register(Box::new(file_edit::FileEditTool::new()));
        registry.register(Box::new(file_write::FileWriteTool::new()));
        registry.register(Box::new(execute_command::ExecuteCommandTool::new()));
        registry.register(Box::new(search::SearchTool::new()));
        registry.register(Box::new(list_files::ListFilesTool::new()));
        registry.register(Box::new(git_operations::GitOperationsTool::new()));
        registry.register(Box::new(task_management::TaskManagementTool::new()));
        registry.register(Box::new(note_edit::NoteEditTool::new()));
        registry.register(Box::new(glob_tool::GlobTool::new()));
        registry.register(Box::new(grep_tool::GrepTool::new()));
        registry.register(Box::new(bash_security::BashSecurityTool::new()));
        registry.register(Box::new(sandbox::SandboxTool::new()));

        registry.register(Box::new(advanced::WorkerTool::new()));
        registry.register(Box::new(advanced::TeamTool::new()));
        registry.register(Box::new(advanced::CronTool::new()));
        registry.register(Box::new(advanced::LspTool::new()));
        registry.register(Box::new(advanced::McpToolBridge::new()));
        registry.register(Box::new(advanced::WebFetchTool::new()));
        registry.register(Box::new(advanced::WebSearchTool::new()));
        registry.register(Box::new(advanced::AskQuestionTool::new()));
        registry.register(Box::new(advanced::PermissionTool::new()));
        registry.register(Box::new(advanced::AgentTool::new()));
        registry.register(Box::new(advanced::PlanModeTool::new()));
        registry.register(Box::new(advanced::WorktreeTool::new()));
        registry.register(Box::new(advanced::BriefTool::new()));
        registry.register(Box::new(advanced::TodoWriteTool::new()));
        registry.register(Box::new(advanced::ToolSearchTool::new()));
        registry.register(Box::new(advanced::NotebookEditTool::new()));
        registry.register(Box::new(advanced::PowerShellTool::new()));
        registry.register(Box::new(advanced::BashTool::new()));
        registry.register(Box::new(advanced::ConfigTool::new()));
        registry.register(Box::new(advanced::TaskCreateTool::new()));
        registry.register(Box::new(advanced::TaskGetTool::new()));
        registry.register(Box::new(advanced::TaskListTool::new()));
        registry.register(Box::new(advanced::TaskUpdateTool::new()));
        registry.register(Box::new(advanced::TaskOutputTool::new()));
        registry.register(Box::new(advanced::TaskStopTool::new()));
        registry.register(Box::new(advanced::SendMessageTool::new()));
        registry.register(Box::new(advanced::ListMcpResourcesTool::new()));
        registry.register(Box::new(advanced::ReadMcpResourceTool::new()));
        registry.register(Box::new(advanced::McpAuthTool::new()));
        registry.register(Box::new(advanced::RemoteTriggerTool::new()));
        registry.register(Box::new(advanced::SyntheticOutputTool::new()));
        registry.register(Box::new(advanced::SkillTool::new()));

        registry
    }
    
    /// Register a tool
    pub fn register(&mut self, tool: Box<dyn Tool>) {
        self.tools.insert(tool.name().to_string(), tool);
    }
    
    /// Get a tool by name
    pub fn get(&self, name: &str) -> Option<&dyn Tool> {
        self.tools.get(name).map(|b| b.as_ref())
    }
    
    /// List all tools
    pub fn list(&self) -> Vec<&dyn Tool> {
        self.tools.values().map(|b| b.as_ref()).collect()
    }
    
    /// Execute a tool
    pub async fn execute(&self, name: &str, input: serde_json::Value) -> Result<ToolOutput, ToolError> {
        match self.tools.get(name) {
            Some(tool) => tool.execute(input).await,
            None => Err(ToolError {
                message: format!("Tool not found: {}", name),
                code: Some("tool_not_found".to_string()),
            }),
        }
    }
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}