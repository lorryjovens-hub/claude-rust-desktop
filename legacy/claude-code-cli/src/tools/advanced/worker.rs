//! Worker Tool - Agent boot lifecycle management
//!
//! Manages worker boot sessions with trust-gate and prompt-delivery guards.
//! Based on claw-code-main's worker tool implementation.

use crate::tools::{Tool, ToolOutput, ToolError};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use chrono::{DateTime, Utc};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerState {
    pub worker_id: String,
    pub status: WorkerStatus,
    pub cwd: String,
    pub trusted_roots: Vec<String>,
    pub auto_recover_prompt_misdelivery: bool,
    pub last_error: Option<String>,
    pub created_at: DateTime<Utc>,
    pub ready_handshake: bool,
    pub ready_for_prompt: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum WorkerStatus {
    Initializing,
    WaitingForTrust,
    TrustResolved,
    Ready,
    Running,
    Failed,
    Terminated,
}

pub struct WorkerTool {
    workers: Arc<RwLock<HashMap<String, WorkerState>>>,
}

impl Default for WorkerTool {
    fn default() -> Self {
        Self::new()
    }
}

impl WorkerTool {
    pub fn new() -> Self {
        Self {
            workers: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    fn generate_worker_id(&self) -> String {
        format!("worker-{}", uuid::Uuid::new_v4())
    }

    async fn handle_create(&self, input: &serde_json::Value) -> Result<WorkerState, ToolError> {
        let cwd = input["cwd"].as_str()
            .ok_or_else(|| ToolError {
                message: "cwd is required".to_string(),
                code: Some("missing_cwd".to_string()),
            })?
            .to_string();

        let trusted_roots: Vec<String> = input["trusted_roots"]
            .as_array()
            .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
            .unwrap_or_default();

        let auto_recover = input["auto_recover_prompt_misdelivery"]
            .as_bool()
            .unwrap_or(false);

        let worker_id = self.generate_worker_id();
        let worker = WorkerState {
            worker_id: worker_id.clone(),
            status: WorkerStatus::Initializing,
            cwd,
            trusted_roots,
            auto_recover_prompt_misdelivery: auto_recover,
            last_error: None,
            created_at: Utc::now(),
            ready_handshake: false,
            ready_for_prompt: false,
        };

        let mut workers = self.workers.write().await;
        workers.insert(worker_id.clone(), worker.clone());

        Ok(worker)
    }

    async fn handle_get(&self, worker_id: &str) -> Result<WorkerState, ToolError> {
        let workers = self.workers.read().await;
        workers.get(worker_id)
            .cloned()
            .ok_or_else(|| ToolError {
                message: format!("Worker not found: {}", worker_id),
                code: Some("worker_not_found".to_string()),
            })
    }

    async fn handle_observe(&self, worker_id: &str, screen_text: &str) -> Result<String, ToolError> {
        let mut workers = self.workers.write().await;
        let worker = workers.get_mut(worker_id)
            .ok_or_else(|| ToolError {
                message: format!("Worker not found: {}", worker_id),
                code: Some("worker_not_found".to_string()),
            })?;

        let detected = if screen_text.to_lowercase().contains("trust") || screen_text.contains("?") {
            worker.status = WorkerStatus::WaitingForTrust;
            "trust_prompt_detected"
        } else if screen_text.contains("ready") || screen_text.contains("boot") {
            worker.ready_handshake = true;
            worker.ready_for_prompt = true;
            worker.status = WorkerStatus::Ready;
            "ready_handshake_detected"
        } else {
            "no_action_required"
        };

        Ok(detected.to_string())
    }

    async fn handle_resolve_trust(&self, worker_id: &str) -> Result<WorkerState, ToolError> {
        let mut workers = self.workers.write().await;
        let worker = workers.get_mut(worker_id)
            .ok_or_else(|| ToolError {
                message: format!("Worker not found: {}", worker_id),
                code: Some("worker_not_found".to_string()),
            })?;

        worker.status = WorkerStatus::TrustResolved;
        worker.ready_handshake = true;

        Ok(worker.clone())
    }

    async fn handle_await_ready(&self, worker_id: &str) -> Result<String, ToolError> {
        let workers = self.workers.read().await;
        let worker = workers.get(worker_id)
            .ok_or_else(|| ToolError {
                message: format!("Worker not found: {}", worker_id),
                code: Some("worker_not_found".to_string()),
            })?;

        let verdict = if worker.ready_for_prompt {
            "ready_for_prompt"
        } else {
            "not_ready"
        };

        Ok(verdict.to_string())
    }

    async fn handle_send_prompt(&self, worker_id: &str, prompt: &str) -> Result<String, ToolError> {
        let mut workers = self.workers.write().await;
        let worker = workers.get_mut(worker_id)
            .ok_or_else(|| ToolError {
                message: format!("Worker not found: {}", worker_id),
                code: Some("worker_not_found".to_string()),
            })?;

        if !worker.ready_for_prompt {
            return Err(ToolError {
                message: "Worker not ready for prompt".to_string(),
                code: Some("worker_not_ready".to_string()),
            });
        }

        worker.status = WorkerStatus::Running;
        Ok(format!("Prompt delivered to worker {}", worker_id))
    }

    async fn handle_restart(&self, worker_id: &str) -> Result<WorkerState, ToolError> {
        let mut workers = self.workers.write().await;
        let worker = workers.get_mut(worker_id)
            .ok_or_else(|| ToolError {
                message: format!("Worker not found: {}", worker_id),
                code: Some("worker_not_found".to_string()),
            })?;

        worker.status = WorkerStatus::Initializing;
        worker.ready_handshake = false;
        worker.ready_for_prompt = false;
        worker.last_error = None;

        Ok(worker.clone())
    }

    async fn handle_terminate(&self, worker_id: &str) -> Result<String, ToolError> {
        let mut workers = self.workers.write().await;
        let worker = workers.get_mut(worker_id)
            .ok_or_else(|| ToolError {
                message: format!("Worker not found: {}", worker_id),
                code: Some("worker_not_found".to_string()),
            })?;

        worker.status = WorkerStatus::Terminated;
        Ok(format!("Worker {} terminated", worker_id))
    }

    async fn handle_observe_completion(&self, worker_id: &str, finish_reason: &str, tokens_output: i64) -> Result<String, ToolError> {
        let mut workers = self.workers.write().await;
        let worker = workers.get_mut(worker_id)
            .ok_or_else(|| ToolError {
                message: format!("Worker not found: {}", worker_id),
                code: Some("worker_not_found".to_string()),
            })?;

        if finish_reason == "finished" {
            worker.status = WorkerStatus::Ready;
        } else {
            worker.status = WorkerStatus::Failed;
            worker.last_error = Some(format!("Provider degraded: {}", finish_reason));
        }

        Ok(format!("Completion reported: {} tokens output", tokens_output))
    }
}

#[async_trait]
impl Tool for WorkerTool {
    fn name(&self) -> &str {
        "worker"
    }

    fn description(&self) -> &str {
        "Manage worker boot sessions with trust-gate and prompt-delivery guards"
    }

    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "operation": {
                    "type": "string",
                    "enum": ["create", "get", "observe", "resolve_trust", "await_ready", "send_prompt", "restart", "terminate", "observe_completion"]
                },
                "worker_id": { "type": "string" },
                "cwd": { "type": "string" },
                "trusted_roots": { "type": "array", "items": { "type": "string" } },
                "auto_recover_prompt_misdelivery": { "type": "boolean" },
                "screen_text": { "type": "string" },
                "prompt": { "type": "string" },
                "finish_reason": { "type": "string" },
                "tokens_output": { "type": "integer", "minimum": 0 }
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
                let worker = self.handle_create(&input).await?;
                serde_json::json!({
                    "success": true,
                    "worker_id": worker.worker_id,
                    "status": format!("{:?}", worker.status)
                }).to_string()
            }
            "get" => {
                let worker_id = input["worker_id"].as_str()
                    .ok_or_else(|| ToolError {
                        message: "worker_id is required".to_string(),
                        code: Some("missing_worker_id".to_string()),
                    })?;
                let worker = self.handle_get(worker_id).await?;
                serde_json::json!({
                    "success": true,
                    "worker": worker
                }).to_string()
            }
            "observe" => {
                let worker_id = input["worker_id"].as_str()
                    .ok_or_else(|| ToolError {
                        message: "worker_id is required".to_string(),
                        code: Some("missing_worker_id".to_string()),
                    })?;
                let screen_text = input["screen_text"].as_str().unwrap_or("");
                let result = self.handle_observe(worker_id, screen_text).await?;
                serde_json::json!({
                    "success": true,
                    "result": result
                }).to_string()
            }
            "resolve_trust" => {
                let worker_id = input["worker_id"].as_str()
                    .ok_or_else(|| ToolError {
                        message: "worker_id is required".to_string(),
                        code: Some("missing_worker_id".to_string()),
                    })?;
                let worker = self.handle_resolve_trust(worker_id).await?;
                serde_json::json!({
                    "success": true,
                    "worker": worker
                }).to_string()
            }
            "await_ready" => {
                let worker_id = input["worker_id"].as_str()
                    .ok_or_else(|| ToolError {
                        message: "worker_id is required".to_string(),
                        code: Some("missing_worker_id".to_string()),
                    })?;
                let verdict = self.handle_await_ready(worker_id).await?;
                serde_json::json!({
                    "success": true,
                    "verdict": verdict
                }).to_string()
            }
            "send_prompt" => {
                let worker_id = input["worker_id"].as_str()
                    .ok_or_else(|| ToolError {
                        message: "worker_id is required".to_string(),
                        code: Some("missing_worker_id".to_string()),
                    })?;
                let prompt = input["prompt"].as_str().unwrap_or("");
                let result = self.handle_send_prompt(worker_id, prompt).await?;
                serde_json::json!({
                    "success": true,
                    "result": result
                }).to_string()
            }
            "restart" => {
                let worker_id = input["worker_id"].as_str()
                    .ok_or_else(|| ToolError {
                        message: "worker_id is required".to_string(),
                        code: Some("missing_worker_id".to_string()),
                    })?;
                let worker = self.handle_restart(worker_id).await?;
                serde_json::json!({
                    "success": true,
                    "worker": worker
                }).to_string()
            }
            "terminate" => {
                let worker_id = input["worker_id"].as_str()
                    .ok_or_else(|| ToolError {
                        message: "worker_id is required".to_string(),
                        code: Some("missing_worker_id".to_string()),
                    })?;
                let result = self.handle_terminate(worker_id).await?;
                serde_json::json!({
                    "success": true,
                    "result": result
                }).to_string()
            }
            "observe_completion" => {
                let worker_id = input["worker_id"].as_str()
                    .ok_or_else(|| ToolError {
                        message: "worker_id is required".to_string(),
                        code: Some("missing_worker_id".to_string()),
                    })?;
                let finish_reason = input["finish_reason"].as_str().unwrap_or("finished");
                let tokens_output = input["tokens_output"].as_i64().unwrap_or(0);
                let result = self.handle_observe_completion(worker_id, finish_reason, tokens_output).await?;
                serde_json::json!({
                    "success": true,
                    "result": result
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