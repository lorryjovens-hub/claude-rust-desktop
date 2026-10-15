use crate::{
    args::{Cli, OutputFormat, PermissionModeArg},
    config::Selection,
};
use anyhow::{bail, Context, Result};
use claude_core::{
    cache::FileCache,
    db::message_repo,
    mcp::{McpServerManager, McpToolRegistry},
    native_engine::{
        engine_core::ChatRequest, provider_manager::ProviderManager, tool_loop::EngineEvent,
        NativeEngine,
    },
    permissions::PermissionMode,
    runtime::{self, RuntimePaths, SessionStore},
};
use serde_json::{json, Value};
use std::{
    io::{self, Write},
    sync::Arc,
};
use tokio::sync::{mpsc, Mutex};

/// One stdin reader for the entire REPL, also used for tool questions. A detached
/// OS thread avoids Tokio's uncancellable stdin read blocking runtime shutdown.
pub fn terminal_input() -> mpsc::UnboundedReceiver<String> {
    let (tx, rx) = mpsc::unbounded_channel();
    std::thread::spawn(move || {
        let mut input = crate::terminal::input::InputReader::new();
        while let Ok(Some(line)) = input.read_line("") {
            if tx.send(line).is_err() {
                break;
            }
        }
    });
    rx
}

pub struct Session {
    pub id: String,
    pub store: SessionStore,
    pub engine: NativeEngine,
    mcp: Arc<McpServerManager>,
    pub(crate) model: String,
    pub(crate) provider_id: String,
    pub(crate) auto_route: bool,
    providers: Arc<Mutex<ProviderManager>>,
    api_format: claude_core::native_engine::provider_manager::ApiFormat,
}

impl Session {
    pub async fn open(cli: &Cli, paths: &RuntimePaths, selection: Selection) -> Result<Self> {
        claude_core::config::OrchestrationConfig::init_global()?;
        let db = paths.open_database()?;
        let store = SessionStore::new(db.clone());
        let id = if let Some(id) = &cli.resume {
            store.resume(id)?;
            id.clone()
        } else {
            store.create(
                &std::env::current_dir()?,
                &selection.model,
                &selection.provider_id,
            ).context("Creating CLI session")?
        };
        store.db.with_conn(|conn| {
            conn.execute(
                "UPDATE conversations SET model = ?1, provider = ?2 WHERE id = ?3",
                [&selection.model, &selection.provider_id, &id],
            )
        })??;
        let permissions = runtime::default_permissions();
        if let Some(crate::args::Commands::Agent { agent_type, .. }) = &cli.command {
            let definition = claude_core::services::agents::resolve(&crate::services::context(paths)?, agent_type)?;
            claude_core::services::agents::bind(&store.db, &id, &definition)?;
        }
        permissions.set_mode(match cli.permission_mode {
            PermissionModeArg::Ask => PermissionMode::AskPermissions,
            PermissionModeArg::AcceptEdits => PermissionMode::AcceptEdits,
            PermissionModeArg::Plan => PermissionMode::PlanMode,
            PermissionModeArg::Bypass => PermissionMode::BypassPermissions,
        });
        let api_format = selection
            .providers
            .iter()
            .find(|p| p.id == selection.provider_id)
            .context("Selected provider unavailable")?
            .api_format
            .clone();
        let providers = Arc::new(Mutex::new(ProviderManager::in_memory(
            selection.providers,
            paths.data_dir.join("providers.json"),
        )));
        let mcp = Arc::new(McpServerManager::new(
            paths.data_dir.join("mcp-servers.json"),
        ));
        {
            let manager = &mcp;
            manager.initialize().await?;
            for server in manager
                .list_servers()
                .await
                .into_iter()
                .filter(|s| s.enabled)
            {
                if let Err(error) = manager.start_server(&server.id).await {
                    eprintln!("MCP {}: {error}", server.name);
                }
            }
        }
        let engine = runtime::create_engine(
            paths,
            db,
            providers.clone(),
            permissions,
            Arc::new(FileCache::new()),
        )
        .with_mcp_registry(Arc::new(McpToolRegistry::new(mcp.clone())))
        .with_skills_manager(Arc::new(Mutex::new(
            claude_core::skills::SkillsManager::with_path(paths.data_dir.join("skills")),
        )));
        Ok(Self {
            id,
            store,
            engine,
            mcp,
            model: selection.model,
            provider_id: selection.provider_id,
            providers,
            auto_route: cli.auto_route,
            api_format,
        })
    }

    pub async fn switch(&mut self, paths: &RuntimePaths, selection: Selection) -> Result<()> {
        let api_format = selection
            .providers
            .iter()
            .find(|p| p.id == selection.provider_id)
            .context("Selected provider unavailable")?
            .api_format
            .clone();
        let next =
            ProviderManager::in_memory(selection.providers, paths.data_dir.join("providers.json"));
        anyhow::ensure!(
            next.resolve_selected(&selection.provider_id, &selection.model)
                .is_some(),
            "Selected model unavailable"
        );
        let mut manager = self.providers.lock().await;
        self.store.db.with_conn(|conn| {
            conn.execute(
                "UPDATE conversations SET model = ?1, provider = ?2 WHERE id = ?3",
                [&selection.model, &selection.provider_id, &self.id],
            )
        })??;
        *manager = next;
        self.model = selection.model;
        self.provider_id = selection.provider_id;
        self.api_format = api_format;
        self.auto_route = false;
        Ok(())
    }

    pub async fn close(&self) -> Result<()> {
        self.engine.cancel_turn(&self.id).await;
        self.mcp.shutdown_all().await
    }

    pub async fn turn(
        &self,
        cli: &Cli,
        prompt: &str,
        input: &mut Option<mpsc::UnboundedReceiver<String>>,
    ) -> Result<()> {
        let mut history = self.store.model_history(&self.id, &self.api_format)?;
        history.push(json!({"role":"user","content":prompt}));
        let mut system = cli.system_prompt.clone().unwrap_or_default();
        system.push_str("\nThe user is working in the standalone Claude Rust terminal frontend. No Desktop window or Bridge server is running. Use the available native tools directly.\n");
        let cwd = std::env::current_dir()?;
        for name in ["AGENTS.md", "CLAUDE.md"] {
            let path = cwd.join(name);
            if path.is_file() {
                system.push_str(&format!(
                    "\nProject instructions ({name}):\n{}",
                    std::fs::read_to_string(path)?
                ));
            }
        }
        self.store.append(&self.id, "user", prompt).context("Persisting CLI user message")?;
        let mut events = self
            .engine
            .send_message_with_history(
                ChatRequest {
                    conversation_id: self.id.clone(),
                    messages: history,
                    model: self.model.clone(),
                    provider_id: if self.auto_route { None } else { Some(self.provider_id.clone()) },
                    effort: cli.effort.clone(),
                    system_prompt: Some(system),
                    max_tokens: cli.max_tokens,
                    workspace_path: Some(cwd.to_string_lossy().into_owned()),
                    temperature: None,
                    top_p: None,
                    reasoning_mode: None,
                },
                false,
            )
            .await?;
        let mut text = String::new();
        let mut usage = json!({});
        let mut error = None;
        let mut completed = false;
        let mut snapshots = Vec::<Value>::new();
        let mut tools = Vec::<message_repo::ToolCallRow>::new();
        let turn_result: Result<()> = async {
            loop {
                let event = tokio::select! {
                    event = events.recv() => event.context("Engine stream closed before completion")?,
                    _ = tokio::signal::ctrl_c() => bail!("Interrupted"),
                };
                if cli.output_format == OutputFormat::StreamJson {
                    if let Some(value) = event_json(&event) { emit_json(&value)?; }
                }
                match event {
                    EngineEvent::Text(delta) => { text.push_str(&delta); if cli.output_format == OutputFormat::Text { print!("{delta}"); io::stdout().flush()?; } },
                    EngineEvent::Thinking(delta) if cli.verbose => eprint!("{delta}"),
                    EngineEvent::Usage(value) => { if let Some(fields) = value.as_object() { for (key, value) in fields { usage[key] = value.clone(); } } },
                    EngineEvent::Error(message) => { error = Some(message); },
                    EngineEvent::MessageStop { full_text, stop_reason } => {
                        if text.is_empty() && !full_text.is_empty() { if cli.output_format == OutputFormat::Text { print!("{full_text}"); } text = full_text; }
                        if stop_reason.as_deref().is_some_and(|s| ["error", "panic", "stream_error", "interrupted", "failed", "cancelled", "max_iterations", "budget_warning"].contains(&s)) {
                            error.get_or_insert_with(|| format!("Turn stopped: {}", stop_reason.unwrap()));
                        }
                        completed = true; break;
                    },
                    EngineEvent::ToolPermission { request_id, tool_name, input: tool_input, .. } => {
                        let reader = input.as_mut().context("Tool approval required in non-interactive mode; choose --permission-mode explicitly")?;
                        eprintln!("\nAllow {tool_name} {}? [y/N]", serde_json::to_string(&tool_input)?);
                        let answer = read_answer(reader).await?;
                        let answer = if ["y", "yes", "是", "允许"].contains(&answer.trim().to_lowercase().as_str()) { "yes" } else { "no" };
                        self.engine.resume_with_answer(&request_id, answer.into()).await?;
                    },
                    EngineEvent::AskUser { request_id, question, options } => {
                        let reader = input.as_mut().context("User input required in non-interactive mode; resume this session interactively")?;
                        eprintln!("\n{question}");
                        for (i, option) in options.iter().enumerate() { eprintln!("{}. {option}", i + 1); }
                        let answer = read_answer(reader).await?;
                        let answer = answer.trim().parse::<usize>().ok().and_then(|i| i.checked_sub(1)).and_then(|i| options.get(i)).cloned().unwrap_or(answer);
                        self.engine.resume_with_answer(&request_id, answer).await?;
                    },
                    EngineEvent::ToolUseStart { tool_name, .. } if cli.verbose => eprintln!("\n[{tool_name}]"),
                    EngineEvent::ToolUseDone { tool_use_id, tool_name, tool_input, output, is_error }
                    | EngineEvent::PipelineToolResult { tool_use_id, tool_name, tool_input, output, is_error, .. } => {
                        tools.push(message_repo::ToolCallRow { id: tool_use_id, message_id: String::new(), name: tool_name, input: Some(tool_input.to_string()), output: Some(output), is_error, sort_order: tools.len() as i64 });
                    },
                    EngineEvent::RequestSnapshot(snapshot) => {
                        if let Some(old) = snapshots.iter_mut().find(|s| s["id"] == snapshot["id"]) { *old = snapshot; } else { snapshots.push(snapshot); }
                    },
                    EngineEvent::BudgetWarning { message, .. } => eprintln!("{message}"),
                    _ => {},
                }
            }
            Ok(())
        }.await;
        // Cancel and join before persisting, including Ctrl+C and closed stdin.
        self.engine.cancel_turn(&self.id).await;
        if let Err(failure) = turn_result {
            error = Some(failure.to_string());
        }
        if !completed && error.is_none() {
            error = Some("Incomplete response".into());
        }
        if !text.is_empty() || !tools.is_empty() || !snapshots.is_empty() {
            let message_id = self.store.append(&self.id, "assistant", &text)?;
            self.store.db.with_conn(|conn| -> Result<()> {
                for tool in &tools {
                    message_repo::insert_tool_call(
                        conn,
                        &format!("{message_id}:{}", tool.id),
                        &message_id,
                        &tool.name,
                        tool.input.as_deref(),
                        tool.output.as_deref(),
                        tool.is_error,
                        tool.sort_order,
                    )?;
                }
                message_repo::save_request_snapshots(conn, &message_id, &json!(snapshots))?;
                Ok(())
            })??;
        }
        match cli.output_format {
            OutputFormat::Text => println!(),
            _ => emit_json(
                &json!({"type":"result","session_id":self.id,"result":text,"is_error":error.is_some(),"error":error,"usage":usage}),
            )?,
        }
        if let Some(error) = error {
            bail!("{error} (session {})", self.id);
        }
        Ok(())
    }
}

async fn read_answer(reader: &mut mpsc::UnboundedReceiver<String>) -> Result<String> {
    tokio::select! { answer = reader.recv() => answer.context("Input closed"), _ = tokio::signal::ctrl_c() => bail!("Interrupted") }
}

fn emit_json(value: &Value) -> Result<()> {
    let mut stdout = io::stdout().lock();
    serde_json::to_writer(&mut stdout, value)?;
    writeln!(stdout)?;
    stdout.flush()?;
    Ok(())
}

fn event_json(event: &EngineEvent) -> Option<Value> {
    Some(match event {
        EngineEvent::Text(delta) => json!({"type":"text","delta":delta}),
        EngineEvent::Thinking(delta) => json!({"type":"thinking","delta":delta}),
        EngineEvent::Usage(usage) => json!({"type":"usage","usage":usage}),
        EngineEvent::MessageStart { model } => json!({"type":"message_start","model":model}),
        EngineEvent::Error(error) => json!({"type":"error","error":error}),
        EngineEvent::ToolUseStart {
            tool_use_id,
            tool_name,
            tool_input,
            ..
        } => json!({"type":"tool_start","id":tool_use_id,"name":tool_name,"input":tool_input}),
        EngineEvent::ToolUseDone {
            tool_use_id,
            tool_name,
            output,
            is_error,
            ..
        }
        | EngineEvent::PipelineToolResult {
            tool_use_id,
            tool_name,
            output,
            is_error,
            ..
        } => {
            json!({"type":"tool_result","id":tool_use_id,"name":tool_name,"output":output,"is_error":is_error})
        }
        EngineEvent::AskUser {
            request_id,
            question,
            options,
        } => {
            json!({"type":"ask_user","request_id":request_id,"question":question,"options":options})
        }
        EngineEvent::ToolPermission {
            request_id,
            tool_name,
            input,
            ..
        } => json!({"type":"permission","request_id":request_id,"name":tool_name,"input":input}),
        _ => return None,
    })
}
