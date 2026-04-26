use anyhow::{anyhow, Result};
use futures::StreamExt;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::time::{timeout, Duration};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskRequest {
    pub task_id: String,
    pub prompt: String,
    pub model: Option<String>,
    pub max_tokens: Option<u32>,
    pub context: Option<Vec<serde_json::Value>>,
    pub tools: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskResult {
    pub task_id: String,
    pub success: bool,
    pub output: Option<String>,
    pub error: Option<String>,
    pub duration_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskEvent {
    pub task_id: String,
    pub event_type: String,
    pub data: serde_json::Value,
}

pub struct TaskExecutor {
    client: Client,
    api_key: String,
    base_url: String,
    tasks: Arc<Mutex<HashMap<String, TaskState>>>,
}

#[derive(Debug, Clone)]
struct TaskState {
    task_id: String,
    status: TaskStatus,
    result: Option<TaskResult>,
    started_at: std::time::Instant,
}

#[derive(Debug, Clone, PartialEq)]
pub enum TaskStatus {
    Pending,
    Running,
    Completed,
    Failed,
    Cancelled,
}

impl TaskExecutor {
    pub fn new(api_key: String, base_url: String) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(300))
            .build()
            .unwrap_or_else(|_| Client::new());

        Self {
            client,
            api_key,
            base_url,
            tasks: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub async fn execute_task<F>(
        &self,
        request: TaskRequest,
        event_sender: F,
    ) -> Result<TaskResult>
    where
        F: Fn(TaskEvent) -> Box<dyn Send + FnOnce(TaskEvent)> + Clone + Send + 'static,
    {
        let task_id = request.task_id.clone();
        let start_time = std::time::Instant::now();

        {
            let mut tasks = self.tasks.lock().await;
            tasks.insert(task_id.clone(), TaskState {
                task_id: task_id.clone(),
                status: TaskStatus::Pending,
                result: None,
                started_at: start_time,
            });
        }

        event_sender(TaskEvent {
            task_id: task_id.clone(),
            event_type: "task_started".to_string(),
            data: serde_json::json!({ "prompt": request.prompt }),
        });

        let system_prompt = r#"You are a sub-agent executing a specific task. Your role is to complete the assigned task efficiently and report back the results.

Guidelines:
- Stay focused on the specific task assigned
- Report progress periodically
- Return results in a structured format
- If you encounter errors, report them clearly
"#;

        let mut messages = vec![
            serde_json::json!({
                "role": "user",
                "content": [{
                    "type": "text",
                    "text": system_prompt
                }]
            }),
            serde_json::json!({
                "role": "user",
                "content": [{
                    "type": "text",
                    "text": format!("Task: {}\n\nPlease execute this task and report your results.", request.prompt)
                }]
            })
        ];

        if let Some(context) = request.context {
            messages.insert(1, serde_json::json!({
                "role": "user",
                "content": [{
                    "type": "text",
                    "text": "Context for this task:"
                }]
            }));
            messages.insert(2, serde_json::json!({
                "role": "user",
                "content": context
            }));
        }

        let model = request.model.unwrap_or_else(|| "claude-sonnet-4-6".to_string());
        let max_tokens = request.max_tokens.unwrap_or(4096);

        let api_url = format!("{}/v1/messages", self.base_url.trim_end_matches('/'));

        let body = serde_json::json!({
            "model": model,
            "messages": messages,
            "max_tokens": max_tokens,
            "stream": false,
        });

        let mut attempts = 0;
        let max_attempts = 3;
        let mut last_error = None;

        while attempts < max_attempts {
            attempts += 1;

            let response = self.client
                .post(&api_url)
                .header("Content-Type", "application/json")
                .header("x-api-key", &self.api_key)
                .header("anthropic-version", "2023-06-01")
                .json(&body)
                .send()
                .await;

            match response {
                Ok(resp) => {
                    if resp.status().is_success() {
                        let json: serde_json::Value = resp.json().await?;

                        if let Some(content) = json.get("content").and_then(|c| c.as_array()) {
                            let output = content.iter()
                                .filter_map(|block| {
                                    if block.get("type").and_then(|t| t.as_str()) == Some("text") {
                                        block.get("text").and_then(|t| t.as_str()).map(String::from)
                                    } else {
                                        None
                                    }
                                })
                                .collect::<Vec<_>>()
                                .join("\n\n");

                            let duration_ms = start_time.elapsed().as_millis() as u64;
                            let result = TaskResult {
                                task_id: task_id.clone(),
                                success: true,
                                output: Some(output),
                                error: None,
                                duration_ms,
                            };

                            event_sender(TaskEvent {
                                task_id: task_id.clone(),
                                event_type: "task_completed".to_string(),
                                data: serde_json::to_value(&result)?,
                            });

                            {
                                let mut tasks = self.tasks.lock().await;
                                if let Some(state) = tasks.get_mut(&task_id) {
                                    state.status = TaskStatus::Completed;
                                    state.result = Some(result.clone());
                                }
                            }

                            return Ok(result);
                        }
                    } else {
                        last_error = Some(format!("API error: {}", resp.status()));
                    }
                }
                Err(e) => {
                    last_error = Some(format!("Request error: {}", e));
                }
            }

            if attempts < max_attempts {
                tokio::time::sleep(Duration::from_secs(2 * attempts as u64)).await;
            }
        }

        let duration_ms = start_time.elapsed().as_millis() as u64;
        let error_msg = last_error.unwrap_or_else(|| "Unknown error".to_string());
        let result = TaskResult {
            task_id: task_id.clone(),
            success: false,
            output: None,
            error: Some(error_msg.clone()),
            duration_ms,
        };

        event_sender(TaskEvent {
            task_id: task_id.clone(),
            event_type: "task_failed".to_string(),
            data: serde_json::json!({ "error": error_msg }),
        });

        {
            let mut tasks = self.tasks.lock().await;
            if let Some(state) = tasks.get_mut(&task_id) {
                state.status = TaskStatus::Failed;
                state.result = Some(result.clone());
            }
        }

        Ok(result)
    }

    pub async fn execute_task_streaming<F>(
        &self,
        request: TaskRequest,
        event_sender: F,
    ) -> Result<TaskResult>
    where
        F: Fn(TaskEvent) -> Box<dyn Send + FnOnce(TaskEvent)> + Clone + Send + 'static,
    {
        let task_id = request.task_id.clone();
        let start_time = std::time::Instant::now();

        {
            let mut tasks = self.tasks.lock().await;
            tasks.insert(task_id.clone(), TaskState {
                task_id: task_id.clone(),
                status: TaskStatus::Running,
                result: None,
                started_at: start_time,
            });
        }

        event_sender(TaskEvent {
            task_id: task_id.clone(),
            event_type: "task_started".to_string(),
            data: serde_json::json!({ "prompt": request.prompt }),
        });

        let system_prompt = r#"You are a sub-agent executing a specific task. Your role is to complete the assigned task efficiently and report back the results."#;

        let mut messages = vec![
            serde_json::json!({
                "role": "user",
                "content": [{
                    "type": "text",
                    "text": system_prompt
                }]
            }),
            serde_json::json!({
                "role": "user",
                "content": [{
                    "type": "text",
                    "text": format!("Task: {}", request.prompt)
                }]
            })
        ];

        if let Some(context) = request.context {
            messages.insert(2, serde_json::json!({
                "role": "user",
                "content": [{
                    "type": "text",
                    "text": "Context:"
                }]
            }));
            messages.insert(3, serde_json::json!(context));
        }

        let model = request.model.unwrap_or_else(|| "claude-sonnet-4-6".to_string());
        let max_tokens = request.max_tokens.unwrap_or(4096);

        let api_url = format!("{}/v1/messages", self.base_url.trim_end_matches('/'));

        let body = serde_json::json!({
            "model": model,
            "messages": messages,
            "max_tokens": max_tokens,
            "stream": true,
        });

        let mut full_output = String::new();

        match self.client
            .post(&api_url)
            .header("Content-Type", "application/json")
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .json(&body)
            .send()
            .await
        {
            Ok(response) => {
                if response.status().is_success() {
                    let mut stream = response.bytes_stream();
                    let mut buffer = String::new();

                    while let Some(chunk) = stream.next().await {
                        match chunk {
                            Ok(bytes) => {
                                buffer.push_str(&String::from_utf8_lossy(&bytes).to_string());
                                while let Some(pos) = buffer.find('\n') {
                                    let line = buffer[..pos].to_string();
                                    buffer = buffer[pos + 1..].to_string();

                                    if line.starts_with("data: ") {
                                        let data = &line[6..];
                                        if data != "[DONE]" {
                                            if let Ok(event_data) = serde_json::from_str::<serde_json::Value>(data) {
                                                if let Some(delta) = event_data.get("delta")
                                                    .and_then(|d| d.get("text"))
                                                    .and_then(|t| t.as_str())
                                                {
                                                    full_output.push_str(delta);

                                                    event_sender(TaskEvent {
                                                        task_id: task_id.clone(),
                                                        event_type: "task_delta".to_string(),
                                                        data: serde_json::json!({ "delta": delta }),
                                                    });
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                            Err(e) => {
                                event_sender(TaskEvent {
                                    task_id: task_id.clone(),
                                    event_type: "task_error".to_string(),
                                    data: serde_json::json!({ "error": e.to_string() }),
                                });
                            }
                        }
                    }
                }
            }
            Err(e) => {
                let error_msg = format!("Request error: {}", e);
                event_sender(TaskEvent {
                    task_id: task_id.clone(),
                    event_type: "task_failed".to_string(),
                    data: serde_json::json!({ "error": error_msg }),
                });
            }
        }

        let duration_ms = start_time.elapsed().as_millis() as u64;
        let result = TaskResult {
            task_id: task_id.clone(),
            success: true,
            output: Some(full_output),
            error: None,
            duration_ms,
        };

        event_sender(TaskEvent {
            task_id: task_id.clone(),
            event_type: "task_completed".to_string(),
            data: serde_json::to_value(&result)?,
        });

        {
            let mut tasks = self.tasks.lock().await;
            if let Some(state) = tasks.get_mut(&task_id) {
                state.status = TaskStatus::Completed;
                state.result = Some(result.clone());
            }
        }

        Ok(result)
    }

    pub async fn get_task_status(&self, task_id: &str) -> Option<TaskStatus> {
        let tasks = self.tasks.lock().await;
        tasks.get(task_id).map(|s| s.status.clone())
    }

    pub async fn get_task_result(&self, task_id: &str) -> Option<TaskResult> {
        let tasks = self.tasks.lock().await;
        tasks.get(task_id).and_then(|s| s.result.clone())
    }

    pub async fn cancel_task(&self, task_id: &str) -> bool {
        let mut tasks = self.tasks.lock().await;
        if let Some(state) = tasks.get_mut(task_id) {
            if state.status == TaskStatus::Running || state.status == TaskStatus::Pending {
                state.status = TaskStatus::Cancelled;
                return true;
            }
        }
        false
    }

    pub async fn list_tasks(&self) -> Vec<(String, TaskStatus)> {
        let tasks = self.tasks.lock().await;
        tasks.iter()
            .map(|(id, state)| (id.clone(), state.status.clone()))
            .collect()
    }

    pub async fn cleanup_completed(&self) {
        let mut tasks = self.tasks.lock().await;
        tasks.retain(|_, state| {
            state.status == TaskStatus::Running ||
            state.status == TaskStatus::Pending
        });
    }
}

use bytes::Bytes;
use futures::Stream;
