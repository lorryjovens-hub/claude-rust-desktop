//! Delegation Module - Multi-agent parallel task execution
//!
//! Spawns child AIAgent instances with isolated context, restricted toolsets,
//! and their own terminal sessions. Supports single-task and batch (parallel)
//! modes. The parent blocks until all children complete.
//!
//! Each child gets:
//! - A fresh conversation (no parent history)
//! - Its own task_id (own terminal session, file ops cache)
//! - A restricted toolset (configurable, with blocked tools always stripped)
//! - A focused system prompt built from the delegated goal + context
//!
//! The parent's context only sees the delegation call and the summary result,
//! never the child's intermediate tool calls or reasoning.

pub mod subagent;
pub mod task;
pub mod progress;

pub use subagent::{SubAgent, SubAgentConfig, SubAgentResult};
pub use task::{DelegationTask, DelegationResult, TaskStatus};
pub use progress::{ProgressCallback, ProgressEvent};

pub const MAX_CONCURRENT_CHILDREN: usize = 3;
pub const MAX_DELEGATION_DEPTH: usize = 2;

const BLOCKED_TOOLS: &[&str] = &[
    "delegate_task",
    "clarify",
    "memory",
    "send_message",
    "execute_code",
];

pub fn is_blocked_tool(tool_name: &str) -> bool {
    BLOCKED_TOOLS.contains(&tool_name)
}

pub fn filter_blocked_tools(tools: &[String]) -> Vec<String> {
    tools.iter()
        .filter(|t| !is_blocked_tool(t))
        .cloned()
        .collect()
}
