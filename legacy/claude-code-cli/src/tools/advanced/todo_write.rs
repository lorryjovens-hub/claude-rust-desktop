//! TodoWrite Tool - Manage todo lists with simple operations
//!
//! Provides lightweight todo list management for tracking tasks and action items.

use crate::tools::{Tool, ToolOutput, ToolError};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use chrono::{DateTime, Utc};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Todo {
    pub id: String,
    pub content: String,
    pub status: TodoStatus,
    pub priority: TodoPriority,
    pub created_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum TodoStatus {
    Pending,
    InProgress,
    Completed,
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum TodoPriority {
    Low,
    Medium,
    High,
    Urgent,
}

impl std::str::FromStr for TodoPriority {
    type Err = ();
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "low" => Ok(TodoPriority::Low),
            "medium" => Ok(TodoPriority::Medium),
            "high" => Ok(TodoPriority::High),
            "urgent" => Ok(TodoPriority::Urgent),
            _ => Err(()),
        }
    }
}

pub struct TodoWriteTool {
    todos: Arc<RwLock<HashMap<String, Todo>>>,
}

impl Default for TodoWriteTool {
    fn default() -> Self {
        Self::new()
    }
}

impl TodoWriteTool {
    pub fn new() -> Self {
        Self {
            todos: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    fn generate_id(&self) -> String {
        format!("todo-{}", uuid::Uuid::new_v4())
    }

    async fn handle_create(&self, input: &serde_json::Value) -> Result<Todo, ToolError> {
        let content = input["content"].as_str()
            .ok_or_else(|| ToolError {
                message: "content is required".to_string(),
                code: Some("missing_content".to_string()),
            })?
            .to_string();

        let priority_str = input["priority"].as_str().unwrap_or("medium");
        let priority: TodoPriority = priority_str.parse()
            .map_err(|_| ToolError {
                message: "Invalid priority. Must be: low, medium, high, urgent".to_string(),
                code: Some("invalid_priority".to_string()),
            })?;

        let tags: Vec<String> = input["tags"]
            .as_array()
            .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
            .unwrap_or_default();

        let id = self.generate_id();
        let todo = Todo {
            id: id.clone(),
            content,
            status: TodoStatus::Pending,
            priority,
            created_at: Utc::now(),
            completed_at: None,
            tags,
        };

        let mut todos = self.todos.write().await;
        todos.insert(id, todo.clone());

        Ok(todo)
    }

    async fn handle_list(&self) -> Result<Vec<Todo>, ToolError> {
        let todos = self.todos.read().await;
        let list: Vec<Todo> = todos.values().cloned().collect();
        Ok(list)
    }

    async fn handle_get(&self, id: &str) -> Result<Todo, ToolError> {
        let todos = self.todos.read().await;
        todos.get(id)
            .cloned()
            .ok_or_else(|| ToolError {
                message: format!("Todo not found: {}", id),
                code: Some("todo_not_found".to_string()),
            })
    }

    async fn handle_update(&self, id: &str, input: &serde_json::Value) -> Result<Todo, ToolError> {
        let mut todos = self.todos.write().await;
        let todo = todos.get_mut(id)
            .ok_or_else(|| ToolError {
                message: format!("Todo not found: {}", id),
                code: Some("todo_not_found".to_string()),
            })?;

        if let Some(content) = input["content"].as_str() {
            todo.content = content.to_string();
        }

        if let Some(priority_str) = input["priority"].as_str() {
            todo.priority = priority_str.parse()
                .map_err(|_| ToolError {
                    message: "Invalid priority".to_string(),
                    code: Some("invalid_priority".to_string()),
                })?;
        }

        if let Some(status_str) = input["status"].as_str() {
            match status_str {
                "pending" => {
                    todo.status = TodoStatus::Pending;
                    todo.completed_at = None;
                }
                "in_progress" => {
                    todo.status = TodoStatus::InProgress;
                    todo.completed_at = None;
                }
                "completed" => {
                    todo.status = TodoStatus::Completed;
                    todo.completed_at = Some(Utc::now());
                }
                "cancelled" => {
                    todo.status = TodoStatus::Cancelled;
                    todo.completed_at = None;
                }
                _ => return Err(ToolError {
                    message: "Invalid status".to_string(),
                    code: Some("invalid_status".to_string()),
                }),
            }
        }

        if let Some(tags_array) = input["tags"].as_array() {
            todo.tags = tags_array.iter().filter_map(|v| v.as_str().map(String::from)).collect();
        }

        Ok(todo.clone())
    }

    async fn handle_delete(&self, id: &str) -> Result<String, ToolError> {
        let mut todos = self.todos.write().await;
        todos.remove(id)
            .ok_or_else(|| ToolError {
                message: format!("Todo not found: {}", id),
                code: Some("todo_not_found".to_string()),
            })?;

        Ok(format!("Todo {} deleted", id))
    }

    async fn handle_complete(&self, id: &str) -> Result<Todo, ToolError> {
        let mut todos = self.todos.write().await;
        let todo = todos.get_mut(id)
            .ok_or_else(|| ToolError {
                message: format!("Todo not found: {}", id),
                code: Some("todo_not_found".to_string()),
            })?;

        todo.status = TodoStatus::Completed;
        todo.completed_at = Some(Utc::now());

        Ok(todo.clone())
    }

    async fn handle_filter(&self, status: Option<&str>, priority: Option<&str>, tags: Option<Vec<String>>) -> Result<Vec<Todo>, ToolError> {
        let todos = self.todos.read().await;

        let filtered: Vec<Todo> = todos.values()
            .filter(|todo| {
                if let Some(status_filter) = status {
                    let todo_status = match todo.status {
                        TodoStatus::Pending => "pending",
                        TodoStatus::InProgress => "in_progress",
                        TodoStatus::Completed => "completed",
                        TodoStatus::Cancelled => "cancelled",
                    };
                    if todo_status != status_filter {
                        return false;
                    }
                }

                if let Some(priority_filter) = priority {
                    let todo_priority = match todo.priority {
                        TodoPriority::Low => "low",
                        TodoPriority::Medium => "medium",
                        TodoPriority::High => "high",
                        TodoPriority::Urgent => "urgent",
                    };
                    if todo_priority != priority_filter {
                        return false;
                    }
                }

                if let Some(ref filter_tags) = tags {
                    if !filter_tags.iter().all(|tag| todo.tags.contains(tag)) {
                        return false;
                    }
                }

                true
            })
            .cloned()
            .collect();

        Ok(filtered)
    }
}

#[async_trait]
impl Tool for TodoWriteTool {
    fn name(&self) -> &str {
        "todo_write"
    }

    fn description(&self) -> &str {
        "Manage todo lists for tracking tasks and action items"
    }

    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "operation": {
                    "type": "string",
                    "enum": ["create", "list", "get", "update", "delete", "complete", "filter"]
                },
                "id": { "type": "string", "description": "Todo ID" },
                "content": { "type": "string", "description": "Todo content" },
                "priority": {
                    "type": "string",
                    "enum": ["low", "medium", "high", "urgent"],
                    "description": "Todo priority"
                },
                "status": {
                    "type": "string",
                    "enum": ["pending", "in_progress", "completed", "cancelled"],
                    "description": "Todo status"
                },
                "tags": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Todo tags"
                },
                "filter_status": { "type": "string", "description": "Filter by status" },
                "filter_priority": { "type": "string", "description": "Filter by priority" },
                "filter_tags": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Filter by tags"
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
                let todo = self.handle_create(&input).await?;
                serde_json::json!({
                    "success": true,
                    "id": todo.id,
                    "todo": todo
                }).to_string()
            }
            "list" => {
                let todos = self.handle_list().await?;
                serde_json::json!({
                    "success": true,
                    "todos": todos,
                    "count": todos.len()
                }).to_string()
            }
            "get" => {
                let id = input["id"].as_str()
                    .ok_or_else(|| ToolError {
                        message: "id is required".to_string(),
                        code: Some("missing_id".to_string()),
                    })?;
                let todo = self.handle_get(id).await?;
                serde_json::json!({
                    "success": true,
                    "todo": todo
                }).to_string()
            }
            "update" => {
                let id = input["id"].as_str()
                    .ok_or_else(|| ToolError {
                        message: "id is required".to_string(),
                        code: Some("missing_id".to_string()),
                    })?;
                let todo = self.handle_update(id, &input).await?;
                serde_json::json!({
                    "success": true,
                    "todo": todo
                }).to_string()
            }
            "delete" => {
                let id = input["id"].as_str()
                    .ok_or_else(|| ToolError {
                        message: "id is required".to_string(),
                        code: Some("missing_id".to_string()),
                    })?;
                let result = self.handle_delete(id).await?;
                serde_json::json!({
                    "success": true,
                    "result": result
                }).to_string()
            }
            "complete" => {
                let id = input["id"].as_str()
                    .ok_or_else(|| ToolError {
                        message: "id is required".to_string(),
                        code: Some("missing_id".to_string()),
                    })?;
                let todo = self.handle_complete(id).await?;
                serde_json::json!({
                    "success": true,
                    "todo": todo
                }).to_string()
            }
            "filter" => {
                let status = input["filter_status"].as_str();
                let priority = input["filter_priority"].as_str();
                let tags = input["filter_tags"].as_array()
                    .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect());
                let todos = self.handle_filter(status, priority, tags).await?;
                serde_json::json!({
                    "success": true,
                    "todos": todos,
                    "count": todos.len()
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