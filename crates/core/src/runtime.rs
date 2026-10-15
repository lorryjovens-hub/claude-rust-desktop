//! Host-independent initialization and persistence. No window or HTTP server is needed.
use crate::{
    cache::FileCache,
    config::{AppConfig, ProviderConfig},
    db::{conversation_repo, message_repo, project_repo, DbManager},
    native_engine::{
        provider_manager::{ApiFormat, ModelConfig, Provider, ProviderManager},
        NativeEngine,
    },
    permissions::{AuditLogger, PermissionManager},
};
use anyhow::{ensure, Context, Result};
use serde_json::{json, Value};
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};
use tokio::sync::Mutex;

#[derive(Clone, Debug)]
pub struct RuntimePaths {
    pub data_dir: PathBuf,
}

impl RuntimePaths {
    pub fn new(data_dir: PathBuf) -> Result<Self> {
        ensure!(data_dir.is_absolute(), "Data directory must be absolute");
        Ok(Self { data_dir })
    }

    /// Matches Tauri's app_data_dir for com.claude.desktop on desktop platforms.
    pub fn default_data_dir() -> Result<PathBuf> {
        Ok(dirs::data_dir()
            .context("Cannot locate application data directory; use --data-dir")?
            .join("com.claude.desktop"))
    }

    pub fn open_database(&self) -> Result<Arc<DbManager>> {
        let db = Arc::new(DbManager::new(self.data_dir.join("claude_desktop.db")).context("Opening shared database pool")?);
        db.init().context("Initializing shared database schema")?;
        Ok(db)
    }

    /// Read without silently discarding malformed configuration or creating defaults.
    pub fn read_config(&self) -> Result<AppConfig> {
        let path = self.data_dir.join("config.json");
        if !path.exists() {
            return Ok(AppConfig::default());
        }
        serde_json::from_str(&std::fs::read_to_string(&path)?)
            .with_context(|| format!("Invalid config: {}", path.display()))
    }

    pub fn providers(&self) -> Result<Vec<Provider>> {
        if self.data_dir.join("config.json").exists() {
            return Ok(self
                .read_config()?
                .providers
                .iter()
                .map(provider_from_config)
                .collect());
        }
        let path = self.data_dir.join("providers.json");
        if !path.exists() {
            return Ok(Vec::new());
        }
        serde_json::from_str(&std::fs::read_to_string(&path)?)
            .with_context(|| format!("Invalid providers: {}", path.display()))
    }
}

/// One mapping for Desktop's saved configuration and the standalone CLI.
pub fn provider_from_config(p: &ProviderConfig) -> Provider {
    Provider {
        id: p.id.clone(),
        name: p.name.clone(),
        base_url: p.base_url.clone(),
        api_key: p.api_key.clone().unwrap_or_default(),
        api_format: if p.provider_type == "anthropic" && !p.base_url.contains("deepseek") {
            ApiFormat::Anthropic
        } else {
            ApiFormat::OpenAI
        },
        builtin_id: p
            .settings
            .get("builtinId")
            .and_then(Value::as_str)
            .map(String::from),
        enabled: p.enabled,
        web_search_strategy: p.web_search_strategy.clone(),
        models: p
            .models
            .iter()
            .map(|m| ModelConfig {
                id: m.id.clone(),
                name: m.name.clone(),
                enabled: m.enabled,
                max_tokens: m.max_tokens,
                context_window: m.context_window,
                supports_text: m.supports_text,
                supports_vision: m.supports_vision,
                api: m.api.clone(),
                effort: None,
                supports_web_search: false,
            })
            .collect(),
    }
}

pub fn create_engine(
    paths: &RuntimePaths,
    db: Arc<DbManager>,
    providers: Arc<Mutex<ProviderManager>>,
    permissions: Arc<PermissionManager>,
    cache: Arc<FileCache>,
) -> NativeEngine {
    NativeEngine::new(
        providers,
        db,
        paths.data_dir.join("workspaces"),
        permissions,
        cache,
    )
}

pub fn default_permissions() -> Arc<PermissionManager> {
    Arc::new(PermissionManager::new(Arc::new(AuditLogger::new(1000))))
}

/// Sessions use the same project trust, Code prompt and SQLite schema as Desktop.
pub struct SessionStore {
    pub db: Arc<DbManager>,
}

impl SessionStore {
    pub fn new(db: Arc<DbManager>) -> Self {
        Self { db }
    }

    pub fn create(&self, workspace: &Path, model: &str, provider: &str) -> Result<String> {
        let workspace =
            std::fs::canonicalize(workspace).context("Project directory is unavailable")?;
        ensure!(workspace.is_dir(), "Project path must be a directory");
        let path = workspace.to_string_lossy().into_owned();
        let id = uuid::Uuid::new_v4().to_string();
        self.db.with_conn(|conn| -> Result<()> {
            // Acquire the writer before reading project state, avoiding a stale WAL snapshot upgrade.
            let tx = rusqlite::Transaction::new_unchecked(conn, rusqlite::TransactionBehavior::Immediate)?;
            let project = project_repo::list_projects(&tx)?.into_iter().find(|p| {
                !p.is_archived
                    && p.workspace_path
                        .as_deref()
                        .and_then(|s| std::fs::canonicalize(s).ok())
                        .as_ref()
                        == Some(&workspace)
                    && p.folders.iter().all(|f| f.trusted)
            });
            let now = chrono::Utc::now().to_rfc3339();
            let project_id = if let Some(project) = project {
                project.id
            } else {
                let project_id = uuid::Uuid::new_v4().to_string();
                let name = workspace.file_name().unwrap_or_default().to_string_lossy();
                project_repo::insert_project(
                    &tx,
                    &project_id,
                    &name,
                    None,
                    None,
                    Some(&path),
                    false,
                    &now,
                    &now,
                )?;
                project_repo::set_folders(&tx, &project_id, &[path.clone()])?;
                // Invoking CLI in this directory (or passing --path) selects this project.
                project_repo::trust_folders(&tx, &project_id, &[path.clone()])?;
                project_id
            };
            conversation_repo::insert_conversation(
                &tx,
                &id,
                Some("CLI session"),
                Some(model),
                Some(provider),
                Some(&path),
                Some(&project_id),
                false,
                false,
                false,
                &now,
                &now,
                0,
            )?;
            conversation_repo::set_workspace_mode(&tx, &id, "code")?;
            tx.commit()?;
            Ok(())
        })??;
        Ok(id)
    }

    pub fn resume(&self, id: &str) -> Result<conversation_repo::ConversationRow> {
        self.db.with_conn(|conn| -> Result<_> {
            let session =
                conversation_repo::get_conversation(conn, id)?.context("Session not found")?;
            ensure!(
                session.workspace_mode == "code",
                "Only Code sessions can be resumed in CLI"
            );
            ensure!(!session.archived, "Session is archived");
            crate::native_engine::workspace::conversation_project(conn, id)?;
            Ok(session)
        })?
    }

    pub fn history(&self, id: &str) -> Result<Vec<Value>> {
        self.db.with_conn(|conn| -> Result<_> {
            let rows = message_repo::get_messages_by_conversation(conn, id)?;
            Ok(rows
                .into_iter()
                .map(|m| json!({"role":m.role,"content":m.content}))
                .collect())
        })?
    }

    /// Rebuild protocol-valid tool history from the shared SQLite tool rows.
    pub fn model_history(&self, id: &str, format: &ApiFormat) -> Result<Vec<Value>> {
        self.db.with_conn(|conn| -> Result<_> {
            let mut history = Vec::new();
            for message in message_repo::get_messages_by_conversation(conn, id)? {
                let calls = message_repo::get_tool_calls_for_message(conn, &message.id)?;
                if message.role == "assistant" && !calls.is_empty() {
                    match format {
                        ApiFormat::OpenAI => {
                            let tool_calls: Vec<_> = calls.iter().map(|t| json!({"id":t.id,"type":"function","function":{"name":t.name,"arguments":t.input.as_deref().unwrap_or("{}")}})).collect();
                            history.push(json!({"role":"assistant","content":"","tool_calls":tool_calls}));
                            for tool in &calls { history.push(json!({"role":"tool","tool_call_id":tool.id,"content":tool.output.as_deref().unwrap_or("")})); }
                        }
                        ApiFormat::Anthropic => {
                            let blocks: Vec<_> = calls.iter().map(|t| json!({"type":"tool_use","id":t.id,"name":t.name,"input":t.input.as_deref().and_then(|s| serde_json::from_str::<Value>(s).ok()).unwrap_or(json!({}))})).collect();
                            history.push(json!({"role":"assistant","content":blocks}));
                            let results: Vec<_> = calls.iter().map(|t| json!({"type":"tool_result","tool_use_id":t.id,"content":t.output.as_deref().unwrap_or(""),"is_error":t.is_error})).collect();
                            history.push(json!({"role":"user","content":results}));
                        }
                    }
                }
                if !message.content.is_empty() { history.push(json!({"role":message.role,"content":message.content})); }
            }
            Ok(history)
        })?
    }

    pub fn append(&self, id: &str, role: &str, text: &str) -> Result<String> {
        let message_id = uuid::Uuid::new_v4().to_string();
        self.db.with_conn(|conn| -> Result<()> {
            // Message ordering is a read/modify/write operation shared by CLI and Desktop hosts.
            let tx = rusqlite::Transaction::new_unchecked(conn, rusqlite::TransactionBehavior::Immediate)?;
            let order: i64 = tx.query_row(
                "SELECT COALESCE(MAX(sort_order), -1) + 1 FROM messages WHERE conversation_id = ?1",
                [id],
                |row| row.get(0),
            )?;
            let now = chrono::Utc::now().to_rfc3339();
            message_repo::insert_message(
                &tx,
                &message_id,
                id,
                role,
                text,
                None,
                &now,
                false,
                order,
            )?;
            conversation_repo::increment_message_count(&tx, id)?;
            conversation_repo::update_conversation_timestamp(&tx, id, &now)?;
            tx.commit()?;
            Ok(())
        })??;
        Ok(message_id)
    }
}
