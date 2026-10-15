//! Explicit conversion of the historical AgentsService JSON schema.
use super::{
    agents::{self, Definition},
    ServiceContext,
};
use anyhow::{ensure, Context, Result};
use rusqlite::{OptionalExtension, TransactionBehavior};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LegacyDefinition {
    pub agent_type: String,
    pub name: String,
    pub description: String,
    pub when_to_use: String,
    pub tools: Vec<String>,
    pub model: String,
    pub system_prompt: String,
    pub source: String,
    pub base_dir: String,
}

#[derive(Debug, Serialize)]
pub struct Report {
    pub definition: Definition,
    pub preview: bool,
    pub imported: bool,
    pub skipped: bool,
    pub backup: Option<String>,
    pub notes: Vec<String>,
}

pub fn import(context: &ServiceContext, source: &Path, id: &str, preview: bool) -> Result<Report> {
    ensure!(
        std::fs::metadata(source)?.len() <= 1024 * 1024,
        "Agent definition exceeds 1 MiB"
    );
    let legacy: LegacyDefinition = serde_json::from_slice(&std::fs::read(source)?)
        .context("Expected an old AgentDefinition JSON object")?;
    ensure!(
        matches!(
            legacy.agent_type.as_str(),
            "ClaudeCodeGuide" | "Explore" | "GeneralPurpose" | "Plan" | "Verification" | "Custom"
        ),
        "Unknown legacy agent_type"
    );
    ensure!(legacy.tools.len() <= 100, "Too many legacy tools");
    let mut tools = Vec::new();
    for tool in &legacy.tools {
        let mapped = match tool.as_str() {
            "file_read" => "Read",
            "file_write" => "Write",
            "file_edit" => "Edit",
            "search" => "Grep",
            "list_files" => "ListDir",
            "execute_command" => "Bash",
            _ => anyhow::bail!("Unsupported legacy tool: {tool}; define an explicit current-schema tool allowlist instead"),
        }.to_string();
        if !tools.contains(&mapped) {
            tools.push(mapped);
        }
    }
    let definition = Definition {
        id: id.to_string(),
        description: legacy.description.clone(),
        system_prompt: legacy.system_prompt.clone(),
        tools: Some(tools),
        legacy: Some(legacy),
    };
    agents::validate_custom(&definition)?;
    let backup = if preview {
        None
    } else {
        let directory = context.paths.data_dir.join("backups");
        std::fs::create_dir_all(&directory)?;
        let path = directory.join(format!("agents-{}.sqlite3", uuid::Uuid::new_v4()));
        context
            .db
            .with_conn(|c| c.execute("VACUUM INTO ?1", [path.to_string_lossy().as_ref()]))??;
        Some(path.to_string_lossy().into_owned())
    };
    context.db.with_conn(|c| -> Result<Report> {
        let tx = rusqlite::Transaction::new_unchecked(c, TransactionBehavior::Immediate)?;
        let raw: Option<String> = tx.query_row("SELECT value FROM peripheral_settings WHERE key='agent-definitions'", [], |r| r.get(0)).optional()?;
        let mut definitions: Vec<Definition> = raw.map(|v| serde_json::from_str(&v)).transpose()?.unwrap_or_default();
        let skipped = if let Some(prior) = definitions.iter().find(|d| d.id == definition.id) {
            ensure!(prior == &definition, "Agent ID {} conflicts with an existing definition", definition.id);
            true
        } else { definitions.push(definition.clone()); false };
        if !preview && !skipped {
            tx.execute("INSERT INTO peripheral_settings(key,value) VALUES('agent-definitions',?1) ON CONFLICT(key) DO UPDATE SET value=excluded.value", [serde_json::to_string(&definitions)?])?;
        }
        if preview { tx.rollback()?; } else { tx.commit()?; }
        Ok(Report { definition, preview, imported: !preview && !skipped, skipped, backup,
            notes: vec!["Legacy model/source/base_dir are preserved as provenance. Execution uses the explicitly selected host model and trusted project, as the old executor did not apply those fields.".into(), "Mapped tools are a strict allowlist, including an empty list; import does not execute an agent.".into()] })
    })?
}
