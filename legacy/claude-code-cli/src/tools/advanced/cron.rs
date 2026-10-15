//! Cron Tool - Scheduled recurring task management
//!
//! Manages scheduled recurring tasks with cron-like scheduling.
//! Based on claw-code-main's cron tool implementation.

use crate::tools::{Tool, ToolOutput, ToolError};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use chrono::{DateTime, Utc};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CronTask {
    pub cron_id: String,
    pub schedule: String,
    pub prompt: String,
    pub description: String,
    pub status: CronStatus,
    pub created_at: DateTime<Utc>,
    pub last_run: Option<DateTime<Utc>>,
    pub next_run: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum CronStatus {
    Active,
    Paused,
    Completed,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScheduledTask {
    pub id: String,
    pub cron_id: String,
    pub status: TaskRunStatus,
    pub started_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
    pub result: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum TaskRunStatus {
    Pending,
    Running,
    Completed,
    Failed,
}

pub struct CronTool {
    tasks: Arc<RwLock<HashMap<String, CronTask>>>,
    runs: Arc<RwLock<HashMap<String, ScheduledTask>>>,
}

impl Default for CronTool {
    fn default() -> Self {
        Self::new()
    }
}

impl CronTool {
    pub fn new() -> Self {
        Self {
            tasks: Arc::new(RwLock::new(HashMap::new())),
            runs: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    fn generate_id(&self) -> String {
        uuid::Uuid::new_v4().to_string()
    }

    fn parse_schedule(schedule: &str) -> Option<i64> {
        let parts: Vec<&str> = schedule.split_whitespace().collect();
        if parts.len() != 5 {
            return None;
        }

        let minutes = parts[0].parse::<i64>().ok()?;
        if minutes < 0 || minutes > 59 {
            return None;
        }

        Some(minutes * 60)
    }

    async fn handle_create(&self, input: &serde_json::Value) -> Result<CronTask, ToolError> {
        let schedule = input["schedule"].as_str()
            .ok_or_else(|| ToolError {
                message: "schedule is required".to_string(),
                code: Some("missing_schedule".to_string()),
            })?
            .to_string();

        let prompt = input["prompt"].as_str()
            .ok_or_else(|| ToolError {
                message: "prompt is required".to_string(),
                code: Some("missing_prompt".to_string()),
            })?
            .to_string();

        let description = input["description"].as_str()
            .unwrap_or("")
            .to_string();

        let interval_secs = Self::parse_schedule(&schedule)
            .ok_or_else(|| ToolError {
                message: "Invalid cron schedule format. Use: minute hour day month weekday".to_string(),
                code: Some("invalid_schedule".to_string()),
            })?;

        let now = Utc::now();
        let next_run = chrono::Duration::seconds(interval_secs);

        let task = CronTask {
            cron_id: self.generate_id(),
            schedule,
            prompt,
            description,
            status: CronStatus::Active,
            created_at: now,
            last_run: None,
            next_run: Some(now + next_run),
        };

        let mut tasks = self.tasks.write().await;
        tasks.insert(task.cron_id.clone(), task.clone());

        Ok(task)
    }

    async fn handle_delete(&self, cron_id: &str) -> Result<String, ToolError> {
        let mut tasks = self.tasks.write().await;
        let task = tasks.remove(cron_id)
            .ok_or_else(|| ToolError {
                message: format!("Cron task not found: {}", cron_id),
                code: Some("cron_not_found".to_string()),
            })?;

        Ok(format!("Cron task {} deleted", task.cron_id))
    }

    async fn handle_list(&self) -> Result<Vec<CronTask>, ToolError> {
        let tasks = self.tasks.read().await;
        let list: Vec<CronTask> = tasks.values().cloned().collect();
        Ok(list)
    }

    async fn handle_get(&self, cron_id: &str) -> Result<CronTask, ToolError> {
        let tasks = self.tasks.read().await;
        tasks.get(cron_id)
            .cloned()
            .ok_or_else(|| ToolError {
                message: format!("Cron task not found: {}", cron_id),
                code: Some("cron_not_found".to_string()),
            })
    }
}

#[async_trait]
impl Tool for CronTool {
    fn name(&self) -> &str {
        "cron"
    }

    fn description(&self) -> &str {
        "Manage scheduled recurring tasks with cron-like scheduling"
    }

    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "operation": {
                    "type": "string",
                    "enum": ["create", "delete", "list", "get"]
                },
                "cron_id": { "type": "string" },
                "schedule": { "type": "string" },
                "prompt": { "type": "string" },
                "description": { "type": "string" }
            },
            "required": ["operation"]
        })
    }

    async fn execute(&self, input: serde_json::Value) -> Result<ToolOutput, ToolError> {
        let operation = input["operation"].as_str()
            .ok_or_else(|| ToolError {
                message: "operation is required".to_string(),
                code: Some("missing_operation".to_string()),
            })?;

        let result = match operation {
            "create" => {
                let task = self.handle_create(&input).await?;
                serde_json::json!({
                    "success": true,
                    "cron_id": task.cron_id,
                    "schedule": task.schedule,
                    "next_run": task.next_run.map(|dt| dt.to_rfc3339())
                }).to_string()
            }
            "delete" => {
                let cron_id = input["cron_id"].as_str()
                    .ok_or_else(|| ToolError {
                        message: "cron_id is required".to_string(),
                        code: Some("missing_cron_id".to_string()),
                    })?;
                let result = self.handle_delete(cron_id).await?;
                serde_json::json!({
                    "success": true,
                    "result": result
                }).to_string()
            }
            "list" => {
                let tasks = self.handle_list().await?;
                serde_json::json!({
                    "success": true,
                    "tasks": tasks,
                    "count": tasks.len()
                }).to_string()
            }
            "get" => {
                let cron_id = input["cron_id"].as_str()
                    .ok_or_else(|| ToolError {
                        message: "cron_id is required".to_string(),
                        code: Some("missing_cron_id".to_string()),
                    })?;
                let task = self.handle_get(cron_id).await?;
                serde_json::json!({
                    "success": true,
                    "task": task
                }).to_string()
            }
            _ => return Err(ToolError {
                message: format!("Unknown operation: {}", operation),
                code: Some("invalid_operation".to_string()),
            }),
        };

        Ok(ToolOutput {
            output_type: "json".to_string(),
            content: result,
            metadata: HashMap::new(),
        })
    }
}