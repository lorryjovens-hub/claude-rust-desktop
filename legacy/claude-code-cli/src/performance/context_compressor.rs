//! Context Compressor - Automatic context window compression for long conversations
//!
//! Reduces token usage by summarizing middle turns while protecting head and tail context.
//!
//! Algorithm:
//! 1. Prune old tool results (cheap pre-pass)
//! 2. Protect head messages (system prompt + first exchange)
//! 3. Protect tail messages by token budget
//! 4. Summarize middle turns with LLM
//! 5. Iteratively update summary on subsequent compactions

use crate::api::{ChatMessage, Usage};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::RwLock;

const SUMMARY_PREFIX: &str = "[CONTEXT COMPACTION] Earlier turns in this conversation were compacted to save context space. The summary below describes work that was already completed, and the current session state may still reflect that work. Use the summary and the current state to continue from where things left off, and avoid repeating work:";
const PRUNED_TOOL_PLACEHOLDER: &str = "[Old tool output cleared to save context space]";
const CHARS_PER_TOKEN: f64 = 4.0;
const SUMMARY_RATIO: f64 = 0.20;
const MIN_SUMMARY_TOKENS: usize = 2000;
const SUMMARY_TOKENS_CEILING: usize = 12_000;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompressorConfig {
    pub threshold_percent: f64,
    pub protect_first_n: usize,
    pub protect_last_n: usize,
    pub summary_target_ratio: f64,
    pub summary_model: Option<String>,
}

impl Default for CompressorConfig {
    fn default() -> Self {
        Self {
            threshold_percent: 0.50,
            protect_first_n: 3,
            protect_last_n: 20,
            summary_target_ratio: 0.20,
            summary_model: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompressionResult {
    pub original_tokens: usize,
    pub compressed_tokens: usize,
    pub savings_ratio: f64,
    pub messages_preserved: usize,
    pub messages_summarized: usize,
    pub summary: Option<String>,
}

pub struct ContextCompressor {
    config: CompressorConfig,
    context_length: usize,
    threshold_tokens: usize,
    tail_token_budget: usize,
    max_summary_tokens: usize,
    compression_count: usize,
    last_prompt_tokens: usize,
    last_completion_tokens: usize,
    previous_summary: Option<String>,
    messages: Arc<RwLock<Vec<ChatMessage>>>,
}

impl ContextCompressor {
    pub fn new(model: &str, config: CompressorConfig) -> Self {
        let context_length = Self::get_model_context_length(model);
        let threshold_tokens = (context_length as f64 * config.threshold_percent) as usize;
        let target_tokens = (threshold_tokens as f64 * config.summary_target_ratio) as usize;
        let tail_token_budget = target_tokens;
        let max_summary_tokens = ((context_length as f64 * 0.05) as usize).min(SUMMARY_TOKENS_CEILING);

        Self {
            config,
            context_length,
            threshold_tokens,
            tail_token_budget,
            max_summary_tokens,
            compression_count: 0,
            last_prompt_tokens: 0,
            last_completion_tokens: 0,
            previous_summary: None,
            messages: Arc::new(RwLock::new(Vec::new())),
        }
    }

    fn get_model_context_length(model: &str) -> usize {
        let model_lower = model.to_lowercase();
        if model_lower.contains("opus") {
            200_000
        } else if model_lower.contains("sonnet") {
            200_000
        } else if model_lower.contains("haiku") {
            200_000
        } else if model_lower.contains("claude-3-5") {
            200_000
        } else if model_lower.contains("gpt-4o") {
            128_000
        } else if model_lower.contains("gpt-4-turbo") {
            128_000
        } else if model_lower.contains("gpt-4") {
            128_000
        } else if model_lower.contains("gpt-3.5-turbo") {
            16_385
        } else if model_lower.contains("deepseek-chat") {
            64_000
        } else if model_lower.contains("deepseek-coder") {
            128_000
        } else {
            128_000
        }
    }

    pub fn update_from_response(&mut self, usage: &Usage) {
        self.last_prompt_tokens = usage.prompt_tokens;
        self.last_completion_tokens = usage.completion_tokens;
    }

    pub fn should_compress(&self, prompt_tokens: Option<usize>) -> bool {
        let tokens = prompt_tokens.unwrap_or(self.last_prompt_tokens);
        tokens >= self.threshold_tokens
    }

    pub async fn get_messages(&self) -> Vec<ChatMessage> {
        self.messages.read().await.clone()
    }

    pub async fn set_messages(&self, messages: Vec<ChatMessage>) {
        let mut msgs = self.messages.write().await;
        *msgs = messages;
    }

    pub fn estimate_messages_tokens(messages: &[ChatMessage]) -> usize {
        let mut total = 0;
        for msg in messages {
            let role_tokens = 4;
            let content_tokens = msg.content.as_ref()
                .map(|c| (c.len() as f64 / CHARS_PER_TOKEN) as usize)
                .unwrap_or(0);
            total += role_tokens + content_tokens + 3;
        }
        total
    }

    pub fn prune_old_tool_results(
        &self,
        messages: &mut Vec<ChatMessage>,
        protect_tail_count: usize,
    ) -> usize {
        let mut pruned = 0;
        let prune_boundary = messages.len().saturating_sub(protect_tail_count);

        for i in 0..prune_boundary {
            if messages[i].role == "tool" {
                if let Some(content) = &messages[i].content {
                    if content.len() > 200 && content != PRUNED_TOOL_PLACEHOLDER {
                        messages[i].content = Some(PRUNED_TOOL_PLACEHOLDER.to_string());
                        pruned += 1;
                    }
                }
            }
        }
        pruned
    }

    pub fn protect_head_and_tail(
        &self,
        messages: &[ChatMessage],
    ) -> (Vec<ChatMessage>, Vec<ChatMessage>, Vec<ChatMessage>) {
        let protected_head: Vec<ChatMessage> = messages
            .iter()
            .take(self.config.protect_first_n)
            .cloned()
            .collect();

        let protected_tail: Vec<ChatMessage> = messages
            .iter()
            .rev()
            .take(self.config.protect_last_n)
            .cloned()
            .collect();

        let middle_count = messages.len().saturating_sub(self.config.protect_first_n + self.config.protect_last_n);
        let middle: Vec<ChatMessage> = messages
            .iter()
            .skip(self.config.protect_first_n)
            .take(middle_count)
            .cloned()
            .collect();

        (protected_head, middle, protected_tail)
    }

    pub async fn compress(&mut self) -> CompressionResult {
        let mut messages = self.messages.write().await;
        let original_tokens = Self::estimate_messages_tokens(&messages);
        let original_count = messages.len();

        let pruned_count = self.prune_old_tool_results(&mut messages, self.config.protect_last_n);

        let (head, middle, tail) = self.protect_head_and_tail(&messages);
        let middle_tokens = Self::estimate_messages_tokens(&middle);

        let summary_budget = (middle_tokens as f64 * SUMMARY_RATIO)
            .clamp(MIN_SUMMARY_TOKENS as f64, self.max_summary_tokens as f64) as usize;

        let summary = if !middle.is_empty() {
            let mut summary_text = String::new();
            summary_text.push_str(SUMMARY_PREFIX);
            summary_text.push('\n');

            if let Some(prev) = &self.previous_summary {
                summary_text.push_str("\n[Previous Summary]:\n");
                summary_text.push_str(prev);
                summary_text.push('\n');
            }

            summary_text.push_str("\n[Conversation History]:\n");
            for msg in &middle {
                let role = &msg.role;
                let content = msg.content.as_deref().unwrap_or("");
                let preview = if content.len() > 500 {
                    format!("{}...", &content[..500])
                } else {
                    content.to_string()
                };
                summary_text.push_str(&format!("{}: {}\n", role, preview));
            }

            self.previous_summary = Some(summary_text.clone());
            Some(summary_text)
        } else {
            None
        };

        let mut new_messages = head.clone();
        if let Some(ref summ) = summary {
            new_messages.push(ChatMessage {
                role: "system".to_string(),
                content: Some(summ.clone()),
                tool_calls: None,
                tool_call_id: None,
            });
        }
        let tail_len = tail.len();
        new_messages.extend(tail);

        let compressed_tokens = Self::estimate_messages_tokens(&new_messages);
        let savings = if original_tokens > 0 {
            1.0 - (compressed_tokens as f64 / original_tokens as f64)
        } else {
            0.0
        };

        *messages = new_messages;
        self.compression_count += 1;

        CompressionResult {
            original_tokens,
            compressed_tokens,
            savings_ratio: savings,
            messages_preserved: head.len() + tail_len,
            messages_summarized: middle.len(),
            summary,
        }
    }

    pub fn compression_count(&self) -> usize {
        self.compression_count
    }

    pub fn threshold_tokens(&self) -> usize {
        self.threshold_tokens
    }

    pub fn context_length(&self) -> usize {
        self.context_length
    }
}

impl Default for ContextCompressor {
    fn default() -> Self {
        Self::new("claude-sonnet-4", CompressorConfig::default())
    }
}
