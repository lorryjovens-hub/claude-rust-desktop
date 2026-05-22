use crate::db::DbManager;
use crate::native_engine::provider_manager::ProviderManager;
use crate::native_engine::tool_loop::{EngineEvent, ToolLoopExecutor};
use crate::mcp::McpToolRegistry;
use crate::permissions::{PermissionContext, PermissionManager, PermissionResult};
use crate::skills::{SkillExecutionContext, SkillExecutionEngine, SkillsManager};
use anyhow::{anyhow, Result};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex, oneshot};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatRequest {
    pub conversation_id: String,
    pub messages: Vec<Value>,
    pub model: String,
    pub system_prompt: Option<String>,
    pub max_tokens: Option<u32>,
    pub workspace_path: Option<String>,
    pub temperature: Option<f32>,
    pub top_p: Option<f32>,
}

#[derive(Debug)]
pub struct ActiveTurn {
    pub event_tx: mpsc::Sender<EngineEvent>,
    pub executor_handle: Option<tokio::task::JoinHandle<()>>,
    pub cancelled: bool,
}

#[derive(Debug, Clone)]
pub struct ConversationState {
    pub conversation_id: String,
    pub model: String,
    pub messages: Vec<Value>,
    pub system_prompt: Option<String>,
    pub last_activity: String,
    pub turn_count: usize,
    pub status: ConversationStatus,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConversationStatus {
    Idle,
    Active,
    WaitingForUser,
    Completed,
    Error,
}

#[derive(Debug, Clone)]
pub struct ToolCallRecord {
    pub tool_use_id: String,
    pub tool_name: String,
    pub tool_input: Value,
    pub output: String,
    pub is_error: bool,
    pub timestamp: String,
}

pub struct QueryEngine {
    provider_manager: Arc<Mutex<ProviderManager>>,
    db_manager: Arc<DbManager>,
    active_turns: Arc<Mutex<HashMap<String, ActiveTurn>>>,
    conversation_states: Arc<Mutex<HashMap<String, ConversationState>>>,
    workspaces_dir: PathBuf,
    mcp_registry: Option<Arc<McpToolRegistry>>,
    answer_waiters: Arc<Mutex<HashMap<String, oneshot::Sender<String>>>>,
    tool_call_history: Arc<Mutex<HashMap<String, Vec<ToolCallRecord>>>>,
    permission_manager: Arc<PermissionManager>,
    skills_manager: Option<Arc<Mutex<SkillsManager>>>,
}

impl QueryEngine {
    pub fn new(
        provider_manager: Arc<Mutex<ProviderManager>>,
        db_manager: Arc<DbManager>,
        workspaces_dir: PathBuf,
        permission_manager: Arc<PermissionManager>,
    ) -> Self {
        Self {
            provider_manager,
            db_manager,
            active_turns: Arc::new(Mutex::new(HashMap::new())),
            conversation_states: Arc::new(Mutex::new(HashMap::new())),
            workspaces_dir,
            mcp_registry: None,
            answer_waiters: Arc::new(Mutex::new(HashMap::new())),
            tool_call_history: Arc::new(Mutex::new(HashMap::new())),
            permission_manager,
            skills_manager: None,
        }
    }

    pub fn with_mcp_registry(mut self, registry: Arc<McpToolRegistry>) -> Self {
        self.mcp_registry = Some(registry);
        self
    }

    pub fn with_skills_manager(mut self, manager: Arc<Mutex<SkillsManager>>) -> Self {
        self.skills_manager = Some(manager);
        self
    }

    pub async fn check_tool_permission(
        &self,
        tool_name: &str,
        tool_input: Value,
        conversation_id: &str,
        workspace_path: Option<String>,
    ) -> PermissionResult {
        let context = PermissionContext {
            tool_name: tool_name.to_string(),
            tool_input,
            conversation_id: conversation_id.to_string(),
            user_id: None,
            workspace_path,
        };
        self.permission_manager.check_permission(&context)
    }

    pub async fn execute_skill(
        &self,
        skill_id: &str,
        conversation_id: &str,
        messages: Vec<Value>,
        workspace_path: Option<String>,
    ) -> Result<String> {
        let skills_manager = self.skills_manager.as_ref()
            .ok_or_else(|| anyhow!("Skills manager not configured"))?;
        
        let manager = skills_manager.lock().await;
        
        let context = SkillExecutionContext {
            conversation_id: conversation_id.to_string(),
            messages,
            available_tools: crate::tools::get_tool_definitions(),
            available_mcp_tools: Vec::new(),
            current_input: "".to_string(),
            workspace_path,
            variables: std::collections::HashMap::new(),
            mcp_server_manager: None,
        };
        
        manager.execute_skill(skill_id, "", Some(context)).await
    }

    pub async fn confirm_tool_permission(&self, conversation_id: &str, tool_name: &str) {
        self.permission_manager.confirm_permission(conversation_id, tool_name);
    }

    pub async fn set_permission_mode(&self, mode: crate::permissions::PermissionMode) {
        self.permission_manager.set_mode(mode);
    }

    pub async fn sync_providers(&self, providers: Vec<crate::native_engine::provider_manager::Provider>) {
        let mut pm = self.provider_manager.lock().await;
        for provider in providers {
            let id = provider.id.clone();
            pm.update_provider(&id, provider);
        }
    }

    pub async fn resolve_provider(&self, model_id: &str) -> Option<crate::native_engine::provider_manager::ResolvedProvider> {
        let pm = self.provider_manager.lock().await;
        pm.resolve_provider(model_id)
    }

    pub async fn get_conversation_state(&self, conv_id: &str) -> Option<ConversationState> {
        let states = self.conversation_states.lock().await;
        states.get(conv_id).cloned()
    }

    pub async fn update_conversation_state(&self, conv_id: &str, state: ConversationState) {
        let mut states = self.conversation_states.lock().await;
        states.insert(conv_id.to_string(), state);
    }

    pub async fn load_conversation_state(&self, conv_id: &str) -> Result<Option<ConversationState>> {
        let db = self.db_manager.clone();
        let conv_id_clone = conv_id.to_string();
        
        let result = tokio::task::spawn_blocking(move || -> anyhow::Result<Option<ConversationState>> {
            db.with_conn(|conn| {
                let messages = crate::db::message_repo::get_messages_by_conversation(conn, &conv_id_clone)?;
                let conv = crate::db::conversation_repo::get_conversation(conn, &conv_id_clone)?;
                
                if messages.is_empty() && conv.is_none() {
                    return Ok(None);
                }
                
                let model = conv.as_ref().and_then(|c| c.model.as_deref()).unwrap_or("");
                
                let messages_json: Vec<Value> = messages.into_iter().map(|msg| {
                    serde_json::json!({
                        "role": msg.role,
                        "content": msg.content,
                    })
                }).collect();
                
                Ok(Some(ConversationState {
                    conversation_id: conv_id_clone,
                    model: model.to_string(),
                    messages: messages_json,
                    system_prompt: None,
                    last_activity: Utc::now().to_rfc3339(),
                    turn_count: 0,
                    status: ConversationStatus::Idle,
                }))
            })?
        }).await??;
        
        if let Some(state) = &result {
            self.update_conversation_state(conv_id, state.clone()).await;
        }
        
        Ok(result)
    }

    pub async fn save_conversation_state(&self, conv_id: &str) -> Result<()> {
        let states = self.conversation_states.lock().await;
        if let Some(state) = states.get(conv_id) {
            let db = self.db_manager.clone();
            let state_clone = state.clone();
            
            tokio::task::spawn_blocking(move || -> anyhow::Result<()> {
                let result = db.with_conn(|conn| {
                    let tx = conn.unchecked_transaction()?;
                    
                    let existing = crate::db::conversation_repo::get_conversation(&tx, &state_clone.conversation_id)?;
                    if existing.is_none() {
                        crate::db::conversation_repo::insert_conversation(
                            &tx,
                            &state_clone.conversation_id,
                            None,
                            Some(&state_clone.model),
                            None,
                            None,
                            None,
                            false,
                            false,
                            false,
                            &state_clone.last_activity,
                            &state_clone.last_activity,
                            state_clone.turn_count as i64,
                        )?;
                    } else {
                        crate::db::conversation_repo::update_conversation(
                            &tx,
                            &state_clone.conversation_id,
                            None,
                            Some(&state_clone.model),
                            None,
                            None,
                            None,
                            None,
                            None,
                            None,
                            Some(&state_clone.last_activity),
                            Some(state_clone.turn_count as i64),
                        )?;
                    }
                    
                    tx.commit()?;
                    Ok(())
                });
                result?
            }).await??;
        }
        Ok(())
    }

    pub async fn send_message(&self, request: ChatRequest) -> Result<mpsc::Receiver<EngineEvent>> {
        let conv_id = request.conversation_id.clone();
        let model = request.model.clone();
        let workspace_path = request.workspace_path.clone().unwrap_or_else(|| ".".to_string());

        let provider = {
            let pm = self.provider_manager.lock().await;
            pm.resolve_provider(&model)
                .ok_or_else(|| anyhow!("No provider found for model: {}", model))?
        };

        let (event_tx, event_rx) = mpsc::channel::<EngineEvent>(100);

        let state = ConversationState {
            conversation_id: conv_id.clone(),
            model: model.clone(),
            messages: request.messages.clone(),
            system_prompt: request.system_prompt.clone(),
            last_activity: Utc::now().to_rfc3339(),
            turn_count: 0,
            status: ConversationStatus::Active,
        };
        self.update_conversation_state(&conv_id, state).await;

        let mut executor = ToolLoopExecutor::new(
            provider,
            request.messages,
            request.system_prompt,
            request.max_tokens.unwrap_or(8192),
            event_tx.clone(),
            workspace_path,
        )
        .with_conv_id(conv_id.clone())
        .with_answer_waiters(self.answer_waiters.clone())
        .with_permission_manager(self.permission_manager.clone());

        if let Some(ref registry) = self.mcp_registry {
            executor = executor.with_mcp_registry(registry.clone());
        }

        let conv_id_clone = conv_id.clone();
        let active_turns_clone = self.active_turns.clone();
        let conversation_states_clone = self.conversation_states.clone();
        let tool_call_history_clone = self.tool_call_history.clone();
        let db = self.db_manager.clone();

        let executor_handle = tokio::spawn(async move {
            match executor.execute().await {
                Ok((full_text, _stop_reason)) => {
                    if !full_text.is_empty() {
                        let conv_id = conv_id_clone.clone();
                        let _ = tokio::task::spawn_blocking(move || {
                            db.with_conn(|conn| {
                                let msg_id = Uuid::new_v4().to_string();
                                let now = Utc::now().to_rfc3339();
                                let sort_order = crate::db::message_repo::get_messages_by_conversation(conn, &conv_id)
                                    .unwrap_or_default()
                                    .len() as i64;
                                crate::db::message_repo::insert_message(
                                    conn, &msg_id, &conv_id, "assistant", &full_text, None, &now, false, sort_order,
                                )?;
                                crate::db::conversation_repo::increment_message_count(conn, &conv_id)?;
                                Ok::<(), anyhow::Error>(())
                            })
                        }).await;
                    }
                }
                Err(e) => {
                    eprintln!("[QueryEngine] Error in turn for {}: {}", conv_id_clone, e);
                    let mut states = conversation_states_clone.lock().await;
                    if let Some(state) = states.get_mut(&conv_id_clone) {
                        state.status = ConversationStatus::Error;
                        state.last_activity = Utc::now().to_rfc3339();
                    }
                }
            }
            
            let mut turns = active_turns_clone.lock().await;
            if let Some(turn) = turns.get_mut(&conv_id_clone) {
                turn.cancelled = true;
            }
            turns.remove(&conv_id_clone);
            
            let mut states = conversation_states_clone.lock().await;
            if let Some(state) = states.get_mut(&conv_id_clone) {
                state.status = ConversationStatus::Completed;
                state.last_activity = Utc::now().to_rfc3339();
                state.turn_count += 1;
            }
        });

        {
            let mut turns = self.active_turns.lock().await;
            turns.insert(conv_id.clone(), ActiveTurn {
                event_tx: event_tx.clone(),
                executor_handle: Some(executor_handle),
                cancelled: false,
            });
        }

        Ok(event_rx)
    }

    pub async fn cancel_turn(&self, conv_id: &str) {
        let mut turns = self.active_turns.lock().await;
        if let Some(turn) = turns.get_mut(conv_id) {
            turn.cancelled = true;
            if let Some(handle) = turn.executor_handle.take() {
                handle.abort();
            }
        }
        turns.remove(conv_id);

        let mut states = self.conversation_states.lock().await;
        if let Some(state) = states.get_mut(conv_id) {
            state.status = ConversationStatus::Idle;
            state.last_activity = Utc::now().to_rfc3339();
        }
    }

    pub async fn resume_with_answer(&self, conv_id: &str, answer: String) -> Result<()> {
        let mut waiters = self.answer_waiters.lock().await;
        if let Some(tx) = waiters.remove(conv_id) {
            tx.send(answer).map_err(|_| anyhow!("Failed to send answer: receiver already dropped"))?;
            Ok(())
        } else {
            anyhow::bail!("No pending AskUserQuestion for conversation {}", conv_id)
        }
    }

    pub async fn record_tool_call(&self, conv_id: &str, record: ToolCallRecord) {
        let mut history = self.tool_call_history.lock().await;
        history.entry(conv_id.to_string())
            .or_insert_with(Vec::new)
            .push(record);
    }

    pub async fn get_tool_call_history(&self, conv_id: &str) -> Vec<ToolCallRecord> {
        let history = self.tool_call_history.lock().await;
        history.get(conv_id).cloned().unwrap_or_default()
    }

    pub fn get_workspaces_dir(&self) -> &PathBuf {
        &self.workspaces_dir
    }

    pub async fn list_active_conversations(&self) -> Vec<String> {
        let states = self.conversation_states.lock().await;
        states.values()
            .filter(|s| s.status == ConversationStatus::Active)
            .map(|s| s.conversation_id.clone())
            .collect()
    }

    pub async fn cleanup_inactive_conversations(&self, max_idle_minutes: u64) -> usize {
        let now = Utc::now();
        let mut removed_count = 0;
        
        let mut states = self.conversation_states.lock().await;
        let inactive_ids: Vec<String> = states.iter()
            .filter(|(_, s)| {
                if let Ok(last_activity) = chrono::DateTime::parse_from_rfc3339(&s.last_activity) {
                    let duration = now.signed_duration_since(last_activity);
                    duration.num_minutes() > max_idle_minutes as i64
                } else {
                    true
                }
            })
            .map(|(id, _)| id.clone())
            .collect();
        
        for id in inactive_ids {
            states.remove(&id);
            removed_count += 1;
        }
        
        removed_count
    }
}

pub type NativeEngine = QueryEngine;