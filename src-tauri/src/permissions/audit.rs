use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AuditAction {
    PermissionChecked,
    PermissionGranted,
    PermissionDenied,
    PermissionConfirmed,
    ToolExecuted,
    ToolCancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEntry {
    pub id: String,
    pub timestamp: String,
    pub action: AuditAction,
    pub tool_name: String,
    pub conversation_id: String,
    pub user_id: Option<String>,
    pub result: String,
    pub details: Option<serde_json::Value>,
}

impl AuditEntry {
    pub fn new(action: AuditAction, tool_name: &str, conversation_id: &str) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            timestamp: Utc::now().to_rfc3339(),
            action,
            tool_name: tool_name.to_string(),
            conversation_id: conversation_id.to_string(),
            user_id: None,
            result: String::new(),
            details: None,
        }
    }

    pub fn with_user_id(mut self, user_id: Option<&str>) -> Self {
        self.user_id = user_id.map(|s| s.to_string());
        self
    }

    pub fn with_result(mut self, result: &str) -> Self {
        self.result = result.to_string();
        self
    }

    pub fn with_details(mut self, details: serde_json::Value) -> Self {
        self.details = Some(details);
        self
    }
}

pub struct AuditLogger {
    entries: Mutex<VecDeque<AuditEntry>>,
    max_entries: usize,
}

impl AuditLogger {
    pub fn new(max_entries: usize) -> Self {
        Self {
            entries: Mutex::new(VecDeque::with_capacity(max_entries)),
            max_entries,
        }
    }

    pub fn log(&self, entry: AuditEntry) {
        let mut entries = self.entries.lock().unwrap();
        if entries.len() >= self.max_entries {
            entries.pop_front();
        }
        entries.push_back(entry);
    }

    pub fn get_entries(&self) -> Vec<AuditEntry> {
        let entries = self.entries.lock().unwrap();
        entries.clone().into()
    }

    pub fn get_entries_by_conversation(&self, conversation_id: &str) -> Vec<AuditEntry> {
        let entries = self.entries.lock().unwrap();
        entries
            .iter()
            .filter(|e| e.conversation_id == conversation_id)
            .cloned()
            .collect()
    }

    pub fn clear(&self) {
        let mut entries = self.entries.lock().unwrap();
        entries.clear();
    }
}

pub type AuditLoggerRef = Arc<AuditLogger>;
