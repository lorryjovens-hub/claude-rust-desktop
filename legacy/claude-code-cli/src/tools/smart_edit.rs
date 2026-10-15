//! Smart Edit Tool - Advanced code editing with diff/patch support
//!
//! Features:
//! - Multi-line replacements
//! - Context-aware edits
//! - Undo support
//! - Preview changes before applying

use super::{Tool, ToolOutput, ToolError};
use async_trait::async_trait;
use serde_json;
use std::path::Path;

pub struct SmartEditTool;

impl Default for SmartEditTool {
    fn default() -> Self {
        Self::new()
    }
}

impl SmartEditTool {
    pub fn new() -> Self {
        Self
    }

    /// Apply a smart edit with context awareness
    #[allow(dead_code)]
    async fn smart_replace(
        &self,
        file_path: &str,
        old_content: &str,
        new_content: &str,
        _context_lines: usize,
    ) -> Result<ToolOutput, ToolError> {
        let path = Path::new(file_path);

        if !path.exists() {
            return Err(ToolError {
                message: format!("File does not exist: {}", file_path),
                code: Some("file_not_found".to_string()),
            });
        }

        let content = tokio::fs::read_to_string(path).await
            .map_err(|e| ToolError {
                message: format!("Failed to read file: {}", e),
                code: Some("read_error".to_string()),
            })?;

        // Try exact match first
        if content.contains(old_content) {
            let new_file_content = content.replace(old_content, new_content);

            tokio::fs::write(path, new_file_content).await
                .map_err(|e| ToolError {
                    message: format!("Failed to write file: {}", e),
                    code: Some("write_error".to_string()),
                })?;

            return Ok(ToolOutput {
                output_type: "smart_edit".to_string(),
                content: format!("Successfully edited {} (exact match)", file_path),
                metadata: [
                    ("file_path".to_string(), serde_json::json!(file_path)),
                    ("lines_changed".to_string(), serde_json::json!(old_content.lines().count())),
                ].into_iter().collect(),
            });
        }

        // Try fuzzy matching with context
        let lines: Vec<&str> = content.lines().collect();
        let old_lines: Vec<&str> = old_content.lines().collect();

        if old_lines.is_empty() {
            return Err(ToolError {
                message: "old_content cannot be empty".to_string(),
                code: Some("empty_content".to_string()),
            });
        }

        // Find best matching position
        let mut best_match_idx = None;
        let mut best_match_score = 0.0;

        for i in 0..lines.len().saturating_sub(old_lines.len() - 1) {
            let score = self.calculate_match_score(&lines[i..i + old_lines.len()], &old_lines);
            if score > best_match_score && score >= 0.8 {
                best_match_score = score;
                best_match_idx = Some(i);
            }
        }

        if let Some(idx) = best_match_idx {
            let new_lines: Vec<&str> = new_content.lines().collect();
            let mut result_lines = lines[..idx].to_vec();
            result_lines.extend(new_lines);
            result_lines.extend(&lines[idx + old_lines.len()..]);

            let new_content = result_lines.join("\n");
            tokio::fs::write(path, new_content).await
                .map_err(|e| ToolError {
                    message: format!("Failed to write file: {}", e),
                    code: Some("write_error".to_string()),
                })?;

            return Ok(ToolOutput {
                output_type: "smart_edit".to_string(),
                content: format!(
                    "Successfully edited {} (fuzzy match, confidence: {:.1}%)",
                    file_path,
                    best_match_score * 100.0
                ),
                metadata: [
                    ("file_path".to_string(), serde_json::json!(file_path)),
                    ("match_confidence".to_string(), serde_json::json!(best_match_score)),
                    ("lines_changed".to_string(), serde_json::json!(old_lines.len())),
                ].into_iter().collect(),
            });
        }

        Err(ToolError {
            message: "Could not find matching content in file".to_string(),
            code: Some("no_match".to_string()),
        })
    }

    /// Calculate similarity score between two line sets
    fn calculate_match_score(&self, lines1: &[&str], lines2: &[&str]) -> f32 {
        if lines1.len() != lines2.len() {
            return 0.0;
        }

        let mut total_score = 0.0;
        for (l1, l2) in lines1.iter().zip(lines2.iter()) {
            let l1_trimmed = l1.trim();
            let l2_trimmed = l2.trim();

            if l1_trimmed == l2_trimmed {
                total_score += 1.0;
            } else if l1_trimmed.contains(l2_trimmed) || l2_trimmed.contains(l1_trimmed) {
                total_score += 0.5;
            }
        }

        total_score / lines1.len() as f32
    }

    /// Insert content at a specific line
    async fn insert_at_line(
        &self,
        file_path: &str,
        content: &str,
        line_number: usize,
        after: bool,
    ) -> Result<ToolOutput, ToolError> {
        let path = Path::new(file_path);

        let file_content = tokio::fs::read_to_string(path).await
            .map_err(|e| ToolError {
                message: format!("Failed to read file: {}", e),
                code: Some("read_error".to_string()),
            })?;

        let lines: Vec<&str> = file_content.lines().collect();

        let insert_idx = if after {
            line_number
        } else {
            line_number.saturating_sub(1)
        };

        let insert_lines: Vec<&str> = content.lines().collect();

        // Insert lines at position
        let mut new_lines = lines[..insert_idx.min(lines.len())].to_vec();
        let lines_count = insert_lines.len();
        new_lines.extend(insert_lines);
        new_lines.extend(&lines[insert_idx.min(lines.len())..]);

        let new_content = new_lines.join("\n");
        tokio::fs::write(path, new_content).await
            .map_err(|e| ToolError {
                message: format!("Failed to write file: {}", e),
                code: Some("write_error".to_string()),
            })?;

        Ok(ToolOutput {
            output_type: "smart_edit".to_string(),
            content: format!("Inserted content at line {}", line_number),
            metadata: [
                ("file_path".to_string(), serde_json::json!(file_path)),
                ("line_number".to_string(), serde_json::json!(line_number)),
                ("lines_inserted".to_string(), serde_json::json!(lines_count)),
            ].into_iter().collect(),
        })
    }

    /// Delete specific lines
    async fn delete_lines(
        &self,
        file_path: &str,
        start_line: usize,
        end_line: usize,
    ) -> Result<ToolOutput, ToolError> {
        let path = Path::new(file_path);

        let file_content = tokio::fs::read_to_string(path).await
            .map_err(|e| ToolError {
                message: format!("Failed to read file: {}", e),
                code: Some("read_error".to_string()),
            })?;

        let lines: Vec<&str> = file_content.lines().collect();

        if start_line > lines.len() || end_line > lines.len() {
            return Err(ToolError {
                message: "Line number out of range".to_string(),
                code: Some("invalid_line".to_string()),
            });
        }

        let new_lines: Vec<&str> = lines[..start_line - 1]
            .iter()
            .chain(&lines[end_line..])
            .copied()
            .collect();

        let new_content = new_lines.join("\n");
        tokio::fs::write(path, new_content).await
            .map_err(|e| ToolError {
                message: format!("Failed to write file: {}", e),
                code: Some("write_error".to_string()),
            })?;

        Ok(ToolOutput {
            output_type: "smart_edit".to_string(),
            content: format!("Deleted lines {}-{}", start_line, end_line),
            metadata: [
                ("file_path".to_string(), serde_json::json!(file_path)),
                ("start_line".to_string(), serde_json::json!(start_line)),
                ("end_line".to_string(), serde_json::json!(end_line)),
                ("lines_deleted".to_string(), serde_json::json!(end_line - start_line + 1)),
            ].into_iter().collect(),
        })
    }

    /// Generate a diff/patch preview
    async fn preview_changes(
        &self,
        file_path: &str,
        old_content: &str,
        new_content: &str,
    ) -> Result<ToolOutput, ToolError> {
        let path = Path::new(file_path);

        let file_content = tokio::fs::read_to_string(path).await
            .map_err(|e| ToolError {
                message: format!("Failed to read file: {}", e),
                code: Some("read_error".to_string()),
            })?;

        // Generate diff
        let diff = similar::TextDiff::from_lines(old_content, new_content);
        let mut diff_output = String::new();

        for change in diff.iter_all_changes() {
            match change.tag() {
                similar::ChangeTag::Delete => {
                    diff_output.push_str(&format!("-{}\n", change.value()));
                }
                similar::ChangeTag::Insert => {
                    diff_output.push_str(&format!("+{}\n", change.value()));
                }
                similar::ChangeTag::Equal => {
                    diff_output.push_str(&format!(" {}\n", change.value()));
                }
            }
        }

        Ok(ToolOutput {
            output_type: "diff_preview".to_string(),
            content: diff_output,
            metadata: [
                ("file_path".to_string(), serde_json::json!(file_path)),
                ("can_apply".to_string(), serde_json::json!(file_content.contains(old_content))),
            ].into_iter().collect(),
        })
    }
}

#[async_trait]
impl Tool for SmartEditTool {
    fn name(&self) -> &str {
        "smart_edit"
    }

    fn description(&self) -> &str {
        "Advanced code editing with fuzzy matching, multi-line replacements, and diff preview"
    }

    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "operation": {
                    "type": "string",
                    "enum": ["replace", "insert", "delete", "preview"],
                    "description": "Type of edit operation"
                },
                "file_path": {
                    "type": "string",
                    "description": "Path to the file to edit"
                },
                "old_content": {
                    "type": "string",
                    "description": "Content to find and replace (for replace/preview)"
                },
                "new_content": {
                    "type": "string",
                    "description": "New content (for replace/insert/preview)"
                },
                "line_number": {
                    "type": "integer",
                    "description": "Line number for insert/delete operations"
                },
                "start_line": {
                    "type": "integer",
                    "description": "Start line for delete operation"
                },
                "end_line": {
                    "type": "integer",
                    "description": "End line for delete operation"
                },
                "after": {
                    "type": "boolean",
                    "description": "Insert after the line (true) or before (false)"
                },
                "context_lines": {
                    "type": "integer",
                    "description": "Number of context lines for fuzzy matching",
                    "default": 2
                }
            },
            "required": ["operation", "file_path"]
        })
    }

    async fn execute(&self,
        input: serde_json::Value
    ) -> Result<ToolOutput, ToolError> {
        let operation = input["operation"].as_str()
            .ok_or_else(|| ToolError {
                message: "operation is required".to_string(),
                code: Some("missing_parameter".to_string()),
            })?;

        let file_path = input["file_path"].as_str()
            .ok_or_else(|| ToolError {
                message: "file_path is required".to_string(),
                code: Some("missing_parameter".to_string()),
            })?;

        match operation {
            "replace" => {
                let old_content = input["old_content"].as_str()
                    .ok_or_else(|| ToolError {
                        message: "old_content is required for replace".to_string(),
                        code: Some("missing_parameter".to_string()),
                    })?;
                let new_content = input["new_content"].as_str()
                    .ok_or_else(|| ToolError {
                        message: "new_content is required for replace".to_string(),
                        code: Some("missing_parameter".to_string()),
                    })?;
                let context_lines = input["context_lines"].as_u64().unwrap_or(2) as usize;
                self.smart_replace(file_path, old_content, new_content, context_lines).await
            }
            "insert" => {
                let content = input["new_content"].as_str()
                    .ok_or_else(|| ToolError {
                        message: "new_content is required for insert".to_string(),
                        code: Some("missing_parameter".to_string()),
                    })?;
                let line_number = input["line_number"].as_u64()
                    .ok_or_else(|| ToolError {
                        message: "line_number is required for insert".to_string(),
                        code: Some("missing_parameter".to_string()),
                    })? as usize;
                let after = input["after"].as_bool().unwrap_or(true);
                self.insert_at_line(file_path, content, line_number, after).await
            }
            "delete" => {
                let start_line = input["start_line"].as_u64()
                    .ok_or_else(|| ToolError {
                        message: "start_line is required for delete".to_string(),
                        code: Some("missing_parameter".to_string()),
                    })? as usize;
                let end_line = input["end_line"].as_u64()
                    .ok_or_else(|| ToolError {
                        message: "end_line is required for delete".to_string(),
                        code: Some("missing_parameter".to_string()),
                    })? as usize;
                self.delete_lines(file_path, start_line, end_line).await
            }
            "preview" => {
                let old_content = input["old_content"].as_str()
                    .ok_or_else(|| ToolError {
                        message: "old_content is required for preview".to_string(),
                        code: Some("missing_parameter".to_string()),
                    })?;
                let new_content = input["new_content"].as_str()
                    .ok_or_else(|| ToolError {
                        message: "new_content is required for preview".to_string(),
                        code: Some("missing_parameter".to_string()),
                    })?;
                self.preview_changes(file_path, old_content, new_content).await
            }
            _ => Err(ToolError {
                message: format!("Unknown operation: {}", operation),
                code: Some("invalid_operation".to_string()),
            }),
        }
    }
}
