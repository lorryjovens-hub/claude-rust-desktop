//! Glob Tool - File pattern matching
//!
//! This tool allows searching for files using glob patterns like "**/*.rs", "src/**/*.ts", etc.

use super::{Tool, ToolOutput, ToolError};
use async_trait::async_trait;
use serde_json;
use std::path::Path;

pub struct GlobTool;

impl Default for GlobTool {
    fn default() -> Self {
        Self::new()
    }
}

impl GlobTool {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Tool for GlobTool {
    fn name(&self) -> &str {
        "glob"
    }

    fn description(&self) -> &str {
        "Search for files using glob patterns (e.g., **/*.rs, src/**/*.ts)"
    }

    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "pattern": {
                    "type": "string",
                    "description": "The glob pattern to match files against (e.g., **/*.rs, src/**/*.ts)"
                },
                "path": {
                    "type": "string",
                    "description": "The directory to search in. If not specified, the current working directory will be used."
                },
                "limit": {
                    "type": "integer",
                    "description": "Maximum number of files to return (default: 100)"
                }
            },
            "required": ["pattern"]
        })
    }

    async fn execute(&self, input: serde_json::Value) -> Result<ToolOutput, ToolError> {
        let pattern = input["pattern"].as_str()
            .ok_or_else(|| ToolError {
                message: "pattern is required".to_string(),
                code: Some("missing_parameter".to_string()),
            })?;

        let base_path = input["path"].as_str().unwrap_or(".");
        let limit = input["limit"].as_u64().unwrap_or(100) as usize;

        let search_path = Path::new(base_path);

        if !search_path.exists() {
            return Err(ToolError {
                message: format!("Path does not exist: {}", base_path),
                code: Some("path_not_found".to_string()),
            });
        }

        if !search_path.is_dir() {
            return Err(ToolError {
                message: format!("Path is not a directory: {}", base_path),
                code: Some("not_directory".to_string()),
            });
        }

        // Build the full pattern by joining base path with the glob pattern
        let full_pattern = if pattern.starts_with('/') || pattern.starts_with('\\') {
            pattern.to_string()
        } else {
            format!("{}/{}", base_path.trim_end_matches(&['/', '\\'][..]), pattern)
        };

        let mut results = Vec::new();
        let mut truncated = false;

        // Use glob crate for pattern matching
        match glob::glob(&full_pattern) {
            Ok(paths) => {
                for entry in paths.take(limit + 1) {
                    match entry {
                        Ok(path) => {
                            if results.len() >= limit {
                                truncated = true;
                                break;
                            }
                            // Convert to relative path if possible
                            let display_path = if let Ok(rel_path) = path.strip_prefix(".") {
                                rel_path.to_string_lossy().to_string()
                            } else {
                                path.to_string_lossy().to_string()
                            };
                            results.push(display_path);
                        }
                        Err(e) => {
                            eprintln!("Glob error: {}", e);
                        }
                    }
                }
            }
            Err(e) => {
                return Err(ToolError {
                    message: format!("Invalid glob pattern: {}", e),
                    code: Some("invalid_pattern".to_string()),
                });
            }
        }

        let mut content = results.join("\n");
        if truncated {
            content.push_str("\n\n(Results are truncated. Consider using a more specific path or pattern.)");
        } else if results.is_empty() {
            content = "No files found".to_string();
        }

        Ok(ToolOutput {
            output_type: "text".to_string(),
            content,
            metadata: {
                let mut meta = std::collections::HashMap::new();
                meta.insert("num_files".to_string(), serde_json::json!(results.len()));
                meta.insert("truncated".to_string(), serde_json::json!(truncated));
                meta
            },
        })
    }
}
