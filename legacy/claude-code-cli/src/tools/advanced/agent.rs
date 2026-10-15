//! Agent Tool - Core Agent system with fork, resume, and run capabilities
//!
//! Provides comprehensive agent lifecycle management including:
//! - forkSubagent: Create a forked sub-agent for parallel execution
//! - resumeAgent: Resume a paused or stopped agent
//! - runAgent: Run an agent with a specific task
//! - planAgent: Enter planning mode for complex tasks
//! - exploreAgent: Explore code or functionality
//! - verificationAgent: Verify results or assumptions

use crate::tools::{Tool, ToolOutput, ToolError};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use chrono::{DateTime, Utc};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Agent {
    pub agent_id: String,
    pub name: String,
    pub parent_id: Option<String>,
    pub status: AgentStatus,
    pub mode: AgentMode,
    pub cwd: String,
    pub model: Option<String>,
    pub prompt: Option<String>,
    pub created_at: DateTime<Utc>,
    pub last_active: DateTime<Utc>,
    pub metadata: HashMap<String, serde_json::Value>,
    pub history: Vec<AgentHistoryEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum AgentStatus {
    Created,
    Initializing,
    Running,
    Paused,
    WaitingForTrust,
    TrustResolved,
    Ready,
    Completed,
    Failed,
    Terminated,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum AgentMode {
    Expert,
    Explorer,
    Planner,
    Verifier,
    Auto,
}

impl std::str::FromStr for AgentMode {
    type Err = ();
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "expert" => Ok(AgentMode::Expert),
            "explorer" => Ok(AgentMode::Explorer),
            "planner" => Ok(AgentMode::Planner),
            "verifier" => Ok(AgentMode::Verifier),
            "auto" => Ok(AgentMode::Auto),
            _ => Err(()),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentHistoryEntry {
    pub timestamp: DateTime<Utc>,
    pub event: String,
    pub details: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentSession {
    pub session_id: String,
    pub agent_id: String,
    pub status: SessionStatus,
    pub messages: Vec<SessionMessage>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum SessionStatus {
    Active,
    Paused,
    Completed,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionMessage {
    pub role: String,
    pub content: String,
    pub timestamp: DateTime<Utc>,
}

pub struct AgentTool {
    agents: Arc<RwLock<HashMap<String, Agent>>>,
    sessions: Arc<RwLock<HashMap<String, AgentSession>>>,
}

impl Default for AgentTool {
    fn default() -> Self {
        Self::new()
    }
}

impl AgentTool {
    pub fn new() -> Self {
        Self {
            agents: Arc::new(RwLock::new(HashMap::new())),
            sessions: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    fn generate_id(prefix: &str) -> String {
        format!("{}-{}", prefix, uuid::Uuid::new_v4())
    }

    fn add_history(agent: &mut Agent, event: &str, details: Option<String>) {
        agent.history.push(AgentHistoryEntry {
            timestamp: Utc::now(),
            event: event.to_string(),
            details,
        });
    }

    async fn handle_create(&self, input: &serde_json::Value) -> Result<Agent, ToolError> {
        let name = input["name"].as_str()
            .ok_or_else(|| ToolError {
                message: "name is required".to_string(),
                code: Some("missing_name".to_string()),
            })?
            .to_string();

        let mode_str = input["mode"].as_str().unwrap_or("expert");
        let mode: AgentMode = mode_str.parse()
            .map_err(|_| ToolError {
                message: "Invalid mode. Must be: expert, explorer, planner, verifier, auto".to_string(),
                code: Some("invalid_mode".to_string()),
            })?;

        let cwd = input["cwd"].as_str().unwrap_or(".").to_string();
        let model = input["model"].as_str().map(String::from);
        let parent_id = input["parent_id"].as_str().map(String::from);

        let agent_id = Self::generate_id("agent");

        let agent = Agent {
            agent_id: agent_id.clone(),
            name,
            parent_id,
            status: AgentStatus::Created,
            mode,
            cwd,
            model,
            prompt: None,
            created_at: Utc::now(),
            last_active: Utc::now(),
            metadata: HashMap::new(),
            history: Vec::new(),
        };

        let mut agents = self.agents.write().await;
        agents.insert(agent_id.clone(), agent.clone());

        Ok(agent)
    }

    async fn handle_fork(&self, input: &serde_json::Value) -> Result<Agent, ToolError> {
        let parent_id = input["parent_id"].as_str()
            .ok_or_else(|| ToolError {
                message: "parent_id is required".to_string(),
                code: Some("missing_parent_id".to_string()),
            })?;

        let agents = self.agents.read().await;
        let parent = agents.get(parent_id)
            .ok_or_else(|| ToolError {
                message: format!("Parent agent not found: {}", parent_id),
                code: Some("agent_not_found".to_string()),
            })?;

        let parent_name = parent.name.clone();
        let parent_mode = parent.mode.clone();
        let parent_cwd = parent.cwd.clone();
        let parent_model = parent.model.clone();
        drop(agents);

        let mut fork_input = input.clone();
        fork_input["name"] = serde_json::json!(format!("{}-fork", parent_name));
        fork_input["parent_id"] = serde_json::json!(parent_id);
        fork_input["mode"] = serde_json::json!(format!("{:?}", parent_mode));
        fork_input["cwd"] = serde_json::json!(parent_cwd);
        if let Some(ref model) = parent_model {
            fork_input["model"] = serde_json::json!(model);
        }

        let mut agent = self.handle_create(&fork_input).await?;

        let mut agents = self.agents.write().await;
        if let Some(parent_agent) = agents.get_mut(parent_id) {
            Self::add_history(parent_agent, "forked", Some(agent.agent_id.clone()));
        }

        Self::add_history(&mut agent, "forked_from", Some(parent_id.to_string()));

        if let Some(parent_agent) = agents.get(parent_id) {
            agent.prompt = parent_agent.prompt.clone();
            agent.metadata = parent_agent.metadata.clone();
        }

        Ok(agent)
    }

    async fn handle_run(&self, input: &serde_json::Value) -> Result<SessionStatus, ToolError> {
        let agent_id = input["agent_id"].as_str()
            .ok_or_else(|| ToolError {
                message: "agent_id is required".to_string(),
                code: Some("missing_agent_id".to_string()),
            })?;

        let prompt = input["prompt"].as_str()
            .ok_or_else(|| ToolError {
                message: "prompt is required".to_string(),
                code: Some("missing_prompt".to_string()),
            })?;

        let mut agents = self.agents.write().await;
        let agent = agents.get_mut(agent_id)
            .ok_or_else(|| ToolError {
                message: format!("Agent not found: {}", agent_id),
                code: Some("agent_not_found".to_string()),
            })?;

        agent.status = AgentStatus::Running;
        agent.prompt = Some(prompt.to_string());
        agent.last_active = Utc::now();
        Self::add_history(agent, "run_started", None);

        let session_id = Self::generate_id("session");
        let session = AgentSession {
            session_id: session_id.clone(),
            agent_id: agent_id.to_string(),
            status: SessionStatus::Active,
            messages: vec![SessionMessage {
                role: "user".to_string(),
                content: prompt.to_string(),
                timestamp: Utc::now(),
            }],
            created_at: Utc::now(),
        };

        drop(agents);

        let mut sessions = self.sessions.write().await;
        sessions.insert(session_id, session);

        Ok(SessionStatus::Active)
    }

    async fn handle_resume(&self, agent_id: &str) -> Result<SessionStatus, ToolError> {
        let mut agents = self.agents.write().await;
        let agent = agents.get_mut(agent_id)
            .ok_or_else(|| ToolError {
                message: format!("Agent not found: {}", agent_id),
                code: Some("agent_not_found".to_string()),
            })?;

        match agent.status {
            AgentStatus::Paused | AgentStatus::WaitingForTrust | AgentStatus::Ready => {
                agent.status = AgentStatus::Running;
                agent.last_active = Utc::now();
                Self::add_history(agent, "resumed", None);
                Ok(SessionStatus::Active)
            }
            _ => Err(ToolError {
                message: format!("Agent cannot be resumed from status: {:?}", agent.status),
                code: Some("invalid_status".to_string()),
            }),
        }
    }

    async fn handle_pause(&self, agent_id: &str) -> Result<AgentStatus, ToolError> {
        let mut agents = self.agents.write().await;
        let agent = agents.get_mut(agent_id)
            .ok_or_else(|| ToolError {
                message: format!("Agent not found: {}", agent_id),
                code: Some("agent_not_found".to_string()),
            })?;

        agent.status = AgentStatus::Paused;
        agent.last_active = Utc::now();
        Self::add_history(agent, "paused", None);

        Ok(AgentStatus::Paused)
    }

    async fn handle_get(&self, agent_id: &str) -> Result<Agent, ToolError> {
        let agents = self.agents.read().await;
        agents.get(agent_id)
            .cloned()
            .ok_or_else(|| ToolError {
                message: format!("Agent not found: {}", agent_id),
                code: Some("agent_not_found".to_string()),
            })
    }

    async fn handle_list(&self) -> Result<Vec<Agent>, ToolError> {
        let agents = self.agents.read().await;
        let list: Vec<Agent> = agents.values().cloned().collect();
        Ok(list)
    }

    async fn handle_terminate(&self, agent_id: &str) -> Result<String, ToolError> {
        let mut agents = self.agents.write().await;
        let agent = agents.get_mut(agent_id)
            .ok_or_else(|| ToolError {
                message: format!("Agent not found: {}", agent_id),
                code: Some("agent_not_found".to_string()),
            })?;

        agent.status = AgentStatus::Terminated;
        Self::add_history(agent, "terminated", None);

        Ok(format!("Agent {} terminated", agent_id))
    }

    async fn handle_set_mode(&self, agent_id: &str, mode: &str) -> Result<Agent, ToolError> {
        let parsed_mode: AgentMode = mode.parse()
            .map_err(|_| ToolError {
                message: "Invalid mode".to_string(),
                code: Some("invalid_mode".to_string()),
            })?;

        let mut agents = self.agents.write().await;
        let agent = agents.get_mut(agent_id)
            .ok_or_else(|| ToolError {
                message: format!("Agent not found: {}", agent_id),
                code: Some("agent_not_found".to_string()),
            })?;

        Self::add_history(agent, "mode_changed", Some(mode.to_string()));
        agent.mode = parsed_mode;
        Ok(agent.clone())
    }

    async fn handle_plan(&self, agent_id: &str, task: &str) -> Result<String, ToolError> {
        let mut agents = self.agents.write().await;
        let agent = agents.get_mut(agent_id)
            .ok_or_else(|| ToolError {
                message: format!("Agent not found: {}", agent_id),
                code: Some("agent_not_found".to_string()),
            })?;

        agent.mode = AgentMode::Planner;
        agent.status = AgentStatus::Running;
        Self::add_history(agent, "planning_started", Some(task.to_string()));

        Ok(format!("Agent {} entered planning mode for task: {}", agent_id, task))
    }

    async fn handle_explore(&self, agent_id: &str, target: &str) -> Result<String, ToolError> {
        let mut agents = self.agents.write().await;
        let agent = agents.get_mut(agent_id)
            .ok_or_else(|| ToolError {
                message: format!("Agent not found: {}", agent_id),
                code: Some("agent_not_found".to_string()),
            })?;

        agent.mode = AgentMode::Explorer;
        Self::add_history(agent, "exploration_started", Some(target.to_string()));

        Ok(format!("Agent {} exploring: {}", agent_id, target))
    }

    async fn handle_verify(&self, agent_id: &str, hypothesis: &str) -> Result<String, ToolError> {
        let mut agents = self.agents.write().await;
        let agent = agents.get_mut(agent_id)
            .ok_or_else(|| ToolError {
                message: format!("Agent not found: {}", agent_id),
                code: Some("agent_not_found".to_string()),
            })?;

        agent.mode = AgentMode::Verifier;
        Self::add_history(agent, "verification_started", Some(hypothesis.to_string()));

        Ok(format!("Agent {} verifying hypothesis: {}", agent_id, hypothesis))
    }
}

#[async_trait]
impl Tool for AgentTool {
    fn name(&self) -> &str {
        "agent"
    }

    fn description(&self) -> &str {
        "Manage agent lifecycle with fork, resume, run, plan, explore, and verify operations"
    }

    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "operation": {
                    "type": "string",
                    "enum": ["create", "fork", "run", "resume", "pause", "get", "list", "terminate", "set_mode", "plan", "explore", "verify"]
                },
                "agent_id": { "type": "string", "description": "Agent ID" },
                "parent_id": { "type": "string", "description": "Parent agent ID for forking" },
                "name": { "type": "string", "description": "Agent name" },
                "mode": {
                    "type": "string",
                    "enum": ["expert", "explorer", "planner", "verifier", "auto"],
                    "description": "Agent mode"
                },
                "cwd": { "type": "string", "description": "Working directory" },
                "model": { "type": "string", "description": "Model to use" },
                "prompt": { "type": "string", "description": "Prompt for run operation" },
                "task": { "type": "string", "description": "Task for planning mode" },
                "target": { "type": "string", "description": "Target for exploration" },
                "hypothesis": { "type": "string", "description": "Hypothesis to verify" }
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
                let agent = self.handle_create(&input).await?;
                serde_json::json!({
                    "success": true,
                    "agent_id": agent.agent_id,
                    "status": format!("{:?}", agent.status)
                }).to_string()
            }
            "fork" => {
                let agent = self.handle_fork(&input).await?;
                serde_json::json!({
                    "success": true,
                    "agent_id": agent.agent_id,
                    "parent_id": agent.parent_id,
                    "forked_from": agent.parent_id
                }).to_string()
            }
            "run" => {
                let status = self.handle_run(&input).await?;
                serde_json::json!({
                    "success": true,
                    "status": format!("{:?}", status)
                }).to_string()
            }
            "resume" => {
                let agent_id = input["agent_id"].as_str()
                    .ok_or_else(|| ToolError {
                        message: "agent_id is required".to_string(),
                        code: Some("missing_agent_id".to_string()),
                    })?;
                let status = self.handle_resume(agent_id).await?;
                serde_json::json!({
                    "success": true,
                    "status": format!("{:?}", status)
                }).to_string()
            }
            "pause" => {
                let agent_id = input["agent_id"].as_str()
                    .ok_or_else(|| ToolError {
                        message: "agent_id is required".to_string(),
                        code: Some("missing_agent_id".to_string()),
                    })?;
                let status = self.handle_pause(agent_id).await?;
                serde_json::json!({
                    "success": true,
                    "status": format!("{:?}", status)
                }).to_string()
            }
            "get" => {
                let agent_id = input["agent_id"].as_str()
                    .ok_or_else(|| ToolError {
                        message: "agent_id is required".to_string(),
                        code: Some("missing_agent_id".to_string()),
                    })?;
                let agent = self.handle_get(agent_id).await?;
                serde_json::json!({
                    "success": true,
                    "agent": agent
                }).to_string()
            }
            "list" => {
                let agents = self.handle_list().await?;
                serde_json::json!({
                    "success": true,
                    "agents": agents,
                    "count": agents.len()
                }).to_string()
            }
            "terminate" => {
                let agent_id = input["agent_id"].as_str()
                    .ok_or_else(|| ToolError {
                        message: "agent_id is required".to_string(),
                        code: Some("missing_agent_id".to_string()),
                    })?;
                let result = self.handle_terminate(agent_id).await?;
                serde_json::json!({
                    "success": true,
                    "result": result
                }).to_string()
            }
            "set_mode" => {
                let agent_id = input["agent_id"].as_str()
                    .ok_or_else(|| ToolError {
                        message: "agent_id is required".to_string(),
                        code: Some("missing_agent_id".to_string()),
                    })?;
                let mode = input["mode"].as_str()
                    .ok_or_else(|| ToolError {
                        message: "mode is required".to_string(),
                        code: Some("missing_mode".to_string()),
                    })?;
                let agent = self.handle_set_mode(agent_id, mode).await?;
                serde_json::json!({
                    "success": true,
                    "agent": agent
                }).to_string()
            }
            "plan" => {
                let agent_id = input["agent_id"].as_str()
                    .ok_or_else(|| ToolError {
                        message: "agent_id is required".to_string(),
                        code: Some("missing_agent_id".to_string()),
                    })?;
                let task = input["task"].as_str().unwrap_or("");
                let result = self.handle_plan(agent_id, task).await?;
                serde_json::json!({
                    "success": true,
                    "result": result
                }).to_string()
            }
            "explore" => {
                let agent_id = input["agent_id"].as_str()
                    .ok_or_else(|| ToolError {
                        message: "agent_id is required".to_string(),
                        code: Some("missing_agent_id".to_string()),
                    })?;
                let target = input["target"].as_str().unwrap_or("");
                let result = self.handle_explore(agent_id, target).await?;
                serde_json::json!({
                    "success": true,
                    "result": result
                }).to_string()
            }
            "verify" => {
                let agent_id = input["agent_id"].as_str()
                    .ok_or_else(|| ToolError {
                        message: "agent_id is required".to_string(),
                        code: Some("missing_agent_id".to_string()),
                    })?;
                let hypothesis = input["hypothesis"].as_str().unwrap_or("");
                let result = self.handle_verify(agent_id, hypothesis).await?;
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