//! Plan Mode Tool - Strategic planning and thinking mode
//!
//! Provides planning mode operations for complex task decomposition and analysis.

use crate::tools::{Tool, ToolOutput, ToolError};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use chrono::{DateTime, Utc};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Plan {
    pub plan_id: String,
    pub task: String,
    pub steps: Vec<PlanStep>,
    pub status: PlanStatus,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub metadata: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanStep {
    pub step_id: String,
    pub description: String,
    pub status: StepStatus,
    pub dependencies: Vec<String>,
    pub result: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum PlanStatus {
    Draft,
    InProgress,
    Completed,
    Abandoned,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum StepStatus {
    Pending,
    Blocked,
    Ready,
    InProgress,
    Completed,
    Skipped,
}

pub struct PlanModeTool {
    plans: Arc<RwLock<HashMap<String, Plan>>>,
    current_plan: Arc<RwLock<Option<String>>>,
}

impl Default for PlanModeTool {
    fn default() -> Self {
        Self::new()
    }
}

impl PlanModeTool {
    pub fn new() -> Self {
        Self {
            plans: Arc::new(RwLock::new(HashMap::new())),
            current_plan: Arc::new(RwLock::new(None)),
        }
    }

    fn generate_id(prefix: &str) -> String {
        format!("{}-{}", prefix, uuid::Uuid::new_v4())
    }

    async fn handle_create(&self, input: &serde_json::Value) -> Result<Plan, ToolError> {
        let task = input["task"].as_str()
            .ok_or_else(|| ToolError {
                message: "task is required".to_string(),
                code: Some("missing_task".to_string()),
            })?
            .to_string();

        let steps: Vec<PlanStep> = input["steps"]
            .as_array()
            .map(|arr| {
                arr.iter().enumerate().map(|(i, s)| {
                    let empty_map = serde_json::Map::new();
                    let obj = s.as_object().unwrap_or(&empty_map);
                    PlanStep {
                        step_id: format!("step-{}", i),
                        description: obj.get("description")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string(),
                        status: StepStatus::Pending,
                        dependencies: obj.get("dependencies")
                            .and_then(|v| v.as_array())
                            .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
                            .unwrap_or_default(),
                        result: None,
                    }
                }).collect()
            })
            .unwrap_or_default();

        let now = Utc::now();
        let plan = Plan {
            plan_id: Self::generate_id("plan"),
            task,
            steps,
            status: PlanStatus::Draft,
            created_at: now,
            updated_at: now,
            metadata: HashMap::new(),
        };

        let mut plans = self.plans.write().await;
        plans.insert(plan.plan_id.clone(), plan.clone());

        Ok(plan)
    }

    async fn handle_enter(&self, plan_id: &str) -> Result<String, ToolError> {
        let mut plans = self.plans.write().await;
        let plan = plans.get_mut(plan_id)
            .ok_or_else(|| ToolError {
                message: format!("Plan not found: {}", plan_id),
                code: Some("plan_not_found".to_string()),
            })?;

        if plan.status == PlanStatus::Completed {
            return Err(ToolError {
                message: "Cannot enter a completed plan".to_string(),
                code: Some("plan_completed".to_string()),
            });
        }

        plan.status = PlanStatus::InProgress;
        plan.updated_at = Utc::now();

        drop(plans);

        let mut current = self.current_plan.write().await;
        *current = Some(plan_id.to_string());

        Ok(format!("Entered plan mode: {}", plan_id))
    }

    async fn handle_exit(&self) -> Result<String, ToolError> {
        let mut current = self.current_plan.write().await;
        let plan_id = current.take()
            .ok_or_else(|| ToolError {
                message: "Not in any plan".to_string(),
                code: Some("no_active_plan".to_string()),
            })?;

        let mut plans = self.plans.write().await;
        if let Some(plan) = plans.get_mut(&plan_id) {
            plan.status = PlanStatus::Completed;
            plan.updated_at = Utc::now();
        }

        Ok(format!("Exited plan: {}", plan_id))
    }

    async fn handle_get_current(&self) -> Result<Option<Plan>, ToolError> {
        let current = self.current_plan.read().await;
        match current.as_ref() {
            Some(plan_id) => {
                let plans = self.plans.read().await;
                Ok(plans.get(plan_id).cloned())
            }
            None => Ok(None),
        }
    }

    async fn handle_add_step(&self, plan_id: &str, input: &serde_json::Value) -> Result<PlanStep, ToolError> {
        let description = input["description"].as_str()
            .ok_or_else(|| ToolError {
                message: "description is required".to_string(),
                code: Some("missing_description".to_string()),
            })?;

        let dependencies: Vec<String> = input["dependencies"]
            .as_array()
            .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
            .unwrap_or_default();

        let step = PlanStep {
            step_id: Self::generate_id("step"),
            description: description.to_string(),
            status: StepStatus::Pending,
            dependencies,
            result: None,
        };

        let mut plans = self.plans.write().await;
        let plan = plans.get_mut(plan_id)
            .ok_or_else(|| ToolError {
                message: format!("Plan not found: {}", plan_id),
                code: Some("plan_not_found".to_string()),
            })?;

        plan.steps.push(step.clone());
        plan.updated_at = Utc::now();

        Ok(step)
    }

    async fn handle_update_step(&self, plan_id: &str, step_id: &str, input: &serde_json::Value) -> Result<PlanStep, ToolError> {
        let mut plans = self.plans.write().await;
        let plan = plans.get_mut(plan_id)
            .ok_or_else(|| ToolError {
                message: format!("Plan not found: {}", plan_id),
                code: Some("plan_not_found".to_string()),
            })?;

        let step = plan.steps.iter_mut()
            .find(|s| s.step_id == step_id)
            .ok_or_else(|| ToolError {
                message: format!("Step not found: {}", step_id),
                code: Some("step_not_found".to_string()),
            })?;

        if let Some(status_str) = input["status"].as_str() {
            step.status = match status_str {
                "pending" => StepStatus::Pending,
                "blocked" => StepStatus::Blocked,
                "ready" => StepStatus::Ready,
                "in_progress" => StepStatus::InProgress,
                "completed" => StepStatus::Completed,
                "skipped" => StepStatus::Skipped,
                _ => return Err(ToolError {
                    message: "Invalid status".to_string(),
                    code: Some("invalid_status".to_string()),
                }),
            };
        }

        if let Some(result) = input["result"].as_str() {
            step.result = Some(result.to_string());
        }

        plan.updated_at = Utc::now();

        Ok(step.clone())
    }

    async fn handle_list(&self) -> Result<Vec<Plan>, ToolError> {
        let plans = self.plans.read().await;
        let list: Vec<Plan> = plans.values().cloned().collect();
        Ok(list)
    }

    async fn handle_get(&self, plan_id: &str) -> Result<Plan, ToolError> {
        let plans = self.plans.read().await;
        plans.get(plan_id)
            .cloned()
            .ok_or_else(|| ToolError {
                message: format!("Plan not found: {}", plan_id),
                code: Some("plan_not_found".to_string()),
            })
    }

    async fn handle_abandon(&self, plan_id: &str) -> Result<String, ToolError> {
        let mut plans = self.plans.write().await;
        let plan = plans.get_mut(plan_id)
            .ok_or_else(|| ToolError {
                message: format!("Plan not found: {}", plan_id),
                code: Some("plan_not_found".to_string()),
            })?;

        plan.status = PlanStatus::Abandoned;
        plan.updated_at = Utc::now();

        drop(plans);

        let mut current = self.current_plan.write().await;
        if current.as_ref() == Some(&plan_id.to_string()) {
            current.take();
        }

        Ok(format!("Plan {} abandoned", plan_id))
    }

    async fn handle_decompose(&self, plan_id: &str, task: &str) -> Result<Vec<PlanStep>, ToolError> {
        let mut plans = self.plans.write().await;
        let plan = plans.get_mut(plan_id)
            .ok_or_else(|| ToolError {
                message: format!("Plan not found: {}", plan_id),
                code: Some("plan_not_found".to_string()),
            })?;

        let steps = vec![
            PlanStep {
                step_id: Self::generate_id("step"),
                description: format!("Analyze: {}", task),
                status: StepStatus::Ready,
                dependencies: vec![],
                result: None,
            },
            PlanStep {
                step_id: Self::generate_id("step"),
                description: format!("Research: {}", task),
                status: StepStatus::Pending,
                dependencies: vec![],
                result: None,
            },
            PlanStep {
                step_id: Self::generate_id("step"),
                description: format!("Implement: {}", task),
                status: StepStatus::Pending,
                dependencies: vec![],
                result: None,
            },
            PlanStep {
                step_id: Self::generate_id("step"),
                description: format!("Verify: {}", task),
                status: StepStatus::Pending,
                dependencies: vec![],
                result: None,
            },
        ];

        plan.steps.extend(steps.clone());
        plan.updated_at = Utc::now();

        Ok(steps)
    }
}

#[async_trait]
impl Tool for PlanModeTool {
    fn name(&self) -> &str {
        "plan_mode"
    }

    fn description(&self) -> &str {
        "Strategic planning mode for complex task decomposition and analysis"
    }

    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "operation": {
                    "type": "string",
                    "enum": ["create", "enter", "exit", "get_current", "add_step", "update_step", "list", "get", "abandon", "decompose"]
                },
                "plan_id": { "type": "string", "description": "Plan ID" },
                "step_id": { "type": "string", "description": "Step ID" },
                "task": { "type": "string", "description": "Task description" },
                "steps": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {
                            "description": { "type": "string" },
                            "dependencies": { "type": "array", "items": { "type": "string" } }
                        }
                    }
                },
                "description": { "type": "string", "description": "Step description" },
                "dependencies": { "type": "array", "items": { "type": "string" }, "description": "Step dependencies" },
                "status": {
                    "type": "string",
                    "enum": ["pending", "blocked", "ready", "in_progress", "completed", "skipped"],
                    "description": "Step status"
                },
                "result": { "type": "string", "description": "Step result" }
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
                let plan = self.handle_create(&input).await?;
                serde_json::json!({
                    "success": true,
                    "plan_id": plan.plan_id,
                    "status": format!("{:?}", plan.status)
                }).to_string()
            }
            "enter" => {
                let plan_id = input["plan_id"].as_str()
                    .ok_or_else(|| ToolError {
                        message: "plan_id is required".to_string(),
                        code: Some("missing_plan_id".to_string()),
                    })?;
                let result = self.handle_enter(plan_id).await?;
                serde_json::json!({
                    "success": true,
                    "result": result
                }).to_string()
            }
            "exit" => {
                let result = self.handle_exit().await?;
                serde_json::json!({
                    "success": true,
                    "result": result
                }).to_string()
            }
            "get_current" => {
                let plan = self.handle_get_current().await?;
                serde_json::json!({
                    "success": true,
                    "plan": plan
                }).to_string()
            }
            "add_step" => {
                let plan_id = input["plan_id"].as_str()
                    .ok_or_else(|| ToolError {
                        message: "plan_id is required".to_string(),
                        code: Some("missing_plan_id".to_string()),
                    })?;
                let step = self.handle_add_step(plan_id, &input).await?;
                serde_json::json!({
                    "success": true,
                    "step": step
                }).to_string()
            }
            "update_step" => {
                let plan_id = input["plan_id"].as_str()
                    .ok_or_else(|| ToolError {
                        message: "plan_id is required".to_string(),
                        code: Some("missing_plan_id".to_string()),
                    })?;
                let step_id = input["step_id"].as_str()
                    .ok_or_else(|| ToolError {
                        message: "step_id is required".to_string(),
                        code: Some("missing_step_id".to_string()),
                    })?;
                let step = self.handle_update_step(plan_id, step_id, &input).await?;
                serde_json::json!({
                    "success": true,
                    "step": step
                }).to_string()
            }
            "list" => {
                let plans = self.handle_list().await?;
                serde_json::json!({
                    "success": true,
                    "plans": plans,
                    "count": plans.len()
                }).to_string()
            }
            "get" => {
                let plan_id = input["plan_id"].as_str()
                    .ok_or_else(|| ToolError {
                        message: "plan_id is required".to_string(),
                        code: Some("missing_plan_id".to_string()),
                    })?;
                let plan = self.handle_get(plan_id).await?;
                serde_json::json!({
                    "success": true,
                    "plan": plan
                }).to_string()
            }
            "abandon" => {
                let plan_id = input["plan_id"].as_str()
                    .ok_or_else(|| ToolError {
                        message: "plan_id is required".to_string(),
                        code: Some("missing_plan_id".to_string()),
                    })?;
                let result = self.handle_abandon(plan_id).await?;
                serde_json::json!({
                    "success": true,
                    "result": result
                }).to_string()
            }
            "decompose" => {
                let plan_id = input["plan_id"].as_str()
                    .ok_or_else(|| ToolError {
                        message: "plan_id is required".to_string(),
                        code: Some("missing_plan_id".to_string()),
                    })?;
                let task = input["task"].as_str().unwrap_or("");
                let steps = self.handle_decompose(plan_id, task).await?;
                serde_json::json!({
                    "success": true,
                    "steps": steps
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