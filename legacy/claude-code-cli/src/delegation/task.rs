//! Task - Task queue and delegation result management
//!
//! Manages the task queue for batch delegation and collects results
//! from parallel sub-agents.

use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::RwLock;

use super::subagent::SubAgentResult;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DelegationTask {
    pub task_id: String,
    pub goal: String,
    pub context: Option<String>,
    pub toolsets: Vec<String>,
    pub model: Option<String>,
    pub status: TaskStatus,
    pub result: Option<String>,
    pub error: Option<String>,
}

impl DelegationTask {
    pub fn new(
        task_id: String,
        goal: String,
        context: Option<String>,
        toolsets: Vec<String>,
    ) -> Self {
        Self {
            task_id,
            goal,
            context,
            toolsets,
            model: None,
            status: TaskStatus::Pending,
            result: None,
            error: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum TaskStatus {
    Pending,
    Running,
    Completed,
    Failed,
    Interrupted,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DelegationResult {
    pub team_id: String,
    pub tasks: Vec<TaskResult>,
    pub total_duration_secs: f64,
    pub completed_count: usize,
    pub failed_count: usize,
    pub interrupted_count: usize,
}

impl DelegationResult {
    pub fn new(team_id: String, tasks: Vec<TaskResult>) -> Self {
        let completed_count = tasks.iter().filter(|t| t.status == TaskStatus::Completed).count();
        let failed_count = tasks.iter().filter(|t| t.status == TaskStatus::Failed).count();
        let interrupted_count = tasks.iter().filter(|t| t.status == TaskStatus::Interrupted).count();
        let total_duration_secs = tasks.iter()
            .map(|t| t.duration_secs)
            .sum();

        Self {
            team_id,
            tasks,
            total_duration_secs,
            completed_count,
            failed_count,
            interrupted_count,
        }
    }

    pub fn success(&self) -> bool {
        self.completed_count > 0 && self.failed_count == 0
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskResult {
    pub task_id: String,
    pub goal: String,
    pub status: TaskStatus,
    pub summary: String,
    pub duration_secs: f64,
    pub api_calls: usize,
    pub tools_used: Vec<String>,
    pub interrupted: bool,
    pub error: Option<String>,
}

impl From<(usize, SubAgentResult)> for TaskResult {
    fn from((index, result): (usize, SubAgentResult)) -> Self {
        let status = match result.status {
            super::subagent::SubAgentStatus::Completed => TaskStatus::Completed,
            super::subagent::SubAgentStatus::Failed => TaskStatus::Failed,
            super::subagent::SubAgentStatus::Interrupted => TaskStatus::Interrupted,
            _ => TaskStatus::Pending,
        };

        Self {
            task_id: format!("task-{}", index),
            goal: String::new(),
            status,
            summary: result.summary,
            duration_secs: result.duration_secs,
            api_calls: result.api_calls,
            tools_used: result.tools_used,
            interrupted: result.interrupted,
            error: result.error,
        }
    }
}

pub struct TaskQueue {
    team_id: String,
    tasks: Arc<RwLock<Vec<DelegationTask>>>,
    results: Arc<RwLock<Vec<TaskResult>>>,
    active_handles: Arc<RwLock<Vec<String>>>,
}

impl TaskQueue {
    pub fn new(team_id: String) -> Self {
        Self {
            team_id,
            tasks: Arc::new(RwLock::new(Vec::new())),
            results: Arc::new(RwLock::new(Vec::new())),
            active_handles: Arc::new(RwLock::new(Vec::new())),
        }
    }

    pub async fn add_task(&self, task: DelegationTask) {
        let mut tasks = self.tasks.write().await;
        tasks.push(task);
    }

    pub async fn add_tasks(&self, tasks: Vec<DelegationTask>) {
        let mut self_tasks = self.tasks.write().await;
        self_tasks.extend(tasks);
    }

    pub async fn get_pending_tasks(&self) -> Vec<DelegationTask> {
        let tasks = self.tasks.read().await;
        tasks.iter()
            .filter(|t| t.status == TaskStatus::Pending)
            .cloned()
            .collect()
    }

    pub async fn mark_running(&self, task_id: &str) {
        let mut tasks = self.tasks.write().await;
        if let Some(task) = tasks.iter_mut().find(|t| t.task_id == task_id) {
            task.status = TaskStatus::Running;
        }
    }

    pub async fn mark_completed(&self, task_id: &str, result: String) {
        let mut tasks = self.tasks.write().await;
        if let Some(task) = tasks.iter_mut().find(|t| t.task_id == task_id) {
            task.status = TaskStatus::Completed;
            task.result = Some(result);
        }
    }

    pub async fn mark_failed(&self, task_id: &str, error: String) {
        let mut tasks = self.tasks.write().await;
        if let Some(task) = tasks.iter_mut().find(|t| t.task_id == task_id) {
            task.status = TaskStatus::Failed;
            task.error = Some(error);
        }
    }

    pub async fn add_result(&self, result: TaskResult) {
        let mut results = self.results.write().await;
        results.push(result);
    }

    pub async fn get_results(&self) -> Vec<TaskResult> {
        let results = self.results.read().await;
        results.clone()
    }

    pub async fn register_handle(&self, agent_id: String) {
        let mut handles = self.active_handles.write().await;
        handles.push(agent_id);
    }

    pub async fn unregister_handle(&self, agent_id: &str) {
        let mut handles = self.active_handles.write().await;
        handles.retain(|h| h != agent_id);
    }

    pub async fn get_active_count(&self) -> usize {
        let handles = self.active_handles.read().await;
        handles.len()
    }

    pub async fn cancel_all(&self) {
        let handles = self.active_handles.read().await;
        for handle in handles.iter() {
            tracing::info!("Cancelling subagent: {}", handle);
        }
    }

    pub async fn finalize(self) -> DelegationResult {
        let tasks = self.tasks.read().await;
        let results = tasks.iter().enumerate().map(|(i, task)| {
            TaskResult {
                task_id: task.task_id.clone(),
                goal: task.goal.clone(),
                status: task.status.clone(),
                summary: task.result.clone().unwrap_or_default(),
                duration_secs: 0.0,
                api_calls: 0,
                tools_used: vec![],
                interrupted: task.status == TaskStatus::Interrupted,
                error: task.error.clone(),
            }
        }).collect();

        DelegationResult::new(self.team_id, results)
    }
}
