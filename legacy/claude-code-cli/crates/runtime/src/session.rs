//! Session Management Module
//!
//! Manages session state, history, and lifecycle.

use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub id: String,
    pub created_at: DateTime<Utc>,
    pub last_active: DateTime<Utc>,
    pub workspace_path: Option<String>,
    pub model: Option<String>,
    pub permission_mode: String,
    pub message_count: usize,
    pub tools_used: Vec<String>,
    pub metadata: HashMap<String, serde_json::Value>,
}

impl Session {
    pub fn new(id: String) -> Self {
        let now = Utc::now();
        Self {
            id,
            created_at: now,
            last_active: now,
            workspace_path: None,
            model: None,
            permission_mode: "normal".to_string(),
            message_count: 0,
            tools_used: Vec::new(),
            metadata: HashMap::new(),
        }
    }

    pub fn touch(&mut self) {
        self.last_active = Utc::now();
    }

    pub fn increment_messages(&mut self) {
        self.message_count += 1;
        self.touch();
    }

    pub fn record_tool_use(&mut self, tool_name: &str) {
        if !self.tools_used.contains(&tool_name.to_string()) {
            self.tools_used.push(tool_name.to_string());
        }
        self.touch();
    }
}

pub struct SessionManager {
    sessions: HashMap<String, Session>,
    current_session: Option<String>,
}

impl Default for SessionManager {
    fn default() -> Self {
        Self::new()
    }
}

impl SessionManager {
    pub fn new() -> Self {
        Self {
            sessions: HashMap::new(),
            current_session: None,
        }
    }

    pub fn create_session(&mut self, workspace: Option<String>, model: Option<String>) -> Session {
        let id = format!("session-{}", uuid::Uuid::new_v4());
        let mut session = Session::new(id.clone());
        session.workspace_path = workspace;
        session.model = model;
        self.sessions.insert(id.clone(), session.clone());
        self.current_session = Some(id.clone());
        session
    }

    pub fn get_session(&self, id: &str) -> Option<&Session> {
        self.sessions.get(id)
    }

    pub fn get_session_mut(&mut self, id: &str) -> Option<&mut Session> {
        self.sessions.get_mut(id)
    }

    pub fn current_session(&self) -> Option<&Session> {
        self.current_session.as_ref().and_then(|id| self.sessions.get(id))
    }

    pub fn current_session_mut(&mut self) -> Option<&mut Session> {
        self.current_session.as_ref().and_then(|id| self.sessions.get_mut(id))
    }

    pub fn list_sessions(&self) -> Vec<&Session> {
        self.sessions.values().collect()
    }

    pub fn delete_session(&mut self, id: &str) -> bool {
        if self.current_session.as_ref() == Some(&id.to_string()) {
            self.current_session = None;
        }
        self.sessions.remove(id).is_some()
    }
}