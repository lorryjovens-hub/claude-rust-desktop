use crate::native_engine::provider_manager::ResolvedProvider;
use crate::tools::ToolDefinition;
use anyhow::Result;
use futures::Stream;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::pin::Pin;
use tokio_stream::StreamExt;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnthropicMessage {
    pub role: String,
    pub content: AnthropicContent,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum AnthropicContent {
    Text(String),
    Blocks(Vec<ContentBlock>),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ContentBlock {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "image")]
    Image {
        source: ImageSource,
    },
    #[serde(rename = "tool_use")]
    ToolUse {
        id: String,
        name: String,
        input: Value,
    },
    #[serde(rename = "tool_result")]
    ToolResult {
        tool_use_id: String,
        content: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        is_error: Option<bool>,
    },
    #[serde(rename = "thinking")]
    Thinking {
        thinking: String,
        signature: Option<String>,
    },
    #[serde(rename = "redacted_thinking")]
    RedactedThinking {
        data: Option<String>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageSource {
    #[serde(rename = "type")]
    pub source_type: String,
    pub media_type: String,
    pub data: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AnthropicResponse {
    pub id: String,
    #[serde(rename = "type")]
    pub response_type: String,
    pub role: String,
    pub content: Vec<ContentBlock>,
    pub model: String,
    pub stop_reason: Option<String>,
    pub stop_sequence: Option<String>,
    pub usage: Option<UsageInfo>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct UsageInfo {
    pub input_tokens: u32,
    pub output_tokens: u32,
    #[serde(default)]
    pub cache_creation_input_tokens: Option<u32>,
    #[serde(default)]
    pub cache_read_input_tokens: Option<u32>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct StreamEvent {
    #[serde(rename = "type")]
    pub event_type: String,
    pub index: Option<usize>,
    pub delta: Option<StreamDelta>,
    pub content_block: Option<ContentBlock>,
    pub message: Option<StreamMessage>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct StreamDelta {
    #[serde(rename = "type")]
    pub delta_type: String,
    pub text: Option<String>,
    pub partial_json: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct StreamMessage {
    pub id: Option<String>,
    #[serde(rename = "type")]
    pub message_type: Option<String>,
    pub role: Option<String>,
    pub content: Option<Vec<ContentBlock>>,
    pub model: Option<String>,
    pub stop_reason: Option<String>,
    pub usage: Option<UsageInfo>,
}


use crate::performance::{retry_with_backoff, checked_response};

pub struct AnthropicClient {
    client: Client,
}

impl AnthropicClient {
    pub fn new() -> Self {
        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(300))
            .build()
            .unwrap_or_else(|_| Client::new());
        Self { client }
    }

    pub async fn send_message(
        &self,
        provider: &ResolvedProvider,
        messages: Vec<AnthropicMessage>,
        system_prompt: Option<&str>,
        tools: Vec<ToolDefinition>,
        max_tokens: u32,
    ) -> Result<AnthropicResponse> {
        let base_url = provider.provider.base_url.trim_end_matches('/');
        let url = if base_url.contains("/v1") {
            format!("{}/messages", base_url)
        } else {
            format!("{}/v1/messages", base_url)
        };

        let mut body = json!({
            "model": provider.model.id,
            "max_tokens": max_tokens,
            "messages": messages,
        });

        if let Some(system) = system_prompt {
            body["system"] = json!(system);
        }

        if !tools.is_empty() {
            let tool_defs: Vec<Value> = tools.iter().map(|t| {
                json!({
                    "name": t.name,
                    "description": t.description,
                    "input_schema": t.input_schema,
                })
            }).collect();
            body["tools"] = json!(tool_defs);
        }

        crate::performance::apply_prompt_cache(&mut body);
        let data: AnthropicResponse = retry_with_backoff(|| async {
            let response = self.client
                .post(&url)
                .header("x-api-key", &provider.provider.api_key)
                .header("anthropic-version", "2023-06-01")
                .header("content-type", "application/json")
                .json(&body)
                .send()
                .await?;

            let response = checked_response(response).await?;

            response.json().await.map_err(|e| anyhow::anyhow!("{:#}", e))
        }).await?;

        Ok(data)
    }

    pub async fn send_message_stream(
        &self,
        provider: &ResolvedProvider,
        messages: Vec<AnthropicMessage>,
        system_prompt: Option<&str>,
        tools: Vec<ToolDefinition>,
        max_tokens: u32,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<String>> + Send>>> {
        let base_url = provider.provider.base_url.trim_end_matches('/');
        let url = if base_url.contains("/v1") {
            format!("{}/messages", base_url)
        } else {
            format!("{}/v1/messages", base_url)
        };

        let mut body = json!({
            "model": provider.model.id,
            "max_tokens": max_tokens,
            "messages": messages,
            "stream": true,
        });

        if let Some(system) = system_prompt {
            body["system"] = json!(system);
        }

        if !tools.is_empty() {
            let tool_defs: Vec<Value> = tools.iter().map(|t| {
                json!({
                    "name": t.name,
                    "description": t.description,
                    "input_schema": t.input_schema,
                })
            }).collect();
            body["tools"] = json!(tool_defs);
        }

        crate::performance::apply_prompt_cache(&mut body);
        let response = retry_with_backoff(|| async {
            let resp = self.client
                .post(&url)
                .header("x-api-key", &provider.provider.api_key)
                .header("anthropic-version", "2023-06-01")
                .header("content-type", "application/json")
                .json(&body)
                .send()
                .await?;

            let resp = checked_response(resp).await?;
            Ok(resp)
        }).await?;

        let stream = response.bytes_stream();
        let event_stream = stream
            .map(|chunk| {
                match chunk {
                    Ok(bytes) => {
                        let text = String::from_utf8_lossy(&bytes);
                        Ok(text.to_string())
                    }
                    Err(e) => Err(anyhow::anyhow!("Stream error: {}", e)),
                }
            });

        Ok(Box::pin(event_stream))
    }

    pub async fn send_message_stream_with_thinking(
        &self,
        provider: &ResolvedProvider,
        messages: Vec<AnthropicMessage>,
        system_prompt: Option<&str>,
        tools: Vec<ToolDefinition>,
        max_tokens: u32,
        enable_thinking: bool,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<String>> + Send>>> {
        let base_url = provider.provider.base_url.trim_end_matches('/');
        let url = if base_url.contains("/v1") {
            format!("{}/messages", base_url)
        } else {
            format!("{}/v1/messages", base_url)
        };

        let mut body = json!({
            "model": provider.model.id,
            "max_tokens": max_tokens,
            "messages": messages,
            "stream": true,
        });

        if let Some(system) = system_prompt {
            body["system"] = json!(system);
        }

        if !tools.is_empty() {
            let tool_defs: Vec<Value> = tools.iter().map(|t| {
                json!({
                    "name": t.name,
                    "description": t.description,
                    "input_schema": t.input_schema,
                })
            }).collect();
            body["tools"] = json!(tool_defs);
        }

        if let Some(effort) = provider.model.effort.as_deref() {
            apply_effort(&mut body, effort, &provider.model.id, max_tokens)?;
        } else if enable_thinking {
            body["thinking"] = json!({"type":"enabled", "budget_tokens": 10000u32.min(max_tokens.saturating_sub(1))});
        }

        crate::performance::apply_prompt_cache(&mut body);
        let response = retry_with_backoff(|| async {
            let resp = self.client
                .post(&url)
                .header("x-api-key", &provider.provider.api_key)
                .header("anthropic-version", "2023-06-01")
                .header("content-type", "application/json")
                .json(&body)
                .send()
                .await?;

            let resp = checked_response(resp).await?;
            Ok(resp)
        }).await?;

        let stream = response.bytes_stream();
        let event_stream = stream
            .map(|chunk| {
                match chunk {
                    Ok(bytes) => {
                        let text = String::from_utf8_lossy(&bytes);
                        Ok(text.to_string())
                    }
                    Err(e) => Err(anyhow::anyhow!("Stream error: {}", e)),
                }
            });

        Ok(Box::pin(event_stream))
    }
}

impl Default for AnthropicClient {
    fn default() -> Self {
        Self::new()
    }
}

pub fn apply_effort(body: &mut Value, effort: &str, model: &str, max_tokens: u32) -> Result<()> {
    if effort == "off" { body["thinking"] = json!({"type":"disabled"}); return Ok(()); }
    if model.contains("opus-4-6") || model.contains("opus-4-7") || model.contains("sonnet-4-6") {
        body["thinking"] = json!({"type":"adaptive"});
        body["output_config"] = json!({"effort": if effort == "max" && model.contains("sonnet") { "high" } else { effort }});
    } else {
        if max_tokens <= 1024 { anyhow::bail!("Thinking requires max output tokens greater than 1024"); }
        let budget = match effort { "low" => 2048, "high" => 10000, "max" => 32000, _ => anyhow::bail!("Invalid effort") };
        body["thinking"] = json!({"type":"enabled", "budget_tokens": budget.min(max_tokens - 1)});
    }
    Ok(())
}
