//! Brief Tool - Generate brief summaries of content
//!
//! Provides concise summarization capabilities for various types of content.

use crate::tools::{Tool, ToolOutput, ToolError};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BriefOptions {
    pub max_length: Option<usize>,
    pub format: BriefFormat,
    pub include_keywords: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum BriefFormat {
    Short,
    Medium,
    Detailed,
    BulletPoints,
}

impl std::str::FromStr for BriefFormat {
    type Err = ();
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "short" => Ok(BriefFormat::Short),
            "medium" => Ok(BriefFormat::Medium),
            "detailed" => Ok(BriefFormat::Detailed),
            "bulletpoints" | "bullets" => Ok(BriefFormat::BulletPoints),
            _ => Err(()),
        }
    }
}

pub struct BriefTool;

impl Default for BriefTool {
    fn default() -> Self {
        Self::new()
    }
}

impl BriefTool {
    pub fn new() -> Self {
        Self
    }

    fn extract_keywords(&self, content: &str) -> Vec<String> {
        let stop_words = vec![
            "the", "a", "an", "and", "or", "but", "in", "on", "at", "to", "for",
            "of", "with", "by", "from", "as", "is", "was", "are", "were", "be",
            "been", "being", "have", "has", "had", "do", "does", "did", "will",
            "would", "could", "should", "may", "might", "can", "this", "that",
            "these", "those", "i", "you", "he", "she", "it", "we", "they"
        ];

        let words: Vec<String> = content
            .split(|c: char| !c.is_alphanumeric())
            .filter(|s| !s.is_empty())
            .map(|s| s.to_lowercase())
            .filter(|s| s.len() > 3)
            .filter(|s| !stop_words.contains(&s.as_str()))
            .collect();

        let mut word_count: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
        for word in &words {
            *word_count.entry(word.clone()).or_insert(0) += 1;
        }

        let mut keywords: Vec<(String, usize)> = word_count.into_iter().collect();
        keywords.sort_by(|a, b| b.1.cmp(&a.1));

        keywords.into_iter()
            .take(10)
            .map(|(word, _)| word)
            .collect()
    }

    fn generate_brief(&self, content: &str, format: &BriefFormat, max_length: Option<usize>) -> String {
        let trimmed = content.trim();
        let target_length = max_length.unwrap_or(match format {
            BriefFormat::Short => 50,
            BriefFormat::Medium => 150,
            BriefFormat::Detailed => 300,
            BriefFormat::BulletPoints => 200,
        });

        if trimmed.len() <= target_length {
            return trimmed.to_string();
        }

        let truncated = &trimmed[..target_length];

        if let Some(last_space) = truncated.rfind(' ') {
            format!("{}...", &truncated[..last_space])
        } else {
            format!("{}...", truncated)
        }
    }

    async fn handle_brief(&self, input: &serde_json::Value) -> Result<String, ToolError> {
        let content = input["content"].as_str()
            .ok_or_else(|| ToolError {
                message: "content is required".to_string(),
                code: Some("missing_content".to_string()),
            })?;

        let format_str = input["format"].as_str().unwrap_or("medium");
        let format: BriefFormat = format_str.parse()
            .map_err(|_| ToolError {
                message: "Invalid format. Must be: short, medium, detailed, bulletpoints".to_string(),
                code: Some("invalid_format".to_string()),
            })?;

        let max_length = input["max_length"].as_u64().map(|v| v as usize);
        let include_keywords = input["include_keywords"].as_bool().unwrap_or(false);

        let brief = self.generate_brief(content, &format, max_length);

        if include_keywords {
            let keywords = self.extract_keywords(content);
            Ok(format!("{}\n\nKeywords: {}", brief, keywords.join(", ")))
        } else {
            Ok(brief)
        }
    }

    async fn handle_code_summary(&self, code: &str) -> Result<String, ToolError> {
        let lines: Vec<&str> = code.lines().collect();
        let summary = if lines.len() > 20 {
            format!(
                "Code file with {} lines. First 5 lines:\n{}\n\n... ({} more lines) ...\n\nLast 5 lines:\n{}",
                lines.len(),
                lines.iter().take(5).cloned().collect::<Vec<_>>().join("\n"),
                lines.len() - 10,
                lines.iter().rev().take(5).cloned().collect::<Vec<_>>().join("\n")
            )
        } else {
            format!("Code file with {} lines:\n{}", lines.len(), code)
        };

        Ok(summary)
    }

    async fn handle_diff_summary(&self, diff: &str) -> Result<String, ToolError> {
        let additions = diff.lines().filter(|l| l.starts_with('+')).count();
        let deletions = diff.lines().filter(|l| l.starts_with('-')).count();

        Ok(format!(
            "Diff summary: +{} additions, -{} deletions",
            additions, deletions
        ))
    }
}

#[async_trait]
impl Tool for BriefTool {
    fn name(&self) -> &str {
        "brief"
    }

    fn description(&self) -> &str {
        "Generate concise summaries and briefs from content"
    }

    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "operation": {
                    "type": "string",
                    "enum": ["brief", "code_summary", "diff_summary"]
                },
                "content": {
                    "type": "string",
                    "description": "Content to summarize"
                },
                "format": {
                    "type": "string",
                    "enum": ["short", "medium", "detailed", "bulletpoints"],
                    "description": "Output format"
                },
                "max_length": {
                    "type": "number",
                    "description": "Maximum length of output"
                },
                "include_keywords": {
                    "type": "boolean",
                    "description": "Include extracted keywords"
                },
                "code": {
                    "type": "string",
                    "description": "Code content for code_summary"
                },
                "diff": {
                    "type": "string",
                    "description": "Diff content for diff_summary"
                }
            },
            "required": ["operation"]
        })
    }

    async fn execute(&self, input: serde_json::Value) -> Result<ToolOutput, ToolError> {
        let operation = input["operation"].as_str()
            .ok_or_else(|| ToolError {
                message: "operation is required".to_string(),
                code: Some("missing_operation".to_string()),
            })?;

        let result = match operation {
            "brief" => {
                let brief = self.handle_brief(&input).await?;
                serde_json::json!({
                    "success": true,
                    "brief": brief
                }).to_string()
            }
            "code_summary" => {
                let code = input["code"].as_str()
                    .ok_or_else(|| ToolError {
                        message: "code is required".to_string(),
                        code: Some("missing_code".to_string()),
                    })?;
                let summary = self.handle_code_summary(code).await?;
                serde_json::json!({
                    "success": true,
                    "summary": summary
                }).to_string()
            }
            "diff_summary" => {
                let diff = input["diff"].as_str()
                    .ok_or_else(|| ToolError {
                        message: "diff is required".to_string(),
                        code: Some("missing_diff".to_string()),
                    })?;
                let summary = self.handle_diff_summary(diff).await?;
                serde_json::json!({
                    "success": true,
                    "summary": summary
                }).to_string()
            }
            _ => return Err(ToolError {
                message: format!("Unknown operation: {}", operation),
                code: Some("invalid_operation".to_string()),
            }),
        };

        Ok(ToolOutput {
            output_type: "json".to_string(),
            content: result,
            metadata: HashMap::new(),
        })
    }
}