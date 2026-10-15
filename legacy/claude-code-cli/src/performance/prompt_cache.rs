//! Prompt Cache - Anthropic prompt caching (system_and_3 strategy)
//!
//! Reduces input token costs by ~75% on multi-turn conversations by caching
//! the conversation prefix. Uses 4 cache_control breakpoints:
//! 1. System prompt (stable across all turns)
//! 2-4. Last 3 non-system messages (rolling window)

use crate::api::ChatMessage;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheConfig {
    pub enabled: bool,
    pub cache_ttl: String,
    pub max_breakpoints: usize,
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            cache_ttl: "5m".to_string(),
            max_breakpoints: 4,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheMarker {
    #[serde(rename = "type")]
    pub marker_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ttl: Option<String>,
}

impl CacheMarker {
    pub fn ephemeral() -> Self {
        Self {
            marker_type: "ephemeral".to_string(),
            ttl: None,
        }
    }

    pub fn with_ttl(ttl: &str) -> Self {
        Self {
            marker_type: "ephemeral".to_string(),
            ttl: Some(ttl.to_string()),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedContent {
    #[serde(rename = "type")]
    pub content_type: String,
    pub text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<CacheMarker>,
}

pub struct PromptCache {
    config: CacheConfig,
}

impl PromptCache {
    pub fn new(config: CacheConfig) -> Self {
        Self { config }
    }

    pub fn apply_cache_control(&self, messages: &mut Vec<ChatMessage>) -> usize {
        if !self.config.enabled || messages.is_empty() {
            return 0;
        }

        let marker = if self.config.cache_ttl == "1h" {
            CacheMarker::with_ttl("1h")
        } else {
            CacheMarker::ephemeral()
        };

        let mut breakpoints_used = 0;

        if messages.first().map(|m| m.role == "system").unwrap_or(false) {
            if let Some(ref content) = messages[0].content {
                let cached = CachedContent {
                    content_type: "text".to_string(),
                    text: content.clone(),
                    cache_control: Some(marker.clone()),
                };
                if let Ok(json) = serde_json::to_string(&cached) {
                    messages[0].content = Some(json);
                }
            }
            breakpoints_used += 1;
        }

        let remaining = self.config.max_breakpoints.saturating_sub(breakpoints_used);
        if remaining == 0 {
            return breakpoints_used;
        }

        let non_sys_indices: Vec<usize> = messages
            .iter()
            .enumerate()
            .filter(|(_, m)| m.role != "system")
            .map(|(i, _)| i)
            .collect();

        for &idx in non_sys_indices.iter().rev().take(remaining) {
            if let Some(ref content) = messages[idx].content {
                let cached = CachedContent {
                    content_type: "text".to_string(),
                    text: content.clone(),
                    cache_control: Some(marker.clone()),
                };
                if let Ok(json) = serde_json::to_string(&cached) {
                    messages[idx].content = Some(json);
                    breakpoints_used += 1;
                }
            }
        }

        breakpoints_used
    }

    pub fn is_cacheable(&self, message: &ChatMessage) -> bool {
        if !self.config.enabled {
            return false;
        }

        if message.role == "system" {
            return true;
        }

        if message.role == "user" && message.content.is_some() {
            let content = message.content.as_ref().unwrap();
            return content.len() < 10000;
        }

        false
    }
}

pub fn apply_anthropic_cache_control(
    messages: &mut Vec<ChatMessage>,
    cache_ttl: &str,
) -> usize {
    let cache = PromptCache::new(CacheConfig {
        enabled: true,
        cache_ttl: cache_ttl.to_string(),
        max_breakpoints: 4,
    });
    cache.apply_cache_control(messages)
}

impl Default for PromptCache {
    fn default() -> Self {
        Self::new(CacheConfig::default())
    }
}
