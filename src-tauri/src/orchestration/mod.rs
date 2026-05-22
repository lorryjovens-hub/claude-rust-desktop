use crate::native_engine::anthropic_client::{AnthropicClient, AnthropicContent, AnthropicMessage};
use crate::native_engine::openai_client::{OpenAIClient, OpenAIContent, OpenAIMessage};
use crate::native_engine::provider_manager::{ApiFormat, ResolvedProvider};
use anyhow::{anyhow, Result};
use futures::StreamExt;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet, BinaryHeap};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{broadcast, Mutex, Semaphore};

pub mod config;
pub mod openspace;
pub mod superpowers;
pub mod gstack;
pub mod test_scenario;

pub use config::*;
pub use openspace::*;
pub use superpowers::*;
pub use gstack::*;
pub use test_scenario::*;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum WorkflowPhase {
    Planning,
    Design,
    Implementation,
    Review,
    Deployment,
    Completed,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowTask {
    pub task_id: String,
    pub name: String,
    pub description: String,
    pub agent_role: AgentRole,
    pub dependencies: Vec<String>,
    pub status: TaskStatus,
    pub output: Option<serde_json::Value>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum TaskStatus {
    Pending,
    InProgress,
    Completed,
    Failed,
    Skipped,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum AgentRole {
    ProductManager,
    Architect,
    Developer,
    Reviewer,
    DevOps,
    Analyst,
    Designer,
    Custom(String),
}

impl AgentRole {
    pub fn to_string(&self) -> String {
        match self {
            AgentRole::ProductManager => "Product Manager".to_string(),
            AgentRole::Architect => "Solution Architect".to_string(),
            AgentRole::Developer => "Software Developer".to_string(),
            AgentRole::Reviewer => "Code Reviewer".to_string(),
            AgentRole::DevOps => "DevOps Engineer".to_string(),
            AgentRole::Analyst => "Data Analyst".to_string(),
            AgentRole::Designer => "UX Designer".to_string(),
            AgentRole::Custom(s) => s.clone(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowEvent {
    pub event_type: String,
    pub task_id: Option<String>,
    pub message: String,
    pub data: Option<serde_json::Value>,
    pub timestamp: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Ord, PartialOrd)]
pub enum TaskPriority {
    Critical = 5,
    High = 4,
    Medium = 3,
    Low = 2,
    Background = 1,
}

impl TaskPriority {
    pub fn from_phase(phase: &str) -> Self {
        match phase.to_lowercase().as_str() {
            "planning" => TaskPriority::High,
            "design" => TaskPriority::High,
            "implementation" => TaskPriority::Medium,
            "review" => TaskPriority::Critical,
            "deployment" => TaskPriority::Critical,
            _ => TaskPriority::Medium,
        }
    }

    pub fn from_role(role: &AgentRole) -> Self {
        match role {
            AgentRole::ProductManager => TaskPriority::High,
            AgentRole::Architect => TaskPriority::High,
            AgentRole::Developer => TaskPriority::Medium,
            AgentRole::Reviewer => TaskPriority::Critical,
            AgentRole::DevOps => TaskPriority::Critical,
            AgentRole::Analyst => TaskPriority::Medium,
            AgentRole::Designer => TaskPriority::High,
            AgentRole::Custom(_) => TaskPriority::Medium,
        }
    }

    pub fn from_i32(value: i32) -> Option<Self> {
        match value {
            5 => Some(TaskPriority::Critical),
            4 => Some(TaskPriority::High),
            3 => Some(TaskPriority::Medium),
            2 => Some(TaskPriority::Low),
            1 => Some(TaskPriority::Background),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Eq)]
pub struct PriorityTask {
    pub priority: TaskPriority,
    pub deadline: Option<Instant>,
    pub task: TaskDefinition,
    pub position: usize,
}

impl PartialEq for PriorityTask {
    fn eq(&self, other: &Self) -> bool {
        self.priority == other.priority && self.task.id == other.task.id
    }
}

impl Ord for PriorityTask {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        let priority_cmp = self.priority.cmp(&other.priority);
        if priority_cmp != std::cmp::Ordering::Equal {
            return priority_cmp;
        }
        
        match (&self.deadline, &other.deadline) {
            (Some(d1), Some(d2)) => d1.cmp(d2),
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (None, None) => self.position.cmp(&other.position),
        }
    }
}

impl PartialOrd for PriorityTask {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

#[derive(Debug, Clone)]
pub struct OrchestratorConfig {
    pub max_concurrent_agents: usize,
    pub default_model: String,
    pub timeout_ms: u64,
    pub enable_priority_scheduling: bool,
    pub priority_adjust_interval_ms: u64,
    pub aging_factor: f64,
}

impl Default for OrchestratorConfig {
    fn default() -> Self {
        Self {
            max_concurrent_agents: 8,
            default_model: "claude-sonnet-4-6".to_string(),
            timeout_ms: 300_000,
            enable_priority_scheduling: true,
            priority_adjust_interval_ms: 10_000,
            aging_factor: 0.1,
        }
    }
}

pub struct MultiAgentOrchestrator {
    config: OrchestratorConfig,
    event_tx: broadcast::Sender<WorkflowEvent>,
    http_client: reqwest::Client,
    openspace: OpenSpaceManager,
    superpowers: SuperpowersEngine,
    gstack: GStackManager,
    task_priorities: Arc<Mutex<HashMap<String, TaskPriority>>>,
    task_start_times: Arc<Mutex<HashMap<String, Instant>>>,
    pending_tasks: Arc<Mutex<BinaryHeap<PriorityTask>>>,
    running_tasks: Arc<Mutex<HashSet<String>>>,
    completed_tasks: Arc<Mutex<HashSet<String>>>,
}

impl MultiAgentOrchestrator {
    pub fn new(config: OrchestratorConfig, data_dir: &std::path::Path) -> Self {
        let event_tx = broadcast::channel(100).0;
        eprintln!("[Orchestrator] Initializing MultiAgentOrchestrator with config: max_concurrent={}, priority_scheduling={}, aging_factor={}", 
            config.max_concurrent_agents, config.enable_priority_scheduling, config.aging_factor);
        
        Self {
            config,
            event_tx,
            http_client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(300))
                .build()
                .expect("Failed to create HTTP client"),
            openspace: OpenSpaceManager::new(data_dir),
            superpowers: SuperpowersEngine::new(),
            gstack: GStackManager::new(data_dir),
            task_priorities: Arc::new(Mutex::new(HashMap::new())),
            task_start_times: Arc::new(Mutex::new(HashMap::new())),
            pending_tasks: Arc::new(Mutex::new(BinaryHeap::new())),
            running_tasks: Arc::new(Mutex::new(HashSet::new())),
            completed_tasks: Arc::new(Mutex::new(HashSet::new())),
        }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<WorkflowEvent> {
        self.event_tx.subscribe()
    }

    fn emit_event(&self, event_type: &str, task_id: Option<String>, message: &str, data: Option<serde_json::Value>) {
        let _ = self.event_tx.send(WorkflowEvent {
            event_type: event_type.to_string(),
            task_id,
            message: message.to_string(),
            data,
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis() as u64,
        });
    }

    fn log(&self, level: &str, task_id: Option<&str>, message: &str) {
        let timestamp = chrono::Local::now().format("%Y-%m-%d %H:%M:%S%.3f");
        match task_id {
            Some(id) => eprintln!("[{}] [{}] [Task: {}] {}", timestamp, level, id, message),
            None => eprintln!("[{}] [{}] {}", timestamp, level, message),
        }
    }

    fn log_trace(&self, task_id: Option<&str>, message: &str) {
        self.log("TRACE", task_id, message);
    }

    fn log_debug(&self, task_id: Option<&str>, message: &str) {
        self.log("DEBUG", task_id, message);
    }

    fn log_info(&self, task_id: Option<&str>, message: &str) {
        self.log("INFO", task_id, message);
    }

    fn log_warn(&self, task_id: Option<&str>, message: &str) {
        self.log("WARN", task_id, message);
    }

    fn log_error(&self, task_id: Option<&str>, message: &str) {
        self.log("ERROR", task_id, message);
    }

    pub async fn execute_workflow(
        &self,
        goal: &str,
        provider: &ResolvedProvider,
    ) -> Result<serde_json::Value> {
        let start_time = Instant::now();
        
        self.log_info(None, &format!("=== WORKFLOW STARTING ==="));
        self.log_info(None, &format!("Goal: {}", goal));
        self.log_info(None, &format!("Provider: {:?}", provider.provider.name));
        self.log_info(None, &format!("Max concurrent agents: {}", self.config.max_concurrent_agents));
        self.log_info(None, &format!("Priority scheduling: {}", self.config.enable_priority_scheduling));
        
        self.emit_event("workflow_start", None, "Starting multi-agent workflow", None);

        self.log_info(None, "Step 1/4: Analyzing requirements with OpenSpace...");
        let requirements = self.openspace.analyze_requirements(goal, provider).await?;
        self.log_info(None, &format!("Requirements analysis completed: {} requirements, {} user stories, {} risks", 
            requirements.requirements.len(), requirements.user_stories.len(), requirements.risks.len()));
        self.emit_event("requirements_completed", None, "Requirements analysis completed", Some(serde_json::to_value(&requirements).unwrap()));

        self.log_info(None, "Step 2/4: Generating work plan with GStack...");
        let plan = self.gstack.generate_workplan(&requirements, provider).await?;
        self.log_info(None, &format!("Work plan generated: {} tasks across {} phases", 
            plan.tasks.len(), plan.phases.len()));
        for (i, phase) in plan.phases.iter().enumerate() {
            let phase_tasks = plan.tasks.iter().filter(|t| t.phase == *phase).count();
            self.log_debug(None, &format!("  Phase {}: {} ({} tasks)", i + 1, phase, phase_tasks));
        }
        self.emit_event("plan_generated", None, "Work plan generated", Some(serde_json::to_value(&plan).unwrap()));

        self.log_info(None, "Step 3/4: Validating plan with Superpowers...");
        let validated_plan = self.superpowers.validate_plan(&plan, provider).await?;
        if !validated_plan.valid {
            self.log_error(None, &format!("Plan validation failed: {}", validated_plan.message));
            return Err(anyhow!("Plan validation failed: {}", validated_plan.message));
        }
        self.log_info(None, &format!("Plan validated successfully: {} issues (all non-blocking)", validated_plan.issues.len()));
        self.emit_event("plan_validated", None, "Plan validated successfully", None);

        self.log_info(None, "Step 4/4: Executing work plan...");
        let execution_result = self.execute_workplan(&plan, provider).await?;

        let duration_ms = start_time.elapsed().as_millis() as u64;
        self.log_info(None, &format!("=== WORKFLOW COMPLETED ==="));
        self.log_info(None, &format!("Total duration: {}ms", duration_ms));
        
        let completed_count = execution_result.get("completed_tasks").and_then(|v| v.as_u64()).unwrap_or(0);
        let total_count = execution_result.get("total_tasks").and_then(|v| v.as_u64()).unwrap_or(0);
        self.log_info(None, &format!("Tasks: {}/{} completed", completed_count, total_count));
        
        self.emit_event("workflow_completed", None, &format!("Workflow completed in {}ms", duration_ms), Some(execution_result.clone()));

        Ok(execution_result)
    }

    async fn execute_workplan(
        &self,
        plan: &WorkPlan,
        provider: &ResolvedProvider,
    ) -> Result<serde_json::Value> {
        let semaphore = Arc::new(Semaphore::new(self.config.max_concurrent_agents));
        let mut task_results: HashMap<String, serde_json::Value> = HashMap::new();
        
        let completed_tasks = self.completed_tasks.clone();
        let running_tasks = self.running_tasks.clone();
        let pending_tasks = self.pending_tasks.clone();
        let task_priorities = self.task_priorities.clone();
        let task_start_times = self.task_start_times.clone();

        completed_tasks.lock().await.clear();
        running_tasks.lock().await.clear();
        pending_tasks.lock().await.clear();

        self.log_info(None, &format!("Initializing task queue with {} tasks", plan.tasks.len()));

        for (idx, task) in plan.tasks.iter().enumerate() {
            let priority = if self.config.enable_priority_scheduling {
                let phase_priority = TaskPriority::from_phase(&task.phase);
                let role_priority = TaskPriority::from_role(&task.agent_role);
                std::cmp::max(phase_priority, role_priority)
            } else {
                TaskPriority::Medium
            };
            
            let deadline = Some(Instant::now() + Duration::from_secs((task.estimated_duration_minutes * 60) as u64));
            
            let priority_clone = priority.clone();
            pending_tasks.lock().await.push(PriorityTask {
                priority: priority_clone.clone(),
                deadline,
                task: task.clone(),
                position: idx,
            });
            
            task_priorities.lock().await.insert(task.id.clone(), priority_clone.clone());
            
            self.log_debug(None, &format!("Task {} added to queue - Priority: {:?}, Phase: {}, Agent: {}", 
                task.id, priority_clone, task.phase, task.agent_role.to_string()));
        }

        let mut priority_adjust_task = None;
        if self.config.enable_priority_scheduling {
            let task_priorities_clone = task_priorities.clone();
            let pending_tasks_clone = pending_tasks.clone();
            let task_start_times_clone_for_priority = task_start_times.clone();
            let config = self.config.clone();
            
            priority_adjust_task = Some(tokio::spawn(async move {
                loop {
                    tokio::time::sleep(Duration::from_millis(config.priority_adjust_interval_ms)).await;
                    
                    let mut priorities = task_priorities_clone.lock().await;
                    let mut pending = pending_tasks_clone.lock().await;
                    
                    if pending.is_empty() {
                        continue;
                    }
                    
                    let mut adjusted_tasks = Vec::new();
                    let task_count = pending.len();
                    
                    for _ in 0..task_count {
                        if let Some(mut pt) = pending.pop() {
                            let task_age_seconds = Instant::now().duration_since(
                                task_start_times_clone_for_priority.lock().await
                                    .get(&pt.task.id)
                                    .copied()
                                    .unwrap_or_else(Instant::now)
                            ).as_secs() as f64;
                            
                            let age_minutes = task_age_seconds / 60.0;
                            let age_bonus = (age_minutes * config.aging_factor).min(2.0) as i32;
                            
                            if age_bonus > 0 {
                                let current_priority_value = pt.priority.clone() as i32;
                                let new_priority_value = std::cmp::min(current_priority_value + age_bonus, TaskPriority::Critical as i32);
                                
                                if let Some(new_priority) = TaskPriority::from_i32(new_priority_value) {
                                    if new_priority != pt.priority {
                                        eprintln!("[Priority] Task {} aged {} mins, priority increased from {:?} to {:?}", 
                                            pt.task.id, age_minutes, pt.priority, new_priority);
                                        pt.priority = new_priority.clone();
                                        priorities.insert(pt.task.id.clone(), new_priority);
                                    }
                                }
                            }
                            adjusted_tasks.push(pt);
                        }
                    }
                    
                    for pt in adjusted_tasks {
                        pending.push(pt);
                    }
                }
            }));
        }

        let mut cycle_count = 0;
        while !pending_tasks.lock().await.is_empty() || !running_tasks.lock().await.is_empty() {
            cycle_count += 1;
            
            let completed = completed_tasks.lock().await.len();
            let running = running_tasks.lock().await.len();
            let pending = pending_tasks.lock().await.len();
            
            self.log_trace(None, &format!("=== SCHEDULING CYCLE {} ===", cycle_count));
            self.log_trace(None, &format!("Queue status at start: completed={}, running={}, pending={}", completed, running, pending));

            let available_slots = self.config.max_concurrent_agents - running;
            self.log_trace(None, &format!("Available execution slots: {}", available_slots));
            
            if available_slots > 0 {
                let mut ready_tasks: Vec<PriorityTask> = Vec::new();
                let completed_set = completed_tasks.lock().await.clone();
                
                let mut pending = pending_tasks.lock().await;
                let mut temp_queue = Vec::new();
                let mut skipped_deps = Vec::new();
                
                while let Some(pt) = pending.pop() {
                    let deps_met = pt.task.dependencies.iter()
                        .all(|dep| completed_set.contains(dep));
                    
                    if deps_met {
                        self.log_trace(Some(&pt.task.id), &format!("Task dependencies met, added to ready queue"));
                        ready_tasks.push(pt);
                    } else {
                        let missing_deps: Vec<_> = pt.task.dependencies.iter()
                            .filter(|dep| !completed_set.contains(*dep))
                            .cloned()
                            .collect();
                        skipped_deps.push((pt.task.id.clone(), missing_deps));
                        temp_queue.push(pt);
                    }
                }
                
                for pt in temp_queue {
                    pending.push(pt);
                }
                
                drop(pending);
                
                if !skipped_deps.is_empty() {
                    self.log_trace(None, &format!("{} tasks waiting on dependencies", skipped_deps.len()));
                    for (task_id, deps) in skipped_deps {
                        self.log_trace(Some(&task_id), &format!("Waiting on: {:?}", deps));
                    }
                }
                
                ready_tasks.sort_by(|a, b| b.cmp(a));
                
                if !ready_tasks.is_empty() {
                    self.log_trace(None, &format!("Ready tasks sorted by priority (top {}):", ready_tasks.len().min(5)));
                    for (i, pt) in ready_tasks.iter().enumerate().take(5) {
                        self.log_trace(Some(&pt.task.id), &format!("  #{}: Priority={:?}, Phase={}", 
                            i+1, pt.priority, pt.task.phase));
                    }
                }
                
                let ready_count = ready_tasks.len();
                let tasks_to_run = ready_tasks.into_iter().take(available_slots).collect::<Vec<_>>();
                
                self.log_info(None, &format!("Found {} ready tasks, executing {} ({} slots available)", 
                    ready_count, tasks_to_run.len(), available_slots));
                
                for pt in tasks_to_run {
                    self.log_info(Some(&pt.task.id), &format!("STARTING - Priority: {:?}, Agent: {}, Phase: {}, Duration: {}min", 
                        pt.priority, pt.task.agent_role.to_string(), pt.task.phase, pt.task.estimated_duration_minutes));
                    
                    running_tasks.lock().await.insert(pt.task.id.clone());
                    task_start_times.lock().await.insert(pt.task.id.clone(), Instant::now());
                    
                    self.log_trace(Some(&pt.task.id), &format!("Added to running_tasks set. Current running: {}", 
                        running_tasks.lock().await.len()));
                    
                    let provider_clone = provider.clone();
                    let semaphore_clone = semaphore.clone();
                    let event_tx_clone = self.event_tx.clone();
                    let task_clone = pt.task.clone();
                    let task_results_clone = Arc::new(Mutex::new(task_results.clone()));
                    let completed_tasks_clone = completed_tasks.clone();
                    let running_tasks_clone = running_tasks.clone();
                    let task_start_times_clone = task_start_times.clone();
                    let orchestrator = self.clone();
                    
                    tokio::spawn(async move {
                        let task_id = task_clone.id.clone();
                        
                        orchestrator.log_trace(Some(&task_id), "Waiting for execution slot...");
                        let _permit = match semaphore_clone.acquire().await {
                            Ok(p) => {
                                orchestrator.log_trace(Some(&task_id), "Execution slot acquired");
                                p
                            }
                            Err(_) => {
                                orchestrator.log_error(Some(&task_id), "CRITICAL: Failed to acquire semaphore");
                                return;
                            }
                        };
                        
                        let start = Instant::now();
                        orchestrator.log_debug(Some(&task_id), "Task execution began");
                        
                        let result = Self::execute_task(&task_clone, &provider_clone, &task_results_clone).await;
                        
                        let duration_ms = start.elapsed().as_millis();
                        
                        orchestrator.log_trace(Some(&task_id), "Task execution complete, updating state");
                        
                        task_start_times_clone.lock().await.remove(&task_id);
                        running_tasks_clone.lock().await.remove(&task_id);
                        
                        orchestrator.log_trace(Some(&task_id), &format!("Removed from running_tasks. Still running: {}", 
                            running_tasks_clone.lock().await.len()));
                        
                        match result {
                            Ok(output) => {
                                task_results_clone.lock().await.insert(task_id.clone(), output.clone());
                                completed_tasks_clone.lock().await.insert(task_id.clone());
                                
                                orchestrator.log_info(Some(&task_id), &format!("COMPLETED - Duration: {}ms", duration_ms));
                                orchestrator.log_trace(Some(&task_id), &format!("Added to completed_tasks. Total completed: {}", 
                                    completed_tasks_clone.lock().await.len()));
                                
                                let _ = event_tx_clone.send(WorkflowEvent {
                                    event_type: "task_completed".to_string(),
                                    task_id: Some(task_id),
                                    message: "Task completed".to_string(),
                                    data: Some(output),
                                    timestamp: std::time::SystemTime::now()
                                        .duration_since(std::time::UNIX_EPOCH)
                                        .unwrap()
                                        .as_millis() as u64,
                                });
                            }
                            Err(e) => {
                                orchestrator.log_error(Some(&task_id), &format!("FAILED - Error: {}", e));
                                orchestrator.log_trace(Some(&task_id), &format!("Not added to completed_tasks. Still completed: {}", 
                                    completed_tasks_clone.lock().await.len()));
                                
                                let _ = event_tx_clone.send(WorkflowEvent {
                                    event_type: "task_failed".to_string(),
                                    task_id: Some(task_id),
                                    message: format!("Task failed: {}", e),
                                    data: None,
                                    timestamp: std::time::SystemTime::now()
                                        .duration_since(std::time::UNIX_EPOCH)
                                        .unwrap()
                                        .as_millis() as u64,
                                });
                            }
                        }
                    });
                }
            } else {
                self.log_trace(None, "No available slots, waiting...");
            }
            
            self.log_trace(None, &format!("Cycle {} complete, sleeping 100ms", cycle_count));
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        
        if let Some(task) = priority_adjust_task {
            task.abort();
        }

        self.log_info(None, "All tasks completed");

        Ok(serde_json::json!({
            "results": task_results,
            "plan_id": plan.id,
            "total_tasks": plan.tasks.len(),
            "completed_tasks": completed_tasks.lock().await.len(),
        }))
    }

    async fn execute_task(
        task: &TaskDefinition,
        provider: &ResolvedProvider,
        task_results: &Arc<Mutex<HashMap<String, serde_json::Value>>>,
    ) -> Result<serde_json::Value> {
        let system_prompt = format!(
            "You are a {} AI agent. Execute the following task and provide your findings in clear markdown format.",
            task.agent_role.to_string()
        );

        let mut context = String::new();
        if !task.dependencies.is_empty() {
            let results = task_results.lock().await;
            for dep in &task.dependencies {
                if let Some(result) = results.get(dep) {
                    context.push_str(&format!("Dependency {} output:\n{}\n\n", dep, result));
                }
            }
        }

        let user_message = format!(
            "Context:\n{}\n\nTask:\n{}\n\nInstructions:\n{}\n\nProvide your detailed response:",
            context, task.name, task.description
        );

        match provider.provider.api_format {
            ApiFormat::Anthropic => Self::call_anthropic(provider, &system_prompt, &user_message).await,
            ApiFormat::OpenAI => Self::call_openai(provider, &system_prompt, &user_message).await,
        }
    }

    async fn call_anthropic(provider: &ResolvedProvider, system_prompt: &str, user_message: &str) -> Result<serde_json::Value> {
        let client = AnthropicClient::new();
        let messages = vec![AnthropicMessage {
            role: "user".to_string(),
            content: AnthropicContent::Text(user_message.to_string()),
        }];
        let response = client
            .send_message(provider, messages, Some(system_prompt), vec![], 4096)
            .await?;
        let text = response
            .content
            .iter()
            .filter_map(|block| match block {
                crate::native_engine::anthropic_client::ContentBlock::Text { text } => Some(text.clone()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("");
        Ok(serde_json::json!({ "output": text }))
    }

    async fn call_openai(provider: &ResolvedProvider, system_prompt: &str, user_message: &str) -> Result<serde_json::Value> {
        let client = OpenAIClient::new();
        let messages = vec![OpenAIMessage {
            role: "user".to_string(),
            content: OpenAIContent::Text(user_message.to_string()),
            tool_calls: None,
            tool_call_id: None,
            reasoning_content: None,
        }];
        let response = client
            .send_message(provider, messages, Some(system_prompt), vec![], 4096)
            .await?;
        let text = response
            .choices
            .first()
            .map(|c| match &c.message.content {
                OpenAIContent::Text(t) => t.clone(),
                OpenAIContent::Multi(parts) => parts
                    .iter()
                    .filter_map(|p| match p {
                        crate::native_engine::openai_client::OpenAIContentPart::Text { text } => Some(text.clone()),
                        _ => None,
                    })
                    .collect::<Vec<_>>()
                    .join(""),
            })
            .unwrap_or_default();
        Ok(serde_json::json!({ "output": text }))
    }

    pub async fn list_workflows(&self) -> Vec<WorkPlan> {
        self.gstack.list_workplans().await
    }

    pub async fn get_workflow(&self, plan_id: &str) -> Option<WorkPlan> {
        self.gstack.get_workplan(plan_id).await
    }

    pub async fn get_scheduling_stats(&self) -> serde_json::Value {
        let completed = self.completed_tasks.lock().await.len();
        let running = self.running_tasks.lock().await.len();
        let pending = self.pending_tasks.lock().await.len();
        
        serde_json::json!({
            "completed_tasks": completed,
            "running_tasks": running,
            "pending_tasks": pending,
            "max_concurrent_agents": self.config.max_concurrent_agents,
            "priority_scheduling_enabled": self.config.enable_priority_scheduling,
        })
    }
}

impl Clone for MultiAgentOrchestrator {
    fn clone(&self) -> Self {
        Self {
            config: self.config.clone(),
            event_tx: self.event_tx.clone(),
            http_client: self.http_client.clone(),
            openspace: self.openspace.clone(),
            superpowers: self.superpowers.clone(),
            gstack: self.gstack.clone(),
            task_priorities: self.task_priorities.clone(),
            task_start_times: self.task_start_times.clone(),
            pending_tasks: self.pending_tasks.clone(),
            running_tasks: self.running_tasks.clone(),
            completed_tasks: self.completed_tasks.clone(),
        }
    }
}
