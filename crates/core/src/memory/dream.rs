use super::{MemExClient, MemoryItem};
use anyhow::{ensure, Result};
use rusqlite::{params, OptionalExtension, TransactionBehavior};
use serde::Serialize;
use serde_json::{json, Value};

#[derive(Serialize)]
pub struct DreamStatus {
    pub pending: usize,
    pub last_run: Option<String>,
    pub runs: usize,
}

impl MemExClient {
    pub async fn dream_sources(&self) -> Result<Vec<MemoryItem>> {
        let config = self.get_config().await;
        ensure!(
            config.enabled && config.backend_url == "local",
            "Dream requires enabled local memory"
        );
        let conn = self.local.open()?;
        let mut items = self.local.search("", usize::MAX)?;
        items.reverse();
        let mut pending = Vec::new();
        for item in items {
            if item
                .metadata
                .as_ref()
                .and_then(|m| m.get("type"))
                .is_some_and(|t| t == "dream")
            {
                continue;
            }
            // Do not send known credential-bearing memory to the model.
            if super::ContextManager::contains_credentials(&item.content) {
                continue;
            }
            let snapshot: Option<String> = conn
                .query_row(
                    "SELECT snapshot FROM dream_sources WHERE id=?1",
                    [&item.id],
                    |r| r.get(0),
                )
                .optional()?;
            if snapshot
                .as_deref()
                .map(serde_json::from_str::<Value>)
                .transpose()?
                .as_ref()
                != Some(&serde_json::to_value(&item)?)
            {
                pending.push(item);
            }
        }
        Ok(pending)
    }
    pub async fn dream_status(&self) -> Result<DreamStatus> {
        let pending = self.dream_sources().await?.len();
        let (last_run, runs) = self.local.open()?.query_row(
            "SELECT MAX(created_at), COUNT(*) FROM dream_runs",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        Ok(DreamStatus {
            pending,
            last_run,
            runs,
        })
    }

    /// Short synchronous commit: no background write can outlive cancellation.
    pub fn commit_dream(&self, sources: &[MemoryItem], summary: &str) -> Result<String> {
        ensure!(
            !sources.is_empty() && !summary.trim().is_empty() && summary.chars().count() <= 32_000,
            "Invalid dream result"
        );
        ensure!(
            !super::ContextManager::contains_credentials(summary),
            "Generated summary contains credential-like content"
        );
        let mut conn = self.local.open()?;
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        if let Some(config) = tx
            .query_row("SELECT config FROM memory_settings WHERE id=1", [], |r| {
                r.get::<_, String>(0)
            })
            .optional()?
        {
            let config: super::MemoryConfig = serde_json::from_str(&config)?;
            ensure!(
                config.enabled && config.backend_url == "local",
                "Memory configuration changed during generation"
            );
        }
        for source in sources {
            let current = tx
                .query_row(
                    "SELECT id,content,importance,created_at,metadata FROM memories WHERE id=?1",
                    [&source.id],
                    |r| {
                        Ok(MemoryItem {
                            id: r.get(0)?,
                            content: r.get(1)?,
                            importance: r.get(2)?,
                            created_at: r.get(3)?,
                            metadata: serde_json::from_str(&r.get::<_, String>(4)?)
                                .unwrap_or_default(),
                            similarity_score: None,
                        })
                    },
                )
                .optional()?;
            ensure!(
                current
                    .map(|v| serde_json::to_value(v))
                    .transpose()?
                    .as_ref()
                    == Some(&serde_json::to_value(source)?),
                "Memory changed during generation; retry"
            );
            let old: Option<String> = tx
                .query_row(
                    "SELECT snapshot FROM dream_sources WHERE id=?1",
                    [&source.id],
                    |r| r.get(0),
                )
                .optional()?;
            ensure!(
                old.as_deref()
                    .map(serde_json::from_str::<Value>)
                    .transpose()?
                    .as_ref()
                    != Some(&serde_json::to_value(source)?),
                "Memory was already consolidated; retry"
            );
        }
        let id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now().to_rfc3339();
        let metadata = json!({"type":"dream", "source_ids":serde_json::to_string(&sources.iter().map(|s| &s.id).collect::<Vec<_>>())?}).to_string();
        // Exact duplicate summary reuses its existing memory without overwriting metadata.
        tx.execute("INSERT INTO memories(id,content,importance,created_at,metadata) VALUES(?1,?2,0.9,?3,?4) ON CONFLICT(content) DO NOTHING", params![id,summary,now,metadata])?;
        let summary_id: String =
            tx.query_row("SELECT id FROM memories WHERE content=?1", [summary], |r| {
                r.get(0)
            })?;
        for source in sources {
            tx.execute("INSERT INTO dream_sources(id,snapshot) VALUES(?1,?2) ON CONFLICT(id) DO UPDATE SET snapshot=excluded.snapshot", params![source.id, serde_json::to_string(source)?])?;
        }
        tx.execute(
            "INSERT INTO dream_runs(id,created_at,source_count,summary_id) VALUES(?1,?2,?3,?4)",
            params![id, now, sources.len(), summary_id],
        )?;
        tx.commit()?;
        Ok(summary_id)
    }
}
