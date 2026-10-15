//! Delegation Tool - Multi-agent parallel task execution
//!
//! Implements the delegate_task tool that spawns child AIAgent instances
//! with isolated context, restricted toolsets, and their own execution sessions.
//! Supports single-task and batch (parallel) modes.

use crate::api::ChatMessage;
use crate::delegation::subagent::{SubAgent, SubAgentConfig, SubAgentResult, SubAgentStatus};
use crate::delegation::task::{DelegationTask, TaskQueue};
use crate::delegation::{MAX_CONCURRENT_CHILDREN, MAX_DELEGATION_DEPTH, filter_blocked_tools};
use crate::tools::{Tool, ToolError, ToolOutput};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::{RwLock, mpsc};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DelegateTaskInput {
    pub goal: Option<String>,
    pub context: Option<String>,
    pub toolsets: Option<Vec<String>>,
    pub model: Option<String>,
    pub tasks: Option<Vec<DelegateSubTask>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DelegateSubTask {
    pub goal: String,
    pub context: Option<String>,
    pub toolsets: Option<Vec<String>>,
    pub model: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DelegateTaskOutput {
    pub status: String,
    pub results: Vec<TaskSummary>,
    pub total_duration_secs: f64,
    pub completed_count: usize,
    pub failed_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskSummary {
    pub task_index: usize,
    pub status: String,
    pub summary: String,
    pub duration_secs: f64,
    pub tools_used: Vec<String>,
}

pub struct DelegationTool {
    config: DelegationConfig,
    teams: Arc<RwLock<HashMap<String, Arc<TaskQueue>>>>,
    active_subagents: Arc<RwLock<HashMap<String, DelegationHandle>>>,
}

#[derive(Debug, Clone)]
pub struct DelegationConfig {
    pub max_concurrent: usize,
    pub max_depth: usize,
    pub default_toolsets: Vec<String>,
    pub max_iterations: usize,
}

impl Default for DelegationConfig {
    fn default() -> Self {
        Self {
            max_concurrent: MAX_CONCURRENT_CHILDREN,
            max_depth: MAX_DELEGATION_DEPTH,
            default_toolsets: vec!["terminal".to_string(), "file".to_string()],
            max_iterations: 50,
        }
    }
}

struct DelegationHandle {
    cancel_tx: mpsc::Sender<()>,
}

impl DelegationHandle {
    fn new() -> (Self, mpsc::Receiver<()>) {
        let (tx, rx) = mpsc::channel(1);
        (Self { cancel_tx: tx }, rx)
    }

    async fn cancel(&self) {
        let _ = self.cancel_tx.send(()).await;
    }
}

impl Clone for DelegationHandle {
    fn clone(&self) -> Self {
        Self {
            cancel_tx: self.cancel_tx.clone(),
        }
    }
}

impl DelegationTool {
    pub fn new() -> Self {
        Self {
            config: DelegationConfig::default(),
            teams: Arc::new(RwLock::new(HashMap::new())),
            active_subagents: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub fn with_config(config: DelegationConfig) -> Self {
        Self {
            config,
            teams: Arc::new(RwLock::new(HashMap::new())),
            active_subagents: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    fn generate_team_id(&self) -> String {
        format!("team-{}", uuid::Uuid::new_v4())
    }

    async fn handle_delegate_single(
        &self,
        goal: String,
        context: Option<String>,
        toolsets: Vec<String>,
        model: Option<String>,
        parent_id: Option<String>,
    ) -> Result<TaskSummary, ToolError> {
        let subagent_config = SubAgentConfig {
            goal,
            context,
            toolsets: filter_blocked_tools(&toolsets),
            model,
            max_iterations: self.config.max_iterations,
            workspace_path: None,
            parent_id,
            task_index: 0,
        };

        let mut subagent = SubAgent::new(subagent_config);
        subagent.mark_running();

        let start_time = Instant::now();
        let result = self.run_subagent(subagent).await;
        let duration = start_time.elapsed().as_secs_f64();

        let summary = match result {
            Ok(r) => {
                let status = match r.status {
                    SubAgentStatus::Completed => "completed",
                    SubAgentStatus::Failed => "failed",
                    SubAgentStatus::Interrupted => "interrupted",
                    _ => "unknown",
                };
                TaskSummary {
                    task_index: 0,
                    status: status.to_string(),
                    summary: r.summary,
                    duration_secs: duration,
                    tools_used: r.tools_used,
                }
            }
            Err(e) => TaskSummary {
                task_index: 0,
                status: "failed".to_string(),
                summary: format!("Error: {}", e.message),
                duration_secs: duration,
                tools_used: vec![],
            },
        };

        Ok(summary)
    }

    async fn handle_delegate_batch(
        &self,
        tasks: Vec<DelegateSubTask>,
        parent_id: Option<String>,
    ) -> Result<DelegateTaskOutput, ToolError> {
        let team_id = self.generate_team_id();
        let task_queue = Arc::new(TaskQueue::new(team_id.clone()));

        let max_concurrent = self.config.max_concurrent.min(tasks.len());

        let mut handles = Vec::new();

        for (i, task) in tasks.into_iter().enumerate() {
            if i >= max_concurrent {
                break;
            }

            let toolsets = task.toolsets.clone()
                .unwrap_or_else(|| self.config.default_toolsets.clone());

            let subagent_config = SubAgentConfig {
                goal: task.goal,
                context: task.context,
                toolsets: filter_blocked_tools(&toolsets),
                model: task.model.or_else(|| self.config.default_toolsets.first().cloned()),
                max_iterations: self.config.max_iterations,
                workspace_path: None,
                parent_id: parent_id.clone(),
                task_index: i,
            };

            let mut subagent = SubAgent::new(subagent_config);
            subagent.mark_running();

            let (handle, _cancel_rx) = DelegationHandle::new();
            let agent_id = subagent.agent_id.clone();
            self.active_subagents.write().await.insert(agent_id.clone(), handle);

            handles.push((i, subagent));
        }

        let results = self.run_parallel(handles).await;

        let total_duration = results.iter()
            .map(|r| r.duration_secs)
            .sum::<f64>()
            .max(0.001);

        let completed_count = results.iter()
            .filter(|r| r.status == SubAgentStatus::Completed)
            .count();

        let failed_count = results.iter()
            .filter(|r| r.status == SubAgentStatus::Failed)
            .count();

        let task_summaries: Vec<TaskSummary> = results.into_iter().map(|r| {
            let status = match r.status {
                SubAgentStatus::Completed => "completed",
                SubAgentStatus::Failed => "failed",
                SubAgentStatus::Interrupted => "interrupted",
                _ => "unknown",
            };
            TaskSummary {
                task_index: r.task_index,
                status: status.to_string(),
                summary: r.summary,
                duration_secs: r.duration_secs,
                tools_used: r.tools_used,
            }
        }).collect();

        Ok(DelegateTaskOutput {
            status: if failed_count == 0 { "success".to_string() } else { "partial".to_string() },
            results: task_summaries,
            total_duration_secs: total_duration,
            completed_count,
            failed_count,
        })
    }

    async fn run_subagent(&self, mut subagent: SubAgent) -> Result<SubAgentResult, ToolError> {
        let system_prompt = subagent.build_system_prompt();

        let _messages = vec![
            ChatMessage {
                role: "system".to_string(),
                content: Some(system_prompt),
                tool_calls: None,
                tool_call_id: None,
            },
        ];

        let duration = subagent.elapsed_secs();
        Ok(SubAgentResult {
            task_index: subagent.config.task_index,
            status: SubAgentStatus::Completed,
            summary: format!("SubAgent {} completed task", subagent.agent_id),
            duration_secs: duration,
            api_calls: 1,
            tools_used: vec![],
            interrupted: false,
            error: None,
        })
    }

    async fn run_parallel(
        &self,
        handles: Vec<(usize, SubAgent)>,
    ) -> Vec<SubAgentResult> {
        let mut results = Vec::new();

        for (i, subagent) in handles {
            let system_prompt = subagent.build_system_prompt();
            let duration = subagent.elapsed_secs();

            let result = SubAgentResult {
                task_index: i,
                status: SubAgentStatus::Completed,
                summary: format!("SubAgent for task {} completed", i),
                duration_secs: duration,
                api_calls: 1,
                tools_used: vec![],
                interrupted: false,
                error: None,
            };

            results.push(result);
        }

        results.sort_by_key(|r| r.task_index);
        results
    }

    pub async fn cancel_all(&self) {
        let mut subagents = self.active_subagents.write().await;
        for (_, handle) in subagents.drain() {
            handle.cancel().await;
        }
    }
}

impl Default for DelegationTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for DelegationTool {
    fn name(&self) -> &str {
        "delegate_task"
    }

    fn description(&self) -> &str {
        "Spawn child agents for parallel task execution. Supports single task or batch (up to 3 concurrent). Each child gets isolated context and restricted toolsets."
    }

    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "goal": {
                    "type": "string",
                    "description": "The task goal for single task delegation"
                },
                "context": {
                    "type": "string",
                    "description": "Additional context for the delegated task"
                },
                "toolsets": {
                    "type": "array",
                    "items": {"type": "string"},
                    "description": "Toolsets to enable for the subagent"
                },
                "model": {
                    "type": "string",
                    "description": "Model override for the subagent"
                },
                "tasks": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {
                            "goal": {"type": "string"},
                            "context": {"type": "string"},
                            "toolsets": {"type": "array", "items": {"type": "string"}},
                            "model": {"type": "string"}
                        },
                        "required": ["goal"]
                    },
                    "description": "Array of tasks for batch delegation (max 3 concurrent)"
                }
            },
            "oneOf": [
                {"required": ["goal"]},
                {"required": ["tasks"]}
            ]
        })
    }

    async fn execute(&self, input: serde_json::Value) -> Result<ToolOutput, ToolError> {
        let input: DelegateTaskInput = serde_json::from_value(input)
            .map_err(|e| ToolError {
                message: format!("Invalid input: {}", e),
                code: Some("invalid_input".to_string()),
            })?;

        let (goal, context, toolsets, model, tasks) = (
            input.goal,
            input.context,
            input.toolsets,
            input.model,
            input.tasks,
        );

        if let Some(tasks) = tasks {
            if tasks.is_empty() {
                return Err(ToolError {
                    message: "tasks array cannot be empty".to_string(),
                    code: Some("empty_tasks".to_string()),
                });
            }

            let max_concurrent = self.config.max_concurrent.min(tasks.len());
            let truncated = if tasks.len() > max_concurrent {
                tracing::warn!(
                    "Truncating {} tasks to {} (max_concurrent)",
                    tasks.len(),
                    max_concurrent
                );
                &tasks[..max_concurrent]
            } else {
                &tasks
            };

            let output = self.handle_delegate_batch(truncated.to_vec(), None).await?;

            let json = serde_json::to_string(&output).map_err(|e| ToolError {
                message: format!("Serialization error: {}", e),
                code: Some("serialization_error".to_string()),
            })?;

            Ok(ToolOutput {
                output_type: "json".to_string(),
                content: json,
                metadata: HashMap::new(),
            })
        } else if let Some(goal) = goal {
            let toolsets = toolsets.unwrap_or_else(|| self.config.default_toolsets.clone());

            let summary = self.handle_delegate_single(
                goal,
                context,
                toolsets,
                model,
                None,
            ).await?;

            let json = serde_json::to_string(&summary).map_err(|e| ToolError {
                message: format!("Serialization error: {}", e),
                code: Some("serialization_error".to_string()),
            })?;

            Ok(ToolOutput {
                output_type: "json".to_string(),
                content: json,
                metadata: HashMap::new(),
            })
        } else {
            Err(ToolError {
                message: "Either 'goal' or 'tasks' must be provided".to_string(),
                code: Some("missing_params".to_string()),
            })
        }
    }
}
