use anyhow::Context;
use crate::cost_tracker::{BudgetCheckResult, CostTracker};
use crate::db::DbManager;
use crate::native_engine::provider_manager::ProviderManager;
use crate::native_engine::tool_loop::{EngineEvent, ToolLoopExecutor};
use crate::mcp::McpToolRegistry;
use crate::permissions::{PermissionContext, PermissionManager, PermissionResult, audit::{AuditAction, AuditEntry}};
use crate::prefetch::PrefetchEngine;
use crate::skills::SkillsManager;
use crate::cache::FileCache;
use anyhow::{anyhow, Result};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::{mpsc, Mutex, oneshot};
use futures::FutureExt;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReasoningMode {
    #[serde(alias = "quick")]
    Quick,
    #[serde(alias = "standard")]
    Standard,
    #[serde(alias = "deep")]
    Deep,
}

pub fn analyze_complexity(message: &str) -> ReasoningMode {
    let msg_lower = message.to_lowercase();
    let has_code = msg_lower.contains("function") || msg_lower.contains("class ") || msg_lower.contains("def ") || msg_lower.contains("import ") || msg_lower.contains("const ") || msg_lower.contains("let ") || msg_lower.contains("fn ");
    let has_file_op = msg_lower.contains("file") || msg_lower.contains("编辑") || msg_lower.contains("修改") || msg_lower.contains("删除") || msg_lower.contains("write") || msg_lower.contains("read") || msg_lower.contains("目录");
    let has_danger = msg_lower.contains("delete") || msg_lower.contains("rm ") || msg_lower.contains("drop ") || msg_lower.contains("删除") || msg_lower.contains("格式化") || msg_lower.contains("format");
    let is_long = message.len() > 200;

    if has_danger {
        ReasoningMode::Deep
    } else if has_code || has_file_op || is_long {
        ReasoningMode::Standard
    } else {
        ReasoningMode::Quick
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatRequest {
    pub conversation_id: String,
    pub messages: Vec<Value>,
    pub model: String,
    #[serde(default)]
    pub provider_id: Option<String>,
    #[serde(default)]
    pub effort: Option<String>,
    pub system_prompt: Option<String>,
    pub max_tokens: Option<u32>,
    pub workspace_path: Option<String>,
    pub temperature: Option<f32>,
    pub top_p: Option<f32>,
    pub reasoning_mode: Option<ReasoningMode>,
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
    file_cache: Arc<FileCache>,
    cost_tracker: Option<Arc<CostTracker>>,
}

impl QueryEngine {
    /// No tools, automatic memory ingestion, detached tasks, or conversation mutations.
    /// Dropping this future cancels the underlying HTTP/runtime request.
    pub async fn generate_text(&self, provider_id: &str, model: &str, system: &str, input: &str) -> Result<String> {
        let provider = self.provider_manager.lock().await.resolve_selected(provider_id, model)
            .ok_or_else(|| anyhow!("Provider/model is unavailable: {provider_id}::{model}"))?;
        super::generation::generate(provider, system, input).await
    }
    pub fn new(
        provider_manager: Arc<Mutex<ProviderManager>>,
        db_manager: Arc<DbManager>,
        workspaces_dir: PathBuf,
        permission_manager: Arc<PermissionManager>,
        file_cache: Arc<FileCache>,
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
            file_cache,
            cost_tracker: None,
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

    pub fn with_cost_tracker(mut self, tracker: Arc<CostTracker>) -> Self {
        self.cost_tracker = Some(tracker);
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

    /// Apply an enabled skill to the normal conversation executor and stream real results.
    pub async fn execute_skill(&self, skill_id: &str, mut request: ChatRequest) -> Result<mpsc::Receiver<EngineEvent>> {
        let manager = self.skills_manager.as_ref().context("Skills manager not configured")?;
        let content = {
            let manager = manager.lock().await;
            let skill = manager.get_skill_by_id(skill_id).await?.filter(|s| s.enabled).context("Skill not found or disabled")?;
            skill.content.context("Skill has no content")?
        };
        request.system_prompt.get_or_insert_with(String::new).push_str(&format!("\nSelected skill instructions:\n{content}\n"));
        self.send_message(request).await
    }

    pub async fn confirm_tool_permission(&self, conversation_id: &str, tool_name: &str) {
        self.permission_manager.confirm_permission(conversation_id, tool_name);
    }

    pub async fn set_permission_mode(&self, mode: crate::permissions::PermissionMode) {
        self.permission_manager.set_mode(mode);
    }

    pub async fn sync_providers(&self, providers: Vec<crate::native_engine::provider_manager::Provider>) {
        let mut pm = self.provider_manager.lock().await;
        let config_path = pm.config_path().clone();
        *pm = ProviderManager::from_providers(providers, config_path);
    }

    pub async fn resolve_provider(&self, model_id: &str) -> Option<crate::native_engine::provider_manager::ResolvedProvider> {
        let pm = self.provider_manager.lock().await;
        pm.resolve_provider(model_id).await
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
        self.send_message_with_history(request, true).await
    }

    /// The desktop SSE route owns its assistant row so snapshots and results share one ID.
    pub async fn send_message_with_history(&self, mut request: ChatRequest, save_response: bool) -> Result<mpsc::Receiver<EngineEvent>> {
        let command = request.messages.iter().rev().find(|m| m["role"] == "user")
            .and_then(|m| m["content"].as_str()).and_then(|text| text.trim_start().strip_prefix('/'))
            .and_then(|text| text.split_whitespace().next()).map(str::to_string);
        if let (Some(command), Some(manager)) = (command, &self.skills_manager) {
            let skills = manager.lock().await.list_enabled_skills().await?;
            let matches: Vec<_> = skills.into_iter().filter(|skill| skill.id == command || skill.name.to_lowercase().split_whitespace().collect::<Vec<_>>().join("-") == command).collect();
            anyhow::ensure!(matches.len() <= 1, "Ambiguous skill command; use its full skill ID");
            if let Some(content) = matches.into_iter().next().and_then(|skill| skill.content) {
                request.system_prompt.get_or_insert_with(String::new).push_str(&format!("\nSelected skill instructions:\n{content}\n"));
            }
        }
        let conv_id = request.conversation_id.clone();
        let plugin_root=self.provider_manager.lock().await.config_path().parent().context("Missing runtime directory")?.to_path_buf();
        let plugin_context=crate::services::ServiceContext{paths:crate::runtime::RuntimePaths::new(plugin_root)?,db:self.db_manager.clone(),credentials:Arc::new(crate::services::EnvironmentCredentials)};
        if let Some(router) = plugin_context.setting::<crate::performance::RouterConfig>("performance.router")? {
            // Route only a fresh plain-text turn; never switch models mid tool conversation.
            if router.enabled && request.provider_id.is_none() && request.messages.len() == 1 && request.messages[0]["role"] == "user"
                && request.messages[0]["content"].as_str().is_some_and(crate::performance::simple_turn) {
                let manager = self.provider_manager.lock().await;
                if manager.resolve_selected(&router.provider_id, &router.model_id).is_some() {
                    request.provider_id = Some(router.provider_id);
                    request.model = router.model_id;
                } else { anyhow::bail!("Configured simple-turn routing target is unavailable"); }
            }
        }
        let plugins=crate::services::plugins::list(&plugin_context)?;
        if plugins.iter().any(|p|p.enabled) {
            request.system_prompt.get_or_insert_with(String::new).push_str(&format!("\nAvailable PluginRun commands: {}\n{}",serde_json::to_string(&plugins.iter().filter(|p|p.enabled).collect::<Vec<_>>())?,crate::services::plugins::prompt_hooks(&plugin_context)?));
        }
        let agent = crate::services::agents::for_session(&self.db_manager, &conv_id)?;
        if let Some(definition) = &agent {
            request.system_prompt.get_or_insert_with(String::new).push_str(&format!("\nAgent role ({}):\n{}", definition.id, definition.system_prompt));
        }
        let db = self.db_manager.clone();
        let project_conv_id = conv_id.clone();
        let workspaces_root = self.workspaces_dir.clone();
        let requested_workspace = request.workspace_path.clone();
        let (workspace, project, prompt_profile) = tokio::task::spawn_blocking(move || db.with_conn(|conn| {
            let (workspace, project) = super::workspace::resolve_workspace(conn, &workspaces_root, &project_conv_id, requested_workspace.as_deref())?;
            let profile = super::workspace::conversation_prompt_profile(conn, &project_conv_id)?;
            Ok::<_, anyhow::Error>((workspace, project, profile))
        })).await???;
        request.system_prompt = Some(crate::prompt::compose_system_prompt(
            request.system_prompt.as_deref(), &request.model, prompt_profile,
        ));
        let workspace_path = workspace.to_string_lossy().into_owned();
        request.workspace_path = Some(workspace_path.clone());
        request.system_prompt.get_or_insert_with(String::new).push_str(&format!("\nWorking directory: {workspace_path}\n"));
        if let Some(project) = project {
            let folders = project.folders.iter().map(|f| f.path.as_str()).collect::<Vec<_>>();
            let context = format!("\nProject: {}\nProject source folders (the first is the working directory):\n{}\nProject instructions:\n{}", project.name, serde_json::to_string(&folders)?, project.instructions.unwrap_or_default());
            request.system_prompt.get_or_insert_with(String::new).push_str(&context);
        }
        let team_context=crate::services::team::model_context(&plugin_context,&workspace)?;
        request.system_prompt.get_or_insert_with(String::new).push_str(&team_context);
        let model = request.model.clone();

        tracing::info!(module = "EngineCore", "send_message: conv_id={}, model={}, workspace={}", conv_id, model, workspace_path);

        if let Some(ref tracker) = self.cost_tracker {
            let estimated_tokens = request.messages.iter()
                .filter_map(|m| m.get("content").and_then(|c| c.as_str()))
                .map(|c| (c.len() as u64) / 4)
                .sum::<u64>();
            let estimated_total = estimated_tokens + 2048;

            match tracker.check_budget(estimated_total) {
                BudgetCheckResult::Exceeded(msg) => {
                    tracing::warn!(module = "EngineCore", "Budget exceeded: {}", msg);
                    anyhow::bail!("Budget exceeded: {}", msg);
                }
                BudgetCheckResult::Warning(msg, usage, limit) => {
                    tracing::warn!(module = "EngineCore", "Budget warning: {}", msg);
                    let (warn_tx, warn_rx) = mpsc::channel::<EngineEvent>(10);
                    let _ = warn_tx.send(EngineEvent::BudgetWarning {
                        message: msg,
                        usage,
                        limit,
                    }).await;
                    let _ = warn_tx.send(EngineEvent::MessageStop {
                        full_text: String::new(),
                        stop_reason: Some("budget_warning".to_string()),
                    }).await;
                    return Ok(warn_rx);
                }
                BudgetCheckResult::WithinBudget => {}
            }
        }

        let reasoning_mode = request.reasoning_mode.clone().unwrap_or_else(|| {
            let first_user_msg = request.messages.iter()
                .find(|m| m.get("role").and_then(|r| r.as_str()) == Some("user"))
                .and_then(|m| m.get("content"))
                .and_then(|c| c.as_str())
                .unwrap_or("");
            analyze_complexity(first_user_msg)
        });

        tracing::info!(module = "EngineCore", "Reasoning mode: {:?}", reasoning_mode);

        let mut provider = {
            let pm = self.provider_manager.lock().await;
            let resolved = if let Some(id) = request.provider_id.as_deref() { pm.resolve_selected(id, &model) } else { pm.resolve_provider(&model).await };
            tracing::info!(module = "EngineCore", "resolve_provider({}): {:?}", model, resolved.as_ref().map(|p| &p.provider.id));
            resolved.ok_or_else(|| anyhow!("No provider found for model: {}", model))?
        };

        let (event_tx, event_rx) = mpsc::channel::<EngineEvent>(500);
        let event_tx_for_turn = event_tx.clone();

        let state = ConversationState {
            conversation_id: conv_id.clone(),
            model: request.provider_id.as_ref().map(|id| format!("{}::{}", id, model)).unwrap_or_else(|| model.clone()),
            messages: request.messages.clone(),
            system_prompt: request.system_prompt.clone(),
            last_activity: Utc::now().to_rfc3339(),
            turn_count: 0,
            status: ConversationStatus::Active,
        };
        self.update_conversation_state(&conv_id, state).await;
        tracing::info!(module = "EngineCore", "Conversation state updated");

        if let Some(effort) = request.effort.as_deref() {
            if !["off", "low", "high", "max"].contains(&effort) { anyhow::bail!("Invalid effort"); }
        }
        provider.model.effort = request.effort.clone();
        validate_model_input(&provider.model, &request.messages)?;
        let configured_max = provider.model.max_tokens.unwrap_or(8192);
        let max_tokens = request.max_tokens.unwrap_or(configured_max).min(configured_max);
        if max_tokens == 0 { anyhow::bail!("max_tokens must be positive"); }
        if let Some(context) = provider.model.context_window {
            let estimated = crate::native_engine::token_counter::TokenCounter::estimate(&request.messages) as u64;
            if estimated + max_tokens as u64 > context as u64 { anyhow::bail!("Input plus output budget exceeds the configured context window; compact the conversation or lower max output tokens"); }
        }

        let search_config_path = self.provider_manager.lock().await.config_path()
            .parent().unwrap_or(std::path::Path::new(".")).join("config.json");
        let learning = if plugin_context.setting::<bool>("learning.enabled")?.unwrap_or(false) {
            Some(crate::learning::TurnObservation::new(plugin_context.clone(), conv_id.clone()))
        } else { None };
        let mut executor = ToolLoopExecutor::new(
            provider,
            request.messages,
            request.system_prompt,
            max_tokens,
            event_tx.clone(),
            workspace_path.clone(),
            reasoning_mode.clone(),
        )
        .with_learning_tools(learning.as_ref().map(|l| l.tools.clone()))
        .with_allowed_tools(agent.and_then(|d|d.tools))
        .with_prompt_profile(prompt_profile)
        .with_memex(crate::memory::MemExClient::with_path(
            self.workspaces_dir.parent().unwrap_or(&self.workspaces_dir).join("memory.sqlite3"), None,
        ))
        .with_search_config_path(search_config_path)
        .with_conv_id(conv_id.clone())
        .with_answer_waiters(self.answer_waiters.clone())
        .with_permission_manager(self.permission_manager.clone())
        .with_prefetch_engine(Arc::new(PrefetchEngine::new(self.file_cache.clone(), workspace_path.clone())));

        if let Some(ref registry) = self.mcp_registry {
            executor = executor.with_mcp_registry(registry.clone());
        }

        let conv_id_clone = conv_id.clone();
        let active_turns_clone = self.active_turns.clone();
        let conversation_states_clone = self.conversation_states.clone();
        let _tool_call_history_clone = self.tool_call_history.clone();
        let db = self.db_manager.clone();
        let provider_manager_title = self.provider_manager.clone();
        let db_title = self.db_manager.clone();

        tracing::info!(module = "EngineCore", "Spawning executor task for conv_id={}", conv_id_clone);

        let executor_handle = tokio::spawn(async move {
            let mut learning = learning;
            tracing::info!(module = "EngineCore", "Executor task STARTED for {}", conv_id_clone);

            // 使用 catch_unwind 捕获 panic
            let exec_result = tokio::select! {
                biased;
                result = std::panic::AssertUnwindSafe(executor.execute()).catch_unwind() => result,
                _ = event_tx.closed() => Ok(Err(anyhow!("Response receiver disconnected; turn cancelled"))),
            };

            let exec_result = match exec_result {
                Ok(result) => result,
                Err(panic_info) => {
                    let panic_msg = if let Some(s) = panic_info.downcast_ref::<&str>() {
                        s.to_string()
                    } else if let Some(s) = panic_info.downcast_ref::<String>() {
                        s.clone()
                    } else {
                        "Unknown panic".to_string()
                    };
                    tracing::error!(module = "EngineCore", "Executor PANIC for {}: {}", conv_id_clone, panic_msg);
                    let _ = event_tx.send(EngineEvent::Error(format!("Engine panic: {}", panic_msg))).await;
                    let _ = event_tx.send(EngineEvent::MessageStop {
                        full_text: String::new(),
                        stop_reason: Some("panic".to_string()),
                    }).await;
                    return;
                }
            };

            let successful_turn = matches!(&exec_result, Ok((_, reason)) if !matches!(reason.as_deref(), Some("stream_error" | "error" | "interrupted" | "failed" | "cancelled" | "max_iterations")));
            if let Some(observation) = learning.as_mut() {
                observation.complete(matches!(&exec_result, Ok((_, reason)) if !matches!(reason.as_deref(), Some("stream_error" | "error" | "interrupted" | "failed" | "cancelled" | "max_iterations"))));
            }
            drop(learning);
            match exec_result {
                Ok((full_text, _stop_reason)) => {
                    tracing::info!(module = "EngineCore", "Executor completed for {}, full_text_len={}", conv_id_clone, full_text.len());
                    if save_response && !full_text.is_empty() {
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
                    tracing::error!(module = "QueryEngine", "Error in turn for {}: {}", conv_id_clone, e);
                    // Note: execute() already sends EngineEvent::Error + MessageStop to event_tx,
                    // so the frontend will receive a proper error message instead of channel closed.
                    let mut states = conversation_states_clone.lock().await;
                    if let Some(state) = states.get_mut(&conv_id_clone) {
                        state.status = ConversationStatus::Error;
                        state.last_activity = Utc::now().to_rfc3339();
                    }
                }
            }

            let mut turns = active_turns_clone.lock().await;
            if turns.get(&conv_id_clone).is_some_and(|turn| !turn.event_tx.same_channel(&event_tx)) { return; }
            if let Some(turn) = turns.get_mut(&conv_id_clone) {
                turn.cancelled = true;
            }
            turns.remove(&conv_id_clone);

            let mut states = conversation_states_clone.lock().await;
            if let Some(state) = states.get_mut(&conv_id_clone) {
                state.status = if successful_turn { ConversationStatus::Completed } else { ConversationStatus::Error };
                state.last_activity = Utc::now().to_rfc3339();
                let current_turn_count = state.turn_count;
                state.turn_count += 1;

                if successful_turn && current_turn_count == 0 {
                    let title_conv_id = conv_id_clone.clone();
                    let title_model = state.model.clone();
                    let pm = provider_manager_title.clone();
                    let db_t = db_title.clone();
                    tokio::spawn(async move {
                        tracing::info!(module = "EngineCore", "Auto-generating title for conv_id={}", title_conv_id);
                        let title_conv_id_inner = title_conv_id.clone();
                        let result = tokio::task::spawn_blocking(move || {
                            db_t.with_conn(|conn| {
                                let conv = crate::db::conversation_repo::get_conversation(conn, &title_conv_id_inner)?;
                                if conv.as_ref().and_then(|c| c.title.as_ref()).map_or(false, |t| !t.is_empty()) {
                                    return Ok::<bool, anyhow::Error>(false);
                                }
                                let messages = crate::db::message_repo::get_messages_by_conversation(conn, &title_conv_id_inner)?;
                                let _first_user_msg = messages.iter().find(|m| m.role == "user").map(|m| m.content.clone());
                                let _first_assistant_msg = messages.iter().find(|m| m.role == "assistant").map(|m| m.content.clone());
                                Ok::<bool, anyhow::Error>(true)
                            })
                        }).await;

                        let should_generate = match result {
                            Ok(Ok(Ok(true))) => true,
                            _ => false,
                        };

                        if should_generate {
                            let resolved = {
                                let pm_guard = pm.lock().await;
                                pm_guard.resolve_provider(&title_model).await
                            };

                            if let Some(resolved) = resolved {
                                let db_msgs = db_title.clone();
                                let conv_id_for_msgs = title_conv_id.clone();
                                let msgs_result = tokio::task::spawn_blocking(move || {
                                    db_msgs.with_conn(|conn| {
                                        let messages = crate::db::message_repo::get_messages_by_conversation(conn, &conv_id_for_msgs)?;
                                        let first_user = messages.iter().find(|m| m.role == "user").map(|m| m.content.clone());
                                        let first_assistant = messages.iter().find(|m| m.role == "assistant").map(|m| m.content.clone());
                                        Ok::<_, anyhow::Error>((first_user, first_assistant))
                                    })
                                }).await;

                                if let Ok(Ok(Ok((Some(user_msg), assistant_msg)))) = msgs_result {
                                    let assistant_text: String = assistant_msg.unwrap_or_default();
                                    let truncated_assistant = if assistant_text.chars().count() > 200 {
                                        let mut end = 0;
                                        for (i, _) in assistant_text.char_indices() {
                                            if i >= 200 { break; }
                                            end = i;
                                        }
                                        assistant_text[..end].to_string()
                                    } else {
                                        assistant_text
                                    };

                                    let prompt = format!(
                                        "Based on the following conversation, generate a concise title (max 50 characters, in the same language as the user's message). Only output the title, nothing else.\n\nUser: {}\nAssistant: {}",
                                        user_msg, truncated_assistant
                                    );

                                    let user_msg_ref: &str = &user_msg;
                                    let fallback: String = if user_msg_ref.chars().count() > 30 {
                                        let mut end = 0;
                                        for (i, _) in user_msg_ref.char_indices() {
                                            if i >= 30 { break; }
                                            end = i;
                                        }
                                        user_msg_ref[..end].to_string()
                                    } else {
                                        user_msg.clone()
                                    };

                                    let title_result: Result<String, anyhow::Error> = match resolved.provider.api_format {
                                        crate::native_engine::provider_manager::ApiFormat::OpenAI => {
                                            let client = crate::native_engine::openai_client::OpenAIClient::new();
                                            let msgs = vec![
                                                crate::native_engine::openai_client::OpenAIMessage {
                                                    role: "user".to_string(),
                                                    content: crate::native_engine::openai_client::OpenAIContent::Text(prompt),
                                                    tool_calls: None,
                                                    tool_call_id: None,
                                                    reasoning_content: None, provider_content: None,
                                                }
                                            ];
                                            match client.send_message(&resolved, msgs, None, vec![], 100).await {
                                                Ok(resp) => {
                                                    let t = resp.choices.first()
                                                        .map(|c| match &c.message.content {
                                                            crate::native_engine::openai_client::OpenAIContent::Text(t) => t.trim().to_string(),
                                                            crate::native_engine::openai_client::OpenAIContent::Multi(parts) => parts.iter()
                                                                .filter_map(|p| if let crate::native_engine::openai_client::OpenAIContentPart::Text { text } = p { Some(text.as_str()) } else { None })
                                                                .collect::<Vec<_>>()
                                                                .join(""),
                                                        })
                                                        .unwrap_or_default();
                                                    Ok(t)
                                                }
                                                Err(e) => Err(e),
                                            }
                                        }
                                        crate::native_engine::provider_manager::ApiFormat::Anthropic => {
                                            let client = crate::native_engine::anthropic_client::AnthropicClient::new();
                                            let msgs = vec![
                                                crate::native_engine::anthropic_client::AnthropicMessage {
                                                    role: "user".to_string(),
                                                    content: crate::native_engine::anthropic_client::AnthropicContent::Text(prompt),
                                                }
                                            ];
                                            match client.send_message(&resolved, msgs, None, vec![], 100).await {
                                                Ok(resp) => {
                                                    let t = resp.content.iter()
                                                        .filter_map(|block| if let crate::native_engine::anthropic_client::ContentBlock::Text { text } = block { Some(text.as_str()) } else { None })
                                                        .collect::<Vec<_>>()
                                                        .join("")
                                                        .trim()
                                                        .to_string();
                                                    Ok(t)
                                                }
                                                Err(e) => Err(e),
                                            }
                                        }
                                    };

                                    let final_title = match title_result {
                                        Ok(t) if !t.is_empty() => t,
                                        _ => fallback,
                                    };

                                    let db_save = db_title.clone();
                                    let conv_id_save = title_conv_id.clone();
                                    let title_save = final_title.clone();
                                    let _ = tokio::task::spawn_blocking(move || {
                                        db_save.with_conn(|conn| {
                                            crate::db::conversation_repo::update_title_if_empty(conn, &conv_id_save, &title_save)
                                        })
                                    }).await;
                                    tracing::info!(module = "EngineCore", "Title generated for {}: {}", title_conv_id, final_title);
                                }
                            }
                        }
                    });
                }
            }
        });

        {
            let mut turns = self.active_turns.lock().await;
            turns.insert(conv_id.clone(), ActiveTurn {
                event_tx: event_tx_for_turn,
                executor_handle: Some(executor_handle),
                cancelled: false,
            });
        }

        Ok(event_rx)
    }

    pub async fn cancel_turn(&self, conv_id: &str) {
        let turn = self.active_turns.lock().await.remove(conv_id);
        if let Some(mut turn) = turn {
            if let Some(handle) = turn.executor_handle.take() {
                handle.abort();
                let _ = handle.await;
            }
        }
        self.answer_waiters.lock().await.retain(|_, sender| !sender.is_closed());

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

    pub async fn record_tool_call(
        &self,
        conv_id: &str,
        tool_name: &str,
        tool_input: &Value,
        output: &str,
        is_error: bool,
        message_id: Option<&str>,
    ) {
        let start = Instant::now();
        let elapsed = start.elapsed();

        let record = ToolCallRecord {
            tool_use_id: message_id.unwrap_or("").to_string(),
            tool_name: tool_name.to_string(),
            tool_input: tool_input.clone(),
            output: output.to_string(),
            is_error,
            timestamp: Utc::now().to_rfc3339(),
        };

        let mut history = self.tool_call_history.lock().await;
        history.entry(conv_id.to_string())
            .or_insert_with(Vec::new)
            .push(record);

        let _audit_entry = AuditEntry::new(
            if is_error { AuditAction::ToolCancelled } else { AuditAction::ToolExecuted },
            tool_name,
            conv_id,
        )
        .with_result(if is_error { "Error" } else { "Success" })
        .with_details(serde_json::json!({
            "input": tool_input,
            "output": output,
            "is_error": is_error,
            "duration_ms": elapsed.as_millis(),
            "message_id": message_id,
        }));

        tracing::info!(
            module = "Audit",
            "Tool: {} | Conv: {} | Error: {} | Duration: {}ms",
            tool_name, conv_id, is_error, elapsed.as_millis()
        );
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

fn validate_model_input(model: &crate::native_engine::provider_manager::ModelConfig, messages: &[Value]) -> Result<()> {
    for message in messages.iter().filter(|m| m["role"] == "user") {
        let content = &message["content"];
        if content.as_str().is_some_and(|s| !s.is_empty()) && !model.supports_text { anyhow::bail!("Selected model does not accept Text input"); }
        if let Some(parts) = content.as_array() {
            for part in parts {
                match part["type"].as_str().unwrap_or("") {
                    "image" | "image_url" | "input_image" if !model.supports_vision => anyhow::bail!("Selected model does not accept Image input"),
                    "text" | "input_text" if !model.supports_text => anyhow::bail!("Selected model does not accept Text input"),
                    _ => {}
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod provider_input_tests {
    use super::*;
    #[test]
    fn provider_input_capabilities_are_enforced() {
        let mut model: crate::native_engine::provider_manager::ModelConfig = serde_json::from_value(serde_json::json!({"id":"m","name":"M","enabled":true,"max_tokens":32000,"supports_vision":false,"supports_web_search":false})).unwrap();
        assert!(validate_model_input(&model,&[serde_json::json!({"role":"user","content":"hello"})]).is_ok());
        assert!(validate_model_input(&model,&[serde_json::json!({"role":"user","content":[{"type":"image_url","image_url":{"url":"data:image/png;base64,AA=="}}]})]).is_err());
        model.supports_text = false;
        assert!(validate_model_input(&model,&[serde_json::json!({"role":"user","content":"hello"})]).is_err());
    }
}
