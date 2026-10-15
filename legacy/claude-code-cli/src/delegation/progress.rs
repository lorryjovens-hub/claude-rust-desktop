//! Progress - Progress tracking and event handling for delegation
//!
//! Provides progress callback mechanism for relaying sub-agent events
//! to the parent display.

use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::RwLock;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProgressEvent {
    pub event_type: ProgressEventType,
    pub agent_id: String,
    pub task_index: usize,
    pub tool_name: Option<String>,
    pub preview: Option<String>,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProgressEventType {
    ToolStarted,
    ToolCompleted,
    Thinking,
    SubagentProgress,
    Started,
    Completed,
    Failed,
    Interrupted,
}

impl ProgressEvent {
    pub fn tool_started(agent_id: String, task_index: usize, tool_name: String, preview: Option<String>) -> Self {
        Self {
            event_type: ProgressEventType::ToolStarted,
            agent_id,
            task_index,
            tool_name: Some(tool_name),
            preview,
            timestamp: chrono::Utc::now(),
        }
    }

    pub fn tool_completed(agent_id: String, task_index: usize, tool_name: String) -> Self {
        Self {
            event_type: ProgressEventType::ToolCompleted,
            agent_id,
            task_index,
            tool_name: Some(tool_name),
            preview: None,
            timestamp: chrono::Utc::now(),
        }
    }

    pub fn thinking(agent_id: String, task_index: usize, text: String) -> Self {
        Self {
            event_type: ProgressEventType::Thinking,
            agent_id,
            task_index,
            tool_name: None,
            preview: Some(text),
            timestamp: chrono::Utc::now(),
        }
    }

    pub fn subagent_progress(agent_id: String, task_index: usize, summary: String) -> Self {
        Self {
            event_type: ProgressEventType::SubagentProgress,
            agent_id,
            task_index,
            tool_name: None,
            preview: Some(summary),
            timestamp: chrono::Utc::now(),
        }
    }
}

pub type ProgressCallback = Arc<dyn Fn(ProgressEvent) + Send + Sync>;

pub struct ProgressTracker {
    callback: Option<ProgressCallback>,
    events: Arc<RwLock<Vec<ProgressEvent>>>,
}

impl ProgressTracker {
    pub fn new(callback: Option<ProgressCallback>) -> Self {
        Self {
            callback,
            events: Arc::new(RwLock::new(Vec::new())),
        }
    }

    pub async fn emit(&self, event: ProgressEvent) {
        let mut events = self.events.write().await;
        events.push(event.clone());

        if let Some(ref callback) = self.callback {
            callback(event);
        }
    }

    pub async fn emit_tool_started(&self, agent_id: String, task_index: usize, tool_name: String, preview: Option<String>) {
        self.emit(ProgressEvent::tool_started(agent_id, task_index, tool_name, preview)).await;
    }

    pub async fn emit_tool_completed(&self, agent_id: String, task_index: usize, tool_name: String) {
        self.emit(ProgressEvent::tool_completed(agent_id, task_index, tool_name)).await;
    }

    pub async fn emit_thinking(&self, agent_id: String, task_index: usize, text: String) {
        self.emit(ProgressEvent::thinking(agent_id, task_index, text)).await;
    }

    pub async fn emit_subagent_progress(&self, agent_id: String, task_index: usize, summary: String) {
        self.emit(ProgressEvent::subagent_progress(agent_id, task_index, summary)).await;
    }

    pub async fn get_events(&self) -> Vec<ProgressEvent> {
        let events = self.events.read().await;
        events.clone()
    }

    pub async fn clear(&self) {
        let mut events = self.events.write().await;
        events.clear();
    }
}

impl Clone for ProgressTracker {
    fn clone(&self) -> Self {
        Self {
            callback: self.callback.clone(),
            events: self.events.clone(),
        }
    }
}

pub struct BatchedProgress {
    tracker: ProgressTracker,
    batch: Arc<RwLock<Vec<String>>>,
    batch_size: usize,
}

impl BatchedProgress {
    pub fn new(tracker: ProgressTracker, batch_size: usize) -> Self {
        Self {
            tracker,
            batch: Arc::new(RwLock::new(Vec::new())),
            batch_size,
        }
    }

    pub async fn add_tool(&self, agent_id: String, task_index: usize, tool_name: String) {
        let mut batch = self.batch.write().await;
        batch.push(tool_name.clone());

        if batch.len() >= self.batch_size {
            let tools: Vec<String> = batch.drain(..).collect();
            let summary = tools.join(", ");
            self.tracker.emit_subagent_progress(agent_id, task_index, summary).await;
        }
    }

    pub async fn flush(&self, agent_id: String, task_index: usize) {
        let mut batch = self.batch.write().await;
        if !batch.is_empty() {
            let tools: Vec<String> = batch.drain(..).collect();
            let summary = tools.join(", ");
            self.tracker.emit_subagent_progress(agent_id, task_index, summary).await;
        }
    }
}

impl Clone for BatchedProgress {
    fn clone(&self) -> Self {
        Self {
            tracker: self.tracker.clone(),
            batch: self.batch.clone(),
            batch_size: self.batch_size,
        }
    }
}
