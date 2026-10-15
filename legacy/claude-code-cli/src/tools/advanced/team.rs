//! Team Tool - Multi-agent coordination
//!
//! Manages teams of sub-agents for parallel task execution.
//! Based on claw-code-main's team tool implementation.

use crate::tools::{Tool, ToolOutput, ToolError};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use chrono::{DateTime, Utc};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Team {
    pub team_id: String,
    pub name: String,
    pub tasks: Vec<TeamTask>,
    pub status: TeamStatus,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TeamTask {
    pub task_id: String,
    pub description: String,
    pub prompt: String,
    pub status: TaskStatus,
    pub result: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum TeamStatus {
    Active,
    Completed,
    Failed,
    Stopped,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum TaskStatus {
    Pending,
    Running,
    Completed,
    Failed,
}

pub struct TeamTool {
    teams: Arc<RwLock<HashMap<String, Team>>>,
}

impl Default for TeamTool {
    fn default() -> Self {
        Self::new()
    }
}

impl TeamTool {
    pub fn new() -> Self {
        Self {
            teams: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    fn generate_id(&self) -> String {
        uuid::Uuid::new_v4().to_string()
    }

    async fn handle_create(&self, input: &serde_json::Value) -> Result<Team, ToolError> {
        let name = input["name"].as_str()
            .ok_or_else(|| ToolError {
                message: "name is required".to_string(),
                code: Some("missing_name".to_string()),
            })?
            .to_string();

        let tasks_input = input["tasks"].as_array()
            .ok_or_else(|| ToolError {
                message: "tasks is required".to_string(),
                code: Some("missing_tasks".to_string()),
            })?;

        let tasks: Vec<TeamTask> = tasks_input.iter().enumerate().map(|(i, t)| {
            let empty_map = serde_json::Map::new();
            let obj = t.as_object().unwrap_or(&empty_map);
            TeamTask {
                task_id: format!("task-{}", i),
                description: obj.get("description").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                prompt: obj.get("prompt").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                status: TaskStatus::Pending,
                result: None,
            }
        }).collect();

        let now = Utc::now();
        let team = Team {
            team_id: self.generate_id(),
            name,
            tasks,
            status: TeamStatus::Active,
            created_at: now,
            updated_at: now,
        };

        let mut teams = self.teams.write().await;
        teams.insert(team.team_id.clone(), team.clone());

        Ok(team)
    }

    async fn handle_delete(&self, team_id: &str) -> Result<String, ToolError> {
        let mut teams = self.teams.write().await;
        let team = teams.get_mut(team_id)
            .ok_or_else(|| ToolError {
                message: format!("Team not found: {}", team_id),
                code: Some("team_not_found".to_string()),
            })?;

        team.status = TeamStatus::Stopped;
        teams.remove(team_id);

        Ok(format!("Team {} deleted", team_id))
    }

    async fn handle_list(&self) -> Result<Vec<Team>, ToolError> {
        let teams = self.teams.read().await;
        let list: Vec<Team> = teams.values().cloned().collect();
        Ok(list)
    }

    async fn handle_get(&self, team_id: &str) -> Result<Team, ToolError> {
        let teams = self.teams.read().await;
        teams.get(team_id)
            .cloned()
            .ok_or_else(|| ToolError {
                message: format!("Team not found: {}", team_id),
                code: Some("team_not_found".to_string()),
            })
    }
}

#[async_trait]
impl Tool for TeamTool {
    fn name(&self) -> &str {
        "team"
    }

    fn description(&self) -> &str {
        "Manage teams of sub-agents for parallel task execution"
    }

    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "operation": {
                    "type": "string",
                    "enum": ["create", "delete", "list", "get"]
                },
                "team_id": { "type": "string" },
                "name": { "type": "string" },
                "tasks": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {
                            "prompt": { "type": "string" },
                            "description": { "type": "string" }
                        },
                        "required": ["prompt"]
                    }
                }
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
                let team = self.handle_create(&input).await?;
                serde_json::json!({
                    "success": true,
                    "team_id": team.team_id,
                    "name": team.name,
                    "tasks_count": team.tasks.len()
                }).to_string()
            }
            "delete" => {
                let team_id = input["team_id"].as_str()
                    .ok_or_else(|| ToolError {
                        message: "team_id is required".to_string(),
                        code: Some("missing_team_id".to_string()),
                    })?;
                let result = self.handle_delete(team_id).await?;
                serde_json::json!({
                    "success": true,
                    "result": result
                }).to_string()
            }
            "list" => {
                let teams = self.handle_list().await?;
                serde_json::json!({
                    "success": true,
                    "teams": teams,
                    "count": teams.len()
                }).to_string()
            }
            "get" => {
                let team_id = input["team_id"].as_str()
                    .ok_or_else(|| ToolError {
                        message: "team_id is required".to_string(),
                        code: Some("missing_team_id".to_string()),
                    })?;
                let team = self.handle_get(team_id).await?;
                serde_json::json!({
                    "success": true,
                    "team": team
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