//! Advanced Tools Module - Extended tool suite from claw-code-main
//!
//! This module provides additional tools that complement the basic tools:
//! - Agent tools: Worker lifecycle management, Agent system
//! - Team tools: Multi-agent coordination
//! - Cron tools: Scheduled task management
//! - LSP tools: Language Server Protocol integration
//! - MCP tools: Model Context Protocol integration
//! - Web tools: WebFetch and WebSearch
//! - Planning tools: Plan mode for strategic thinking
//! - Productivity tools: Brief, TodoWrite, ToolSearch
//! - Worktree tools: Git worktree management
//! - Delegation tools: Multi-agent parallel task execution

pub mod worker;
pub mod team;
pub mod cron;
pub mod lsp;
pub mod mcp_bridge;
pub mod web_tools;
pub mod ask_question;
pub mod permission_tools;
pub mod agent;
pub mod plan_mode;
pub mod worktree;
pub mod brief;
pub mod todo_write;
pub mod tool_search;
pub mod notebook;
pub mod powershell;
pub mod escalation;
pub mod bash;
pub mod config;
pub mod task_tools;
pub mod send_message;
pub mod mcp_tools;
pub mod skill;
pub mod delegation;

pub use worker::{WorkerTool, WorkerState};
pub use team::{TeamTool, Team, TeamTask};
pub use cron::{CronTool, CronTask, ScheduledTask};
pub use lsp::{LspTool, LspAction, LspLocation};
pub use mcp_bridge::{McpToolBridge, McpResource};
pub use web_tools::{WebFetchTool, WebSearchTool};
pub use ask_question::AskQuestionTool;
pub use permission_tools::PermissionTool;
pub use agent::{AgentTool, Agent, AgentStatus, AgentMode};
pub use plan_mode::{PlanModeTool, Plan, PlanStep};
pub use worktree::{WorktreeTool, Worktree};
pub use brief::BriefTool;
pub use todo_write::{TodoWriteTool, Todo};
pub use tool_search::{ToolSearchTool, ToolInfo};
pub use notebook::NotebookEditTool;
pub use powershell::PowerShellTool;
pub use escalation::{PrivilegeEscalation, PrivilegeLevel, EscalationRequest, SharedEscalation, create_shared_escalation};
pub use bash::BashTool;
pub use config::ConfigTool;
pub use task_tools::{TaskCreateTool, TaskGetTool, TaskListTool, TaskUpdateTool, TaskOutputTool, TaskStopTool, TaskStore, SharedTaskStore, create_shared_task_store, get_task_store};
pub use send_message::{SendMessageTool, Message, MessageStore, SharedMessageStore, create_shared_message_store, get_message_store};
pub use mcp_tools::{ListMcpResourcesTool, ReadMcpResourceTool, McpAuthTool, RemoteTriggerTool, SyntheticOutputTool, McpServer, McpStore, SharedMcpStore, create_shared_mcp_store, get_mcp_store};
pub use skill::{SkillTool, Skill, SkillCategory, SkillStore, SharedSkillStore, create_shared_skill_store, get_skill_store};
pub use delegation::{DelegationTool, DelegateTaskInput, DelegateTaskOutput, DelegateSubTask, TaskSummary, DelegationConfig};
