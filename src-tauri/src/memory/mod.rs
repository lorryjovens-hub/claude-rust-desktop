//! MemEx - Infinite Memory Engine for Claude Desktop (Tauri)
//!
//! Provides: semantic search, auto-ingest, context compression
//! Backend: Python MemEx HTTP API (http://127.0.0.1:8765)

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Data Types
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryItem {
    pub id: String,
    pub content: String,
    pub importance: f64,
    pub created_at: String,
    pub metadata: Option<HashMap<String, String>>,
    pub similarity_score: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemorySearchRequest {
    pub query: String,
    pub top_k: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemorySearchResponse {
    pub results: Vec<MemoryItem>,
    pub total_indexed: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryIngestRequest {
    pub content: String,
    pub importance: Option<f64>,
    pub metadata: Option<HashMap<String, String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryStats {
    pub total_memories: usize,
    pub total_tokens_approx: usize,
    pub backend: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryConfig {
    pub enabled: bool,
    pub backend_url: String,
    pub auto_ingest: bool,
    pub auto_search: bool,
    pub search_top_k: usize,
    pub compression_threshold_tokens: usize,
    pub min_importance_threshold: f64,
}

impl Default for MemoryConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            backend_url: "http://127.0.0.1:8765".to_string(),
            auto_ingest: true,
            auto_search: true,
            search_top_k: 5,
            compression_threshold_tokens: 180_000,
            min_importance_threshold: 0.3,
        }
    }
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// MemEx HTTP Client
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

#[derive(Clone)]
pub struct MemExClient {
    client: reqwest::Client,
    base_url: String,
    config: Arc<RwLock<MemoryConfig>>,
}

impl MemExClient {
    pub fn new(base_url: Option<String>) -> Self {
        let config = MemoryConfig {
            backend_url: base_url.unwrap_or_else(|| "http://127.0.0.1:8765".to_string()),
            ..Default::default()
        };
        Self {
            client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(10))
                .build()
                .unwrap_or_default(),
            base_url: config.backend_url.clone(),
            config: Arc::new(RwLock::new(config)),
        }
    }

    pub async fn get_config(&self) -> MemoryConfig {
        self.config.read().await.clone()
    }

    pub async fn update_config(&self, config: MemoryConfig) {
        *self.config.write().await = config;
    }

    pub async fn health_check(&self) -> Result<bool> {
        match self.client.get(format!("{}/health", self.base_url)).send().await {
            Ok(resp) => Ok(resp.status().is_success()),
            Err(_) => Ok(false),
        }
    }

    pub async fn search(&self, query: &str, top_k: Option<usize>) -> Result<Vec<MemoryItem>> {
        let config = self.config.read().await;
        if !config.enabled {
            return Ok(vec![]);
        }
        let req = MemorySearchRequest {
            query: query.to_string(),
            top_k: Some(top_k.unwrap_or(config.search_top_k)),
        };
        match self.client.post(format!("{}/search", self.base_url)).json(&req).send().await {
            Ok(resp) if resp.status().is_success() => {
                let result: MemorySearchResponse = resp.json().await?;
                debug!("[MemEx] Search: {} results", result.results.len());
                Ok(result.results)
            }
            Ok(resp) => { warn!("[MemEx] Search failed: {}", resp.status()); Ok(vec![]) }
            Err(e) => { warn!("[MemEx] Backend unreachable: {}", e); Ok(vec![]) }
        }
    }

    pub async fn ingest(&self, content: &str, importance: Option<f64>, metadata: Option<HashMap<String, String>>) -> Result<()> {
        let config = self.config.read().await;
        if !config.enabled { return Ok(()); }
        let importance = importance.unwrap_or_else(|| Self::estimate_importance(content));
        if importance < config.min_importance_threshold {
            debug!("[MemEx] Skip low-importance ({:.2})", importance);
            return Ok(());
        }
        let req = MemoryIngestRequest { content: content.to_string(), importance: Some(importance), metadata };
        match self.client.post(format!("{}/ingest", self.base_url)).json(&req).send().await {
            Ok(resp) if resp.status().is_success() => {
                debug!("[MemEx] Ingested (importance: {:.2})", importance);
            }
            Ok(resp) => warn!("[MemEx] Ingest failed: {}", resp.status()),
            Err(e) => warn!("[MemEx] Backend unreachable: {}", e),
        }
        Ok(())
    }

    pub async fn stats(&self) -> Result<MemoryStats> {
        match self.client.get(format!("{}/stats", self.base_url)).send().await {
            Ok(resp) if resp.status().is_success() => Ok(resp.json().await?),
            Ok(resp) => Err(anyhow!("Backend: {}", resp.status())),
            Err(e) => Err(anyhow!("Unreachable: {}", e)),
        }
    }

    pub async fn clear(&self) -> Result<()> {
        self.client.post(format!("{}/clear", self.base_url)).send().await?;
        Ok(())
    }

    fn estimate_importance(content: &str) -> f64 {
        let content_lower = content.to_lowercase();
        let mut score = 0.3;
        let high_signals = ["important", "critical", "key", "remember", "password", "token", "api key", "secret", "config", "必须记住", "重要", "关键", "密码"];
        for s in &high_signals { if content_lower.contains(s) { score += 0.15; } }
        let medium_signals = ["architecture", "design", "decision", "pattern", "workflow", "架构", "设计", "决策", "模式"];
        for s in &medium_signals { if content_lower.contains(s) { score += 0.08; } }
        if content.len() > 200 { score += 0.05; }
        if content.len() > 500 { score += 0.05; }
        score.min(1.0)
    }
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Context Manager
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

#[derive(Clone)]
pub struct ContextManager {
    memex: MemExClient,
}

impl ContextManager {
    pub fn new(memex: MemExClient) -> Self { Self { memex } }
    pub fn memex(&self) -> &MemExClient { &self.memex }

    /// Called BEFORE API call: inject relevant memories into context
    pub async fn before_api_call(&self, conversation_id: &str, user_message: &str) -> Option<String> {
        let config = self.memex.get_config().await;
        if !config.enabled || !config.auto_search { return None; }
        match self.memex.search(user_message, Some(config.search_top_k)).await {
            Ok(results) if !results.is_empty() => {
                let memory_context: String = results.iter().enumerate()
                    .map(|(i, m)| format!("[Memory {}] (importance: {:.2}) {}\n", i + 1, m.importance, m.content))
                    .collect();
                let injected = format!(
                    "<relevant_memories>\nThe following information from previous conversations may be relevant:\n\n{}</relevant_memories>\n\nUse this context if relevant.",
                    memory_context
                );
                info!("[MemEx] Injected {} memories into conv {}", results.len(), conversation_id);
                Some(injected)
            }
            _ => None,
        }
    }

    /// Called AFTER response: store conversation turn
    pub async fn after_response(&self, conversation_id: &str, user_message: &str, assistant_message: &str) {
        let config = self.memex.get_config().await;
        if !config.enabled || !config.auto_ingest { return; }
        let trunc = |s: &str| if s.len() > 2000 { format!("{}...", &s[..2000]) } else { s.to_string() };
        let memory = format!("Conv {}:\nUser: {}\nAssistant: {}", conversation_id, trunc(user_message), trunc(assistant_message));
        let mut meta = HashMap::new();
        meta.insert("type".to_string(), "conversation".to_string());
        meta.insert("conversation_id".to_string(), conversation_id.to_string());
        let _ = self.memex.ingest(&memory, None, Some(meta)).await;
    }

    pub fn needs_compression(&self, message_count: usize, total_chars: usize) -> bool {
        let estimated_tokens = total_chars / 4;
        message_count > 50 || estimated_tokens > 180_000
    }

    pub async fn compress_context(&self, conversation_id: &str, old_messages: &[String]) -> Result<String> {
        let summary = Self::simple_summarize(old_messages);
        let mut meta = HashMap::new();
        meta.insert("type".to_string(), "context_compression".to_string());
        meta.insert("conversation_id".to_string(), conversation_id.to_string());
        meta.insert("compressed_count".to_string(), old_messages.len().to_string());
        let _ = self.memex.ingest(&summary, Some(0.9), Some(meta)).await;
        info!("[MemEx] Compressed {} messages for conv {}", old_messages.len(), conversation_id);
        Ok(summary)
    }

    fn simple_summarize(messages: &[String]) -> String {
        let combined = messages.join("\n");
        let key_signals = ["important", "key", "decision", "architecture", "design", "pattern", "bug", "fix", "error", "solution", "conclusion", "重要", "关键", "决定", "架构", "设计", "错误", "修复", "解决方案", "结论"];
        let mut key_sentences: Vec<&str> = Vec::new();
        for line in combined.lines() {
            let line_lower = line.to_lowercase();
            for signal in &key_signals {
                if line_lower.contains(signal) && !key_sentences.contains(&line) {
                    key_sentences.push(line);
                    break;
                }
            }
        }
        if key_sentences.is_empty() {
            let lines: Vec<&str> = combined.lines().collect();
            let total = lines.len();
            let take = 10.min(total);
            let mut summary = String::from("[Context Summary]\n");
            for line in &lines[..take.min(total / 2)] { summary.push_str(line); summary.push('\n'); }
            if total > take * 2 { summary.push_str("...\n"); }
            for line in &lines[total.saturating_sub(take)..] { summary.push_str(line); summary.push('\n'); }
            summary
        } else {
            format!("[Context Summary - {} key points]\n{}", key_sentences.len(), key_sentences.join("\n"))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_importance() {
        let low = MemExClient::estimate_importance("hello world");
        let high = MemExClient::estimate_importance("IMPORTANT: The API key is sk-abc123. Remember this!");
        assert!(high > low);
    }

    #[test]
    fn test_summarize() {
        let msgs = vec!["The architecture uses microservices".to_string(), "Bug: memory leak in pool".to_string(), "Random chat".to_string()];
        let s = ContextManager::simple_summarize(&msgs);
        assert!(s.contains("architecture") || s.contains("Bug"));
    }
}
