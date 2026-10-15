//! Tool Result Persistence - Preserve large outputs instead of truncating
//!
//! Defense against context-window overflow operates at three levels:
//! 1. Per-tool output cap (inside each tool)
//! 2. Per-result persistence (maybe_persist_tool_result)
//! 3. Per-turn aggregate budget (enforce_turn_budget)

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;

const PERSISTED_OUTPUT_TAG: &str = "<persisted-output>";
const PERSISTED_OUTPUT_CLOSING_TAG: &str = "</persisted-output>";
const DEFAULT_PREVIEW_SIZE: usize = 500;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersistedOutput {
    pub tool_use_id: String,
    pub tool_name: String,
    pub file_path: String,
    pub original_size: usize,
    pub preview: String,
}

pub struct ToolPersistence {
    storage_dir: PathBuf,
    max_preview_chars: usize,
    persisted: Arc<RwLock<HashMap<String, PersistedOutput>>>,
}

impl ToolPersistence {
    pub fn new(storage_dir: PathBuf) -> Self {
        std::fs::create_dir_all(&storage_dir).ok();
        Self {
            storage_dir,
            max_preview_chars: DEFAULT_PREVIEW_SIZE,
            persisted: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub fn set_preview_size(&mut self, size: usize) {
        self.max_preview_chars = size;
    }

    pub fn generate_preview(content: &str) -> (String, bool) {
        if content.len() <= DEFAULT_PREVIEW_SIZE {
            return (content.to_string(), false);
        }

        let truncated = &content[..DEFAULT_PREVIEW_SIZE];
        let last_nl = truncated.rfind('\n').unwrap_or(DEFAULT_PREVIEW_SIZE / 2);
        let preview = truncated[..last_nl].to_string();

        (preview, true)
    }

    pub async fn persist_if_needed(
        &self,
        content: &str,
        tool_name: &str,
        tool_use_id: &str,
        threshold: usize,
    ) -> String {
        if content.len() <= threshold {
            return content.to_string();
        }

        let remote_path = self.storage_dir.join(format!("{}.txt", tool_use_id));
        let preview = Self::generate_preview(content);
        let has_more = preview.1;

        if let Err(e) = tokio::fs::write(&remote_path, content).await {
            eprintln!("Failed to persist tool result: {}", e);
            return format!(
                "{}\n\n[Truncated: tool response was {} chars. Full output could not be saved.]",
                preview.0,
                content.len()
            );
        }

        let size_kb = content.len() as f64 / 1024.0;
        let size_str = if size_kb >= 1024.0 {
            format!("{:.1} MB", size_kb / 1024.0)
        } else {
            format!("{:.1} KB", size_kb)
        };

        let persisted = PersistedOutput {
            tool_use_id: tool_use_id.to_string(),
            tool_name: tool_name.to_string(),
            file_path: remote_path.to_string_lossy().to_string(),
            original_size: content.len(),
            preview: preview.0.clone(),
        };

        let mut persisted_map = self.persisted.write().await;
        persisted_map.insert(tool_use_id.to_string(), persisted);

        format!(
            "{}\n{}\nThis tool result was too large ({} characters, {}).\nFull output saved to: {}\nUse the read_file tool with offset and limit to access specific sections of this output.\n\nPreview (first {} chars):\n{}{}\n{}",
            PERSISTED_OUTPUT_TAG,
            format!("This tool result was too large ({} characters, {}).", content.len(), size_str),
            content.len(),
            size_str,
            remote_path.to_string_lossy(),
            preview.0.len(),
            preview.0,
            if has_more { "\n..." } else { "" },
            PERSISTED_OUTPUT_CLOSING_TAG
        )
    }

    pub async fn get_persisted(&self, tool_use_id: &str) -> Option<PersistedOutput> {
        let persisted = self.persisted.read().await;
        persisted.get(tool_use_id).cloned()
    }

    pub async fn read_persisted_content(&self, tool_use_id: &str) -> Option<String> {
        let persisted = self.get_persisted(tool_use_id).await?;
        tokio::fs::read_to_string(&persisted.file_path).await.ok()
    }

    pub async fn enforce_turn_budget(
        &self,
        tool_messages: &mut Vec<String>,
        max_chars: usize,
    ) {
        let total_size: usize = tool_messages.iter().map(|m| m.len()).sum();

        if total_size <= max_chars {
            return;
        }

        let mut candidates: Vec<(usize, usize)> = tool_messages
            .iter()
            .enumerate()
            .filter(|(_, m)| !m.contains(PERSISTED_OUTPUT_TAG))
            .map(|(i, m)| (i, m.len()))
            .collect();

        candidates.sort_by(|a, b| b.1.cmp(&a.1));

        let mut current_size = total_size;
        for (idx, size) in candidates {
            if current_size <= max_chars {
                break;
            }

            let content = &tool_messages[idx];
            let persisted = self.persist_if_needed(
                content,
                "unknown",
                &format!("turn_budget_{}", idx),
                1000,
            ).await;

            tool_messages[idx] = persisted;
            current_size -= size;
        }
    }
}

impl Default for ToolPersistence {
    fn default() -> Self {
        let storage_dir = std::env::temp_dir().join("claude-code-results");
        Self::new(storage_dir)
    }
}
