//! Explicit, lossless local-memory transfer. Never silently redirects a remote backend.
use super::{MemExClient, MemoryItem};
use anyhow::{ensure, Result};
use rusqlite::{params, OptionalExtension, TransactionBehavior};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Serialize, Deserialize)]
pub struct MemoryArchive {
    pub version: u32,
    pub memories: Vec<MemoryItem>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct MemoryImportReport {
    pub imported: usize,
    pub skipped: usize,
    pub preview: bool,
    pub backup: Option<String>,
}

fn parse(value: Value) -> Result<Vec<MemoryItem>> {
    let items = if value.is_array() {
        let mut items = Vec::new();
        for entry in value.as_array().unwrap() {
            // Old MemoryManager::export writes a MemoryEntry array.
            ensure!(
                entry.get("memory_type").is_some(),
                "Expected legacy MemoryEntry array"
            );
            let mut metadata = std::collections::HashMap::new();
            metadata.insert("legacy_source".into(), serde_json::to_string(entry)?);
            items.push(MemoryItem {
                id: serde_json::from_value(entry["id"].clone())?,
                content: serde_json::from_value(entry["content"].clone())?,
                importance: serde_json::from_value(entry["importance"].clone())?,
                created_at: serde_json::from_value(entry["timestamp"].clone())?,
                metadata: Some(metadata),
                similarity_score: None,
            });
        }
        items
    } else {
        let archive: MemoryArchive = serde_json::from_value(value)?;
        ensure!(archive.version == 1, "Unsupported memory archive version");
        archive.memories
    };
    for item in &items {
        ensure!(!item.id.trim().is_empty(), "Empty memory ID");
        ensure!(
            !item.content.trim().is_empty() && item.content.chars().count() <= 32_000,
            "Invalid memory content"
        );
        ensure!(
            item.importance.is_finite() && (0.0..=1.0).contains(&item.importance),
            "Invalid importance"
        );
        chrono::DateTime::parse_from_rfc3339(&item.created_at)?;
    }
    Ok(items)
}

impl MemExClient {
    pub async fn export_archive(&self) -> Result<MemoryArchive> {
        ensure!(
            self.get_config().await.backend_url == "local",
            "Memory transfer requires local storage"
        );
        let local = self.local.clone();
        tokio::task::spawn_blocking(move || {
            Ok(MemoryArchive {
                version: 1,
                memories: local.search("", usize::MAX)?,
            })
        })
        .await?
    }

    pub async fn import_archive(&self, value: Value, preview: bool) -> Result<MemoryImportReport> {
        ensure!(
            self.get_config().await.backend_url == "local",
            "Memory transfer requires local storage"
        );
        let items = parse(value)?;
        let local = self.local.clone();
        tokio::task::spawn_blocking(move || {
            let mut conn = local.open()?;
            let mut report = MemoryImportReport { imported: 0, skipped: 0, preview, backup: None };
            if !preview {
                let directory = local.path.parent().unwrap().join("backups");
                std::fs::create_dir_all(&directory)?;
                let target = directory.join(format!("memory-{}.sqlite3", uuid::Uuid::new_v4()));
                conn.execute("VACUUM INTO ?1", [target.to_string_lossy().as_ref()])?;
                report.backup = Some(target.to_string_lossy().into_owned());
            }
            // Recheck conflicts under the write lock, including writes since backup.
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            for item in items {
                let metadata = serde_json::to_string(&item.metadata)?;
                let existing: Option<(String, f64, String, String)> = tx.query_row("SELECT content, importance, created_at, metadata FROM memories WHERE id=?1", [&item.id], |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?))).optional()?;
                if let Some((content, importance, created_at, meta)) = existing {
                    ensure!(content == item.content && importance == item.importance && created_at == item.created_at && serde_json::from_str::<Value>(&meta)? == serde_json::from_str::<Value>(&metadata)?, "Memory ID conflict: {}", item.id);
                    report.skipped += 1;
                    continue;
                }
                // A different ID with identical content cannot be represented losslessly
                // by the existing unique-content store. Report it instead of dropping metadata.
                ensure!(!tx.query_row("SELECT EXISTS(SELECT 1 FROM memories WHERE content=?1)", [&item.content], |r| r.get::<_,bool>(0))?, "Memory content conflict: {}", item.id);
                tx.execute("INSERT INTO memories(id,content,importance,created_at,metadata) VALUES (?1,?2,?3,?4,?5)", params![item.id,item.content,item.importance,item.created_at,metadata])?;
                report.imported += 1;
            }
            if preview { tx.rollback()?; } else { tx.commit()?; }
            Ok(report)
        }).await?
    }
}
