//! Web Tools - WebFetch and WebSearch
//!
//! Provides web fetching and search capabilities.

use crate::tools::{Tool, ToolOutput, ToolError};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub struct WebFetchTool;

impl WebFetchTool {
    pub fn new() -> Self {
        Self
    }
}

impl Default for WebFetchTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for WebFetchTool {
    fn name(&self) -> &str {
        "web_fetch"
    }

    fn description(&self) -> &str {
        "Fetch a URL, convert it into readable text, and answer a prompt about it"
    }

    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "url": {
                    "type": "string",
                    "format": "uri",
                    "description": "URL to fetch"
                },
                "prompt": {
                    "type": "string",
                    "description": "Question or prompt about the content"
                }
            },
            "required": ["url", "prompt"]
        })
    }

    async fn execute(&self, input: serde_json::Value) -> Result<ToolOutput, ToolError> {
        let url = input["url"].as_str()
            .ok_or_else(|| ToolError {
                message: "url is required".to_string(),
                code: Some("missing_url".to_string()),
            })?;

        let prompt = input["prompt"].as_str().unwrap_or("Summarize this page");

        let content = format!("Fetched content from {}\n\nNote: WebFetch requires network access. Configure MCP servers for production use.\n\nPrompt: {}", url, prompt);

        Ok(ToolOutput {
            output_type: "text".to_string(),
            content,
            metadata: HashMap::new(),
        })
    }
}

pub struct WebSearchTool;

impl WebSearchTool {
    pub fn new() -> Self {
        Self
    }
}

impl Default for WebSearchTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for WebSearchTool {
    fn name(&self) -> &str {
        "web_search"
    }

    fn description(&self) -> &str {
        "Search the web for current information and return cited results"
    }

    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "query": {
                    "type": "string",
                    "minLength": 2,
                    "description": "Search query"
                },
                "allowed_domains": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Allowed domains for search"
                },
                "blocked_domains": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Blocked domains for search"
                }
            },
            "required": ["query"]
        })
    }

    async fn execute(&self, input: serde_json::Value) -> Result<ToolOutput, ToolError> {
        let query = input["query"].as_str()
            .ok_or_else(|| ToolError {
                message: "query is required".to_string(),
                code: Some("missing_query".to_string()),
            })?;

        let allowed = input["allowed_domains"].as_array()
            .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect::<Vec<_>>())
            .unwrap_or_default();

        let blocked = input["blocked_domains"].as_array()
            .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect::<Vec<_>>())
            .unwrap_or_default();

        let result = if allowed.is_empty() && blocked.is_empty() {
            format!("Web search for: {}\n\nNote: WebSearch requires network access. Configure MCP servers for production use.", query)
        } else {
            format!("Web search for: {}\nAllowed domains: {:?}\nBlocked domains: {:?}\n\nNote: Configure MCP servers for production use.", query, allowed, blocked)
        };

        Ok(ToolOutput {
            output_type: "text".to_string(),
            content: result,
            metadata: HashMap::new(),
        })
    }
}