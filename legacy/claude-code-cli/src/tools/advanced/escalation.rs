//! Privilege Escalation System
//!
//! Provides privilege escalation (sudo-like) functionality for elevated operations.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum PrivilegeLevel {
    Standard,
    Elevated,
    Admin,
}

impl PrivilegeLevel {
    pub fn can_escalate_to(&self, target: &PrivilegeLevel) -> bool {
        match (self, target) {
            (PrivilegeLevel::Admin, _) => true,
            (PrivilegeLevel::Elevated, PrivilegeLevel::Standard) => true,
            _ => false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EscalationRequest {
    pub id: String,
    pub reason: String,
    pub requested_level: PrivilegeLevel,
    pub command: Option<String>,
    pub timestamp: u64,
    pub status: EscalationStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EscalationStatus {
    Pending,
    Approved,
    Denied,
    Expired,
}

impl EscalationRequest {
    pub fn new(id: String, reason: String, requested_level: PrivilegeLevel, command: Option<String>) -> Self {
        Self {
            id,
            reason,
            requested_level,
            command,
            timestamp: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            status: EscalationStatus::Pending,
        }
    }

    pub fn is_expired(&self, ttl_seconds: u64) -> bool {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        now.saturating_sub(self.timestamp) > ttl_seconds
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EscalationRecord {
    pub request: EscalationRequest,
    pub approved_by: Option<String>,
    pub executed_at: Option<u64>,
    pub output: Option<String>,
}

pub struct PrivilegeEscalation {
    current_level: PrivilegeLevel,
    history: Vec<EscalationRecord>,
    pending_requests: HashMap<String, EscalationRequest>,
}

impl Default for PrivilegeEscalation {
    fn default() -> Self {
        Self::new()
    }
}

impl PrivilegeEscalation {
    pub fn new() -> Self {
        Self {
            current_level: PrivilegeLevel::Standard,
            history: Vec::new(),
            pending_requests: HashMap::new(),
        }
    }

    pub fn get_current_level(&self) -> PrivilegeLevel {
        self.current_level.clone()
    }

    pub fn request_escalation(
        &mut self,
        reason: String,
        requested_level: PrivilegeLevel,
        command: Option<String>,
    ) -> String {
        let id = format!("escalate_{}_{}", self.history.len(), SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis());

        let request = EscalationRequest::new(
            id.clone(),
            reason,
            requested_level,
            command,
        );

        self.pending_requests.insert(id.clone(), request);
        id
    }

    pub fn approve_request(&mut self, request_id: &str, approved_by: &str) -> Result<PrivilegeLevel, EscalationError> {
        let request = self.pending_requests.remove(request_id)
            .ok_or(EscalationError::RequestNotFound(request_id.to_string()))?;

        if request.is_expired(300) {
            return Err(EscalationError::RequestExpired);
        }

        if !self.current_level.can_escalate_to(&request.requested_level) {
            return Err(EscalationError::InsufficientPrivilege);
        }

        let record = EscalationRecord {
            request: EscalationRequest {
                id: request.id,
                reason: request.reason,
                requested_level: request.requested_level.clone(),
                command: request.command,
                timestamp: request.timestamp,
                status: EscalationStatus::Approved,
            },
            approved_by: Some(approved_by.to_string()),
            executed_at: Some(SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs()),
            output: None,
        };

        self.current_level = request.requested_level;
        self.history.push(record);

        Ok(self.current_level.clone())
    }

    pub fn deny_request(&mut self, request_id: &str, denied_by: &str) -> Result<(), EscalationError> {
        let request = self.pending_requests.remove(request_id)
            .ok_or(EscalationError::RequestNotFound(request_id.to_string()))?;

        let record = EscalationRecord {
            request: EscalationRequest {
                id: request.id,
                reason: request.reason,
                requested_level: request.requested_level,
                command: request.command,
                timestamp: request.timestamp,
                status: EscalationStatus::Denied,
            },
            approved_by: Some(denied_by.to_string()),
            executed_at: None,
            output: None,
        };

        self.history.push(record);
        Ok(())
    }

    pub fn drop_privileges(&mut self) {
        self.current_level = PrivilegeLevel::Standard;
    }

    pub fn get_history(&self) -> &[EscalationRecord] {
        &self.history
    }

    pub fn get_pending_requests(&self) -> Vec<&EscalationRequest> {
        self.pending_requests.values().collect()
    }
}

#[derive(Debug, thiserror::Error)]
pub enum EscalationError {
    #[error("Request not found: {0}")]
    RequestNotFound(String),
    #[error("Request has expired")]
    RequestExpired,
    #[error("Insufficient privilege to perform escalation")]
    InsufficientPrivilege,
    #[error("Operation not permitted")]
    NotPermitted,
}

pub type SharedEscalation = Arc<RwLock<PrivilegeEscalation>>;

pub fn create_shared_escalation() -> SharedEscalation {
    Arc::new(RwLock::new(PrivilegeEscalation::new()))
}