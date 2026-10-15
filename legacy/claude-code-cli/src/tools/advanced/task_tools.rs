//! Task Tools - Task management system
//!
//! Provides comprehensive task management: create, get, list, update, output, stop.

use crate::tools::{Tool, ToolOutput, ToolError};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use std::time::{SystemTime, UNIX_EPOCH};
use std::sync::OnceLock;

static TASK_STORE: OnceLock<SharedTaskStore> = OnceLock::new();

pub fn get_task_store() -> SharedTaskStore {
    TASK_STORE.get_or_init(create_shared_task_store).clone()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub status: TaskStatus,
    pub priority: TaskPriority,
    pub created_at: u64,
    pub updated_at: u64,
    pub output: Option<String>,
    pub metadata: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum TaskStatus {
    Pending,
    Running,
    Completed,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TaskPriority {
    Low,
    Medium,
    High,
    Critical,
}

impl std::fmt::Display for TaskPriority {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TaskPriority::Low => write!(f, "low"),
            TaskPriority::Medium => write!(f, "medium"),
            TaskPriority::High => write!(f, "high"),
            TaskPriority::Critical => write!(f, "critical"),
        }
    }
}

pub struct TaskStore {
    tasks: HashMap<String, Task>,
}

impl Default for TaskStore {
    fn default() -> Self {
        Self::new()
    }
}

impl TaskStore {
    pub fn new() -> Self {
        Self {
            tasks: HashMap::new(),
        }
    }

    pub fn generate_id() -> String {
        format!("task_{}", SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis())
    }

    pub fn create_task(&mut self, name: String, description: Option<String>, priority: TaskPriority) -> Task {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let task = Task {
            id: Self::generate_id(),
            name,
            description,
            status: TaskStatus::Pending,
            priority,
            created_at: now,
            updated_at: now,
            output: None,
            metadata: HashMap::new(),
        };

        self.tasks.insert(task.id.clone(), task.clone());
        task
    }

    pub fn get_task(&self, id: &str) -> Option<&Task> {
        self.tasks.get(id)
    }

    pub fn list_tasks(&self, status: Option<TaskStatus>) -> Vec<&Task> {
        match status {
            Some(s) => self.tasks.values().filter(|t| t.status == s).collect(),
            None => self.tasks.values().collect(),
        }
    }

    pub fn update_task(&mut self, id: &str, status: Option<TaskStatus>, output: Option<String>) -> Option<&Task> {
        if let Some(task) = self.tasks.get_mut(id) {
            if let Some(s) = status {
                task.status = s;
            }
            if let Some(o) = output {
                task.output = Some(o);
            }
            task.updated_at = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs();
            return Some(&self.tasks[id]);
        }
        None
    }

    pub fn delete_task(&mut self, id: &str) -> bool {
        self.tasks.remove(id).is_some()
    }
}

pub type SharedTaskStore = Arc<RwLock<TaskStore>>;

pub fn create_shared_task_store() -> SharedTaskStore {
    Arc::new(RwLock::new(TaskStore::new()))
}

#[derive(Debug, Clone)]
pub struct TaskCreateTool;

impl TaskCreateTool {
    pub fn new() -> Self {
        Self
    }
}

impl Default for TaskCreateTool {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Deserialize)]
struct TaskInput {
    name: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    priority: Option<String>,
}

#[async_trait]
impl Tool for TaskCreateTool {
    fn name(&self) -> &str {
        "TaskCreate"
    }

    fn description(&self) -> &str {
        "Create a new task with name, optional description and priority"
    }

    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "name": {
                    "type": "string",
                    "description": "Task name"
                },
                "description": {
                    "type": "string",
                    "description": "Task description"
                },
                "priority": {
                    "type": "string",
                    "enum": ["low", "medium", "high", "critical"],
                    "description": "Task priority"
                }
            },
            "required": ["name"]
        })
    }

    async fn execute(&self, input: serde_json::Value) -> Result<ToolOutput, ToolError> {
        let input: TaskInput = serde_json::from_value(input)
            .map_err(|e| ToolError {
                message: format!("Invalid input: {}", e),
                code: Some("invalid_input".to_string()),
            })?;

        let priority = match input.priority.as_deref() {
            Some("high") => TaskPriority::High,
            Some("critical") => TaskPriority::Critical,
            Some("low") => TaskPriority::Low,
            _ => TaskPriority::Medium,
        };

        let store = get_task_store();
        let mut store = store.write().await;
        let task = store.create_task(input.name, input.description, priority);

        Ok(ToolOutput {
            output_type: "json".to_string(),
            content: serde_json::to_string_pretty(&task).unwrap(),
            metadata: HashMap::new(),
        })
    }
}

#[derive(Debug, Clone)]
pub struct TaskGetTool;

impl TaskGetTool {
    pub fn new() -> Self {
        Self
    }
}

impl Default for TaskGetTool {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Deserialize)]
struct TaskGetInput {
    id: String,
}

#[async_trait]
impl Tool for TaskGetTool {
    fn name(&self) -> &str {
        "TaskGet"
    }

    fn description(&self) -> &str {
        "Get task details by ID"
    }

    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "id": {
                    "type": "string",
                    "description": "Task ID"
                }
            },
            "required": ["id"]
        })
    }

    async fn execute(&self, input: serde_json::Value) -> Result<ToolOutput, ToolError> {
        let input: TaskGetInput = serde_json::from_value(input)
            .map_err(|e| ToolError {
                message: format!("Invalid input: {}", e),
                code: Some("invalid_input".to_string()),
            })?;

        let store = get_task_store();
        let store = store.read().await;
        let task = store.get_task(&input.id)
            .ok_or_else(|| ToolError {
                message: format!("Task not found: {}", input.id),
                code: Some("not_found".to_string()),
            })?;

        Ok(ToolOutput {
            output_type: "json".to_string(),
            content: serde_json::to_string_pretty(&task).unwrap(),
            metadata: HashMap::new(),
        })
    }
}

#[derive(Debug, Clone)]
pub struct TaskListTool;

impl TaskListTool {
    pub fn new() -> Self {
        Self
    }
}

impl Default for TaskListTool {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Deserialize)]
struct TaskListInput {
    #[serde(default)]
    status: Option<String>,
}

#[async_trait]
impl Tool for TaskListTool {
    fn name(&self) -> &str {
        "TaskList"
    }

    fn description(&self) -> &str {
        "List all tasks, optionally filter by status"
    }

    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "status": {
                    "type": "string",
                    "enum": ["pending", "running", "completed", "failed", "cancelled"],
                    "description": "Filter by status"
                }
            }
        })
    }

    async fn execute(&self, input: serde_json::Value) -> Result<ToolOutput, ToolError> {
        let input: TaskListInput = serde_json::from_value(input)
            .map_err(|e| ToolError {
                message: format!("Invalid input: {}", e),
                code: Some("invalid_input".to_string()),
            })?;

        let status_filter = match input.status.as_deref() {
            Some("pending") => Some(TaskStatus::Pending),
            Some("running") => Some(TaskStatus::Running),
            Some("completed") => Some(TaskStatus::Completed),
            Some("failed") => Some(TaskStatus::Failed),
            Some("cancelled") => Some(TaskStatus::Cancelled),
            _ => None,
        };

        let store = get_task_store();
        let store = store.read().await;
        let tasks = store.list_tasks(status_filter);

        let task_summaries: Vec<serde_json::Value> = tasks.iter().map(|t| {
            serde_json::json!({
                "id": t.id,
                "name": t.name,
                "status": format!("{:?}", t.status),
                "priority": t.priority,
                "created_at": t.created_at,
                "updated_at": t.updated_at,
            })
        }).collect();

        Ok(ToolOutput {
            output_type: "json".to_string(),
            content: serde_json::to_string_pretty(&task_summaries).unwrap(),
            metadata: HashMap::new(),
        })
    }
}

#[derive(Debug, Clone)]
pub struct TaskUpdateTool;

impl TaskUpdateTool {
    pub fn new() -> Self {
        Self
    }
}

impl Default for TaskUpdateTool {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Deserialize)]
struct TaskUpdateInput {
    id: String,
    #[serde(default)]
    status: Option<String>,
    #[serde(default)]
    output: Option<String>,
}

#[async_trait]
impl Tool for TaskUpdateTool {
    fn name(&self) -> &str {
        "TaskUpdate"
    }

    fn description(&self) -> &str {
        "Update task status or output"
    }

    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "id": {
                    "type": "string",
                    "description": "Task ID"
                },
                "status": {
                    "type": "string",
                    "enum": ["pending", "running", "completed", "failed", "cancelled"],
                    "description": "New status"
                },
                "output": {
                    "type": "string",
                    "description": "Task output"
                }
            },
            "required": ["id"]
        })
    }

    async fn execute(&self, input: serde_json::Value) -> Result<ToolOutput, ToolError> {
        let input: TaskUpdateInput = serde_json::from_value(input)
            .map_err(|e| ToolError {
                message: format!("Invalid input: {}", e),
                code: Some("invalid_input".to_string()),
            })?;

        let status = match input.status.as_deref() {
            Some("pending") => Some(TaskStatus::Pending),
            Some("running") => Some(TaskStatus::Running),
            Some("completed") => Some(TaskStatus::Completed),
            Some("failed") => Some(TaskStatus::Failed),
            Some("cancelled") => Some(TaskStatus::Cancelled),
            _ => None,
        };

        let store = get_task_store();
        let mut store = store.write().await;
        let task = store.update_task(&input.id, status, input.output)
            .ok_or_else(|| ToolError {
                message: format!("Task not found: {}", input.id),
                code: Some("not_found".to_string()),
            })?;

        Ok(ToolOutput {
            output_type: "json".to_string(),
            content: serde_json::to_string_pretty(&task).unwrap(),
            metadata: HashMap::new(),
        })
    }
}

#[derive(Debug, Clone)]
pub struct TaskOutputTool;

impl TaskOutputTool {
    pub fn new() -> Self {
        Self
    }
}

impl Default for TaskOutputTool {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Deserialize)]
struct TaskOutputInput {
    id: String,
}

#[async_trait]
impl Tool for TaskOutputTool {
    fn name(&self) -> &str {
        "TaskOutput"
    }

    fn description(&self) -> &str {
        "Get task output by ID"
    }

    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "id": {
                    "type": "string",
                    "description": "Task ID"
                }
            },
            "required": ["id"]
        })
    }

    async fn execute(&self, input: serde_json::Value) -> Result<ToolOutput, ToolError> {
        let input: TaskOutputInput = serde_json::from_value(input)
            .map_err(|e| ToolError {
                message: format!("Invalid input: {}", e),
                code: Some("invalid_input".to_string()),
            })?;

        let store = get_task_store();
        let store = store.read().await;
        let task = store.get_task(&input.id)
            .ok_or_else(|| ToolError {
                message: format!("Task not found: {}", input.id),
                code: Some("not_found".to_string()),
            })?;

        let output = task.output.clone().unwrap_or_default();

        Ok(ToolOutput {
            output_type: "text".to_string(),
            content: output,
            metadata: HashMap::new(),
        })
    }
}

#[derive(Debug, Clone)]
pub struct TaskStopTool;

impl TaskStopTool {
    pub fn new() -> Self {
        Self
    }
}

impl Default for TaskStopTool {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Deserialize)]
struct TaskStopInput {
    id: String,
}

#[async_trait]
impl Tool for TaskStopTool {
    fn name(&self) -> &str {
        "TaskStop"
    }

    fn description(&self) -> &str {
        "Stop/cancel a running task"
    }

    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "id": {
                    "type": "string",
                    "description": "Task ID"
                }
            },
            "required": ["id"]
        })
    }

    async fn execute(&self, input: serde_json::Value) -> Result<ToolOutput, ToolError> {
        let input: TaskStopInput = serde_json::from_value(input)
            .map_err(|e| ToolError {
                message: format!("Invalid input: {}", e),
                code: Some("invalid_input".to_string()),
            })?;

        let store = get_task_store();
        let mut store = store.write().await;
        let task = store.update_task(&input.id, Some(TaskStatus::Cancelled), None)
            .ok_or_else(|| ToolError {
                message: format!("Task not found: {}", input.id),
                code: Some("not_found".to_string()),
            })?;

        Ok(ToolOutput {
            output_type: "json".to_string(),
            content: serde_json::to_string_pretty(&serde_json::json!({
                "message": "Task stopped successfully",
                "task_id": task.id
            })).unwrap(),
            metadata: HashMap::new(),
        })
    }
}