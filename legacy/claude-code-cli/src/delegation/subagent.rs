//! SubAgent - Isolated child agent for delegated tasks
//!
//! Manages the lifecycle of a sub-agent including creation, execution,
//! and result collection.

use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubAgentConfig {
    pub goal: String,
    pub context: Option<String>,
    pub toolsets: Vec<String>,
    pub model: Option<String>,
    pub max_iterations: usize,
    pub workspace_path: Option<String>,
    pub parent_id: Option<String>,
    pub task_index: usize,
}

impl Default for SubAgentConfig {
    fn default() -> Self {
        Self {
            goal: String::new(),
            context: None,
            toolsets: vec!["terminal".to_string(), "file".to_string()],
            model: None,
            max_iterations: 50,
            workspace_path: None,
            parent_id: None,
            task_index: 0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubAgentResult {
    pub task_index: usize,
    pub status: SubAgentStatus,
    pub summary: String,
    pub duration_secs: f64,
    pub api_calls: usize,
    pub tools_used: Vec<String>,
    pub interrupted: bool,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum SubAgentStatus {
    Pending,
    Running,
    Completed,
    Failed,
    Interrupted,
}

#[derive(Debug, Clone)]
pub struct SubAgent {
    pub agent_id: String,
    pub config: SubAgentConfig,
    pub status: SubAgentStatus,
    pub result: Option<SubAgentResult>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub started_at: Option<chrono::DateTime<chrono::Utc>>,
    pub finished_at: Option<chrono::DateTime<chrono::Utc>>,
}

impl SubAgent {
    pub fn new(config: SubAgentConfig) -> Self {
        let agent_id = format!("subagent-{}", uuid::Uuid::new_v4());
        Self {
            agent_id,
            config,
            status: SubAgentStatus::Pending,
            result: None,
            created_at: chrono::Utc::now(),
            started_at: None,
            finished_at: None,
        }
    }

    pub fn build_system_prompt(&self) -> String {
        let mut parts = vec![
            "You are a focused subagent working on a specific delegated task.".to_string(),
            String::new(),
            format!("YOUR TASK:\n{}", self.config.goal),
        ];

        if let Some(ref context) = self.config.context {
            if !context.trim().is_empty() {
                parts.push(format!("\nCONTEXT:\n{}", context));
            }
        }

        if let Some(ref path) = self.config.workspace_path {
            if !path.trim().is_empty() {
                parts.push(format!(
                    "\nWORKSPACE PATH:\n{}\nUse this exact path for local repository/workdir operations unless the task explicitly says otherwise.",
                    path
                ));
            }
        }

        parts.push(
            "\nComplete this task using the tools available to you. When finished, provide a clear, concise summary of:\n- What you did\n- What you found or accomplished\n- Any files you created or modified\n- Any issues encountered\n\nImportant workspace rule: Never assume a repository lives at /workspace/... or any other container-style path unless the task/context explicitly gives that path. If no exact local path is provided, discover it first before issuing git/workdir-specific commands.\n\nBe thorough but concise -- your response is returned to the parent agent as a summary.".to_string(),
        );

        parts.join("\n")
    }

    pub fn mark_running(&mut self) {
        self.status = SubAgentStatus::Running;
        self.started_at = Some(chrono::Utc::now());
    }

    pub fn mark_completed(&mut self, result: SubAgentResult) {
        self.status = if result.interrupted {
            SubAgentStatus::Interrupted
        } else {
            SubAgentStatus::Completed
        };
        self.finished_at = Some(chrono::Utc::now());
        self.result = Some(result);
    }

    pub fn mark_failed(&mut self, error: String) {
        self.status = SubAgentStatus::Failed;
        self.finished_at = Some(chrono::Utc::now());
        self.result = Some(SubAgentResult {
            task_index: self.config.task_index,
            status: SubAgentStatus::Failed,
            summary: String::new(),
            duration_secs: self.elapsed_secs(),
            api_calls: 0,
            tools_used: vec![],
            interrupted: false,
            error: Some(error),
        });
    }

    pub fn elapsed_secs(&self) -> f64 {
        let start = self.started_at.unwrap_or(self.created_at);
        let end = self.finished_at.unwrap_or_else(chrono::Utc::now);
        (end - start).num_milliseconds() as f64 / 1000.0
    }
}
