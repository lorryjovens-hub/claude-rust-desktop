use super::{McpServerConfig, McpServerManager, McpServerState, McpServerStatus};
use anyhow::{ensure, Context, Result};
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    fs::OpenOptions,
    io::Write,
    path::{Path, PathBuf},
    sync::Mutex,
};

#[derive(Deserialize)]
struct Legacy {
    name: String,
    command: String,
    args: Vec<String>,
    env: HashMap<String, String>,
    cwd: Option<PathBuf>,
    auto_start: bool,
}

#[derive(Debug, Serialize)]
pub struct Report {
    pub imported: Vec<String>,
    pub skipped: Vec<String>,
    pub preview: bool,
    pub backup: Option<PathBuf>,
}

// All manager writes use this lock and revision check, including ordinary edits.
pub(super) fn persist(
    path: &Path,
    revision: &Mutex<Option<Vec<u8>>>,
    configs: &HashMap<String, McpServerConfig>,
    backup: bool,
) -> Result<Option<PathBuf>> {
    let parent = path
        .parent()
        .context("MCP config needs a parent directory")?;
    std::fs::create_dir_all(parent)?;
    let lock = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(path.with_extension("lock"))?;
    lock.try_lock()
        .context("MCP configuration is being edited; retry")?;
    let mut revision = revision.lock().unwrap();
    let current = match std::fs::read(path) {
        Ok(bytes) => Some(bytes),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => return Err(e.into()),
    };
    ensure!(
        *revision == current,
        "MCP configuration changed in another process; reopen this host before editing"
    );
    let backup = if backup {
        if let Some(bytes) = &current {
            let directory = parent.join("backups");
            std::fs::create_dir_all(&directory)?;
            let target = directory.join(format!("mcp-{}.json", uuid::Uuid::new_v4()));
            let mut file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&target)?;
            file.write_all(bytes)?;
            file.sync_all()?;
            Some(target)
        } else {
            None
        }
    } else {
        None
    };
    let bytes = serde_json::to_vec_pretty(&serde_json::json!({"mcpServers": configs}))?;
    let mut file = tempfile::NamedTempFile::new_in(parent)?;
    file.write_all(&bytes)?;
    file.as_file().sync_all()?;
    file.persist(path)?;
    *revision = Some(bytes);
    Ok(backup)
}

impl McpServerManager {
    pub async fn import_legacy(&self, source: &Path, preview: bool) -> Result<Report> {
        let unloaded = self.revision.lock().unwrap().is_none();
        if unloaded && self.config_path.exists() {
            self.load_config().await?;
        }
        ensure!(
            std::fs::metadata(source)?.len() <= 4 * 1024 * 1024,
            "MCP settings exceed 4 MiB"
        );
        let raw: serde_json::Value = serde_json::from_slice(&std::fs::read(source)?)?;
        let entries: Vec<Legacy> = serde_json::from_value(
            raw.get("mcp_servers")
                .context("Expected legacy settings.json with mcp_servers array")?
                .clone(),
        )?;
        ensure!(entries.len() <= 1000, "Too many MCP servers");
        let _lifecycle = self.lifecycle_lock.lock().await;
        let mut servers = self.servers.write().await;
        let mut configs: HashMap<_, _> = servers
            .iter()
            .map(|(id, s)| (id.clone(), s.config.clone()))
            .collect();
        let mut report = Report {
            imported: vec![],
            skipped: vec![],
            preview,
            backup: None,
        };
        let mut seen = HashSet::new();
        for entry in entries {
            ensure!(
                !entry.name.trim().is_empty()
                    && entry.name.len() <= 256
                    && seen.insert(entry.name.clone()),
                "Empty, oversized or duplicate MCP name"
            );
            ensure!(!entry.command.trim().is_empty(), "MCP command is empty");
            if let Some(cwd) = &entry.cwd {
                ensure!(
                    cwd.is_absolute() && cwd.is_dir(),
                    "MCP working directory must be an existing absolute directory"
                );
            }
            let config = McpServerConfig {
                id: entry.name.clone(),
                name: entry.name,
                command: entry.command,
                args: entry.args,
                env: entry.env,
                cwd: entry.cwd,
                enabled: entry.auto_start,
            };
            if let Some(prior) = configs.get(&config.id) {
                ensure!(
                    prior == &config,
                    "MCP server {} conflicts with existing configuration",
                    config.id
                );
                report.skipped.push(config.id);
            } else {
                report.imported.push(config.id.clone());
                configs.insert(config.id.clone(), config);
            }
        }
        if !preview && !report.imported.is_empty() {
            report.backup = persist(&self.config_path, &self.revision, &configs, true)?;
            for id in &report.imported {
                let config = configs.remove(id).unwrap();
                let status = McpServerStatus {
                    id: id.clone(),
                    name: config.name.clone(),
                    command: config.command.clone(),
                    args: config.args.clone(),
                    env: Some(config.env.clone()),
                    cwd: config.cwd.clone(),
                    enabled: config.enabled,
                    running: false,
                    pid: None,
                    tools_count: 0,
                    resources_count: 0,
                    error: None,
                    transport_type: "stdio".into(),
                };
                servers.insert(
                    id.clone(),
                    McpServerState {
                        config,
                        status,
                        connector: None,
                    },
                );
            }
        }
        Ok(report)
    }
}
