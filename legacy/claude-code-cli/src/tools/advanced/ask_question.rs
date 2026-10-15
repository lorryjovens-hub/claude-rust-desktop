//! AskQuestion Tool - Interactive user prompts
//!
//! Asks the user a question and waits for their response.

use crate::tools::{Tool, ToolOutput, ToolError};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub struct AskQuestionTool;

impl AskQuestionTool {
    pub fn new() -> Self {
        Self
    }
}

impl Default for AskQuestionTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for AskQuestionTool {
    fn name(&self) -> &str {
        "ask_user_question"
    }

    fn description(&self) -> &str {
        "Ask the user a question and wait for their response"
    }

    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "question": {
                    "type": "string",
                    "description": "Question to ask the user"
                },
                "options": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Optional list of choices"
                }
            },
            "required": ["question"]
        })
    }

    async fn execute(&self, input: serde_json::Value) -> Result<ToolOutput, ToolError> {
        let question = input["question"].as_str()
            .ok_or_else(|| ToolError {
                message: "question is required".to_string(),
                code: Some("missing_question".to_string()),
            })?;

        let options = input["options"].as_array()
            .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect::<Vec<_>>())
            .unwrap_or_default();

        let options_text = if options.is_empty() {
            String::new()
        } else {
            format!("\nOptions:\n{}", options.iter().enumerate()
                .map(|(i, o)| format!("  {}. {}", i + 1, o))
                .collect::<Vec<_>>()
                .join("\n"))
        };

        Ok(ToolOutput {
            output_type: "text".to_string(),
            content: format!("Question: {}{}\n\nAwaiting user response...", question, options_text),
            metadata: HashMap::new(),
        })
    }
}