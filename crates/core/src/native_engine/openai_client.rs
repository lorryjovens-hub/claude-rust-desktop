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
pub struct OpenAIMessage {
    pub role: String,
    pub content: OpenAIContent,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<OpenAIToolCall>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning_content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider_content: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum OpenAIContent {
    Text(String),
    Multi(Vec<OpenAIContentPart>),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum OpenAIContentPart {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "image_url")]
    Image { image_url: ImageUrl },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageUrl {
    pub url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAIToolCall {
    pub id: String,
    #[serde(rename = "type")]
    pub call_type: String,
    pub function: FunctionCall,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionCall {
    pub name: String,
    pub arguments: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct OpenAIResponse {
    pub id: String,
    pub object: String,
    pub created: u64,
    pub model: String,
    pub choices: Vec<OpenAIChoice>,
    pub usage: Option<OpenAIUsage>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct OpenAIChoice {
    pub index: usize,
    pub message: OpenAIMessage,
    pub finish_reason: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct OpenAIUsage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct OpenAIStreamChunk {
    pub id: String,
    pub object: String,
    pub created: u64,
    pub model: String,
    pub choices: Vec<OpenAIStreamChoice>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct OpenAIStreamChoice {
    pub index: usize,
    pub delta: OpenAIDelta,
    pub finish_reason: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct OpenAIDelta {
    pub role: Option<String>,
    pub content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<OpenAIToolCallDelta>>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct OpenAIToolCallDelta {
    pub index: usize,
    pub id: Option<String>,
    #[serde(rename = "type")]
    pub call_type: Option<String>,
    pub function: Option<FunctionCallDelta>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct FunctionCallDelta {
    pub name: Option<String>,
    pub arguments: Option<String>,
}


use crate::performance::{retry_with_backoff, checked_response};

pub struct OpenAIClient {
    client: Client,
}

impl OpenAIClient {
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
        messages: Vec<OpenAIMessage>,
        system_prompt: Option<&str>,
        tools: Vec<ToolDefinition>,
        max_tokens: u32,
    ) -> Result<OpenAIResponse> {
        if provider.provider.builtin_id.is_some() {
            let mut stream = super::provider_runtime::stream(provider, messages, system_prompt, tools, max_tokens).await?;
            let mut text = String::new();
            let mut thinking = String::new();
            let mut calls = std::collections::BTreeMap::<usize, Value>::new();
            let mut native = None;
            let mut usage = None;
            let mut finish = None;
            while let Some(chunk) = stream.next().await {
                let chunk = chunk?;
                let data = chunk.trim().strip_prefix("data: ").unwrap_or("");
                if data == "[DONE]" { continue; }
                let value: Value = serde_json::from_str(data)?;
                let choice = &value["choices"][0];
                if let Some(part) = choice["delta"]["content"].as_str() { text.push_str(part); }
                if let Some(part) = choice["delta"]["reasoning_content"].as_str() { thinking.push_str(part); }
                if let Some(items) = choice["delta"]["tool_calls"].as_array() {
                    for item in items {
                        let index = item["index"].as_u64().unwrap_or_default() as usize;
                        let call = calls.entry(index).or_insert_with(|| json!({"id":"","type":"function","function":{"name":"","arguments":""}}));
                        if let Some(id) = item["id"].as_str() { call["id"] = json!(id); }
                        for field in ["name", "arguments"] {
                            if let Some(delta) = item["function"][field].as_str() {
                                call["function"][field] = json!(format!("{}{}", call["function"][field].as_str().unwrap_or_default(), delta));
                            }
                        }
                    }
                }
                if let Some(reason) = choice["finish_reason"].as_str() { finish = Some(reason.to_string()); }
                if value.get("provider_content").is_some() { native = Some(value["provider_content"].clone()); }
                if value.get("usage").is_some() { usage = Some(serde_json::from_value(value["usage"].clone())?); }
            }
            return Ok(OpenAIResponse { id: uuid::Uuid::new_v4().to_string(), object: "chat.completion".into(), created: 0, model: provider.model.id.clone(),
                choices: vec![OpenAIChoice { index: 0, finish_reason: finish, message: OpenAIMessage { role: "assistant".into(), content: OpenAIContent::Text(text),
                    tool_calls: if calls.is_empty() { None } else { Some(calls.into_values().map(serde_json::from_value).collect::<std::result::Result<Vec<_>,_>>()?) }, tool_call_id: None,
                    reasoning_content: if thinking.is_empty() { None } else { Some(thinking) }, provider_content: native } }], usage });
        }
        let base_url = crate::native_engine::provider_manager::ProviderManager::normalize_base_url(&provider.provider.base_url);
        let url = format!("{}/v1/chat/completions", base_url.trim_end_matches('/'));

        let mut body_messages = Vec::new();
        
        if let Some(system) = system_prompt {
            body_messages.push(OpenAIMessage {
                role: "system".to_string(),
                content: OpenAIContent::Text(system.to_string()),
                tool_calls: None,
                tool_call_id: None,
                reasoning_content: None, provider_content: None,
            });
        }
        
        body_messages.extend(messages);

        let mut body = json!({
            "model": provider.model.id,
            "max_tokens": max_tokens,
            "messages": body_messages,
        });

        if !tools.is_empty() {
            let tool_defs: Vec<Value> = tools.iter().map(|t| {
                json!({
                    "type": "function",
                    "function": {
                        "name": t.name,
                        "description": t.description,
                        "parameters": t.input_schema,
                    }
                })
            }).collect();
            body["tools"] = json!(tool_defs);
        }

        apply_effort(&mut body, provider.model.effort.as_deref(), &provider.provider.base_url);
        if let Some(messages) = body["messages"].as_array_mut() { for message in messages { if let Some(object) = message.as_object_mut() { object.remove("provider_content"); } } }
        let data: OpenAIResponse = retry_with_backoff(|| async {
            let response = self.client
                .post(&url)
                .header("Authorization", format!("Bearer {}", provider.provider.api_key))
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
        messages: Vec<OpenAIMessage>,
        system_prompt: Option<&str>,
        tools: Vec<ToolDefinition>,
        max_tokens: u32,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<String>> + Send>>> {
        if provider.provider.builtin_id.is_some() {
            return super::provider_runtime::stream(provider, messages, system_prompt, tools, max_tokens).await;
        }
        let base_url = crate::native_engine::provider_manager::ProviderManager::normalize_base_url(&provider.provider.base_url);
        let url = format!("{}/v1/chat/completions", base_url.trim_end_matches('/'));

        let mut body_messages = Vec::new();
        
        if let Some(system) = system_prompt {
            body_messages.push(OpenAIMessage {
                role: "system".to_string(),
                content: OpenAIContent::Text(system.to_string()),
                tool_calls: None,
                tool_call_id: None,
                reasoning_content: None, provider_content: None,
            });
        }
        
        body_messages.extend(messages);

        let mut body = json!({
            "model": provider.model.id,
            "max_tokens": max_tokens,
            "messages": body_messages,
            "stream": true,
            "stream_options": { "include_usage": true },
        });

        if !tools.is_empty() {
            let tool_defs: Vec<Value> = tools.iter().map(|t| {
                json!({
                    "type": "function",
                    "function": {
                        "name": t.name,
                        "description": t.description,
                        "parameters": t.input_schema,
                    }
                })
            }).collect();
            body["tools"] = json!(tool_defs);
        }

        apply_effort(&mut body, provider.model.effort.as_deref(), &provider.provider.base_url);
        if let Some(messages) = body["messages"].as_array_mut() { for message in messages { if let Some(object) = message.as_object_mut() { object.remove("provider_content"); } } }
        let response = retry_with_backoff(|| async {
            let resp = self.client
                .post(&url)
                .header("Authorization", format!("Bearer {}", provider.provider.api_key))
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

impl Default for OpenAIClient {
    fn default() -> Self {
        Self::new()
    }
}

pub fn apply_effort(body: &mut Value, effort: Option<&str>, base_url: &str) {
    if let Some(effort) = effort {
        if base_url.contains("deepseek.com") {
            body["thinking"] = json!({"type": if effort == "off" { "disabled" } else { "enabled" }});
            if effort != "off" { body["reasoning_effort"] = json!(effort); }
        } else { body["reasoning_effort"] = json!(if effort == "off" { "none" } else { effort }); }
    }
}
