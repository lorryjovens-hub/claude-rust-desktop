//! LSP Tool - Language Server Protocol integration
//!
//! Provides code intelligence via LSP: symbols, references, diagnostics, etc.
//! Based on claw-code-main's LSP tool implementation.

use crate::tools::{Tool, ToolOutput, ToolError};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum LspAction {
    Symbols,
    References,
    Diagnostics,
    Definition,
    Hover,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LspSymbol {
    pub name: String,
    pub kind: String,
    pub location: LspLocation,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LspLocation {
    pub path: String,
    pub line: u32,
    pub character: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LspReference {
    pub symbol: String,
    pub location: LspLocation,
    pub context: String,
}

pub struct LspTool;

impl LspTool {
    pub fn new() -> Self {
        Self
    }

    fn find_symbols_in_file(path: &str) -> Vec<LspSymbol> {
        let path = PathBuf::from(path);
        if !path.exists() {
            return Vec::new();
        }

        let content = std::fs::read_to_string(&path).unwrap_or_default();
        let mut symbols = Vec::new();

        for (i, line) in content.lines().enumerate() {
            let trimmed = line.trim();
            if trimmed.starts_with("fn ") || trimmed.starts_with("pub fn ") ||
               trimmed.starts_with("struct ") || trimmed.starts_with("pub struct ") ||
               trimmed.starts_with("impl ") || trimmed.starts_with("enum ") ||
               trimmed.starts_with("trait ") || trimmed.starts_with("mod ") {
                let name = trimmed.split_whitespace()
                    .nth(1)
                    .unwrap_or(trimmed)
                    .split(|c: char| c == '{' || c == '<' || c == '(')
                    .next()
                    .unwrap_or(trimmed)
                    .to_string();

                symbols.push(LspSymbol {
                    name,
                    kind: if trimmed.starts_with("fn ") || trimmed.starts_with("pub fn ") {
                        "function".to_string()
                    } else if trimmed.starts_with("struct ") || trimmed.starts_with("pub struct ") {
                        "class".to_string()
                    } else if trimmed.starts_with("enum ") {
                        "enum".to_string()
                    } else if trimmed.starts_with("trait ") {
                        "interface".to_string()
                    } else {
                        "module".to_string()
                    },
                    location: LspLocation {
                        path: path.to_string_lossy().to_string(),
                        line: i as u32,
                        character: 0,
                    },
                });
            }
        }

        symbols
    }

    fn find_references_in_file(path: &str, query: &str) -> Vec<LspReference> {
        let path = PathBuf::from(path);
        if !path.exists() || query.is_empty() {
            return Vec::new();
        }

        let content = std::fs::read_to_string(&path).unwrap_or_default();
        let mut references = Vec::new();

        for (i, line) in content.lines().enumerate() {
            if line.contains(query) {
                let col = line.find(query).unwrap_or(0);
                let context: String = line.chars()
                    .skip(col.saturating_sub(20))
                    .take(40)
                    .collect();

                references.push(LspReference {
                    symbol: query.to_string(),
                    location: LspLocation {
                        path: path.to_string_lossy().to_string(),
                        line: i as u32,
                        character: col as u32,
                    },
                    context,
                });
            }
        }

        references
    }

    async fn handle_symbols(&self, path: &str) -> Result<Vec<LspSymbol>, ToolError> {
        if path.is_empty() {
            return Err(ToolError {
                message: "path is required".to_string(),
                code: Some("missing_path".to_string()),
            });
        }

        let symbols = Self::find_symbols_in_file(path);
        Ok(symbols)
    }

    async fn handle_references(&self, path: &str, query: &str) -> Result<Vec<LspReference>, ToolError> {
        if path.is_empty() {
            return Err(ToolError {
                message: "path is required".to_string(),
                code: Some("missing_path".to_string()),
            });
        }

        let references = Self::find_references_in_file(path, query);
        Ok(references)
    }

    async fn handle_diagnostics(&self, path: &str) -> Result<String, ToolError> {
        if path.is_empty() {
            return Err(ToolError {
                message: "path is required".to_string(),
                code: Some("missing_path".to_string()),
            });
        }

        let path = PathBuf::from(path);
        if !path.exists() {
            return Err(ToolError {
                message: format!("File not found: {}", path.display()),
                code: Some("file_not_found".to_string()),
            });
        }

        Ok("LSP diagnostics placeholder - integrate with rust-analyzer or clangd".to_string())
    }

    async fn handle_definition(&self, path: &str, line: u32, character: u32) -> Result<Option<LspLocation>, ToolError> {
        if path.is_empty() {
            return Err(ToolError {
                message: "path is required".to_string(),
                code: Some("missing_path".to_string()),
            });
        }

        let symbols = Self::find_symbols_in_file(path);
        for symbol in symbols {
            if symbol.location.line == line {
                return Ok(Some(symbol.location));
            }
        }

        Ok(None)
    }

    async fn handle_hover(&self, path: &str, line: u32, character: u32, query: &str) -> Result<String, ToolError> {
        if path.is_empty() {
            return Err(ToolError {
                message: "path is required".to_string(),
                code: Some("missing_path".to_string()),
            });
        }

        let symbols = Self::find_symbols_in_file(path);
        if let Some(symbol) = symbols.iter().find(|s| s.name == query || s.location.line == line) {
            return Ok(format!("{} ({})\nLocation: {}:{}", symbol.name, symbol.kind, symbol.location.path, symbol.location.line));
        }

        Ok(format!("No hover info found for query: {}", query))
    }
}

impl Default for LspTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for LspTool {
    fn name(&self) -> &str {
        "lsp"
    }

    fn description(&self) -> &str {
        "Query Language Server Protocol for code intelligence (symbols, references, diagnostics)"
    }

    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["symbols", "references", "diagnostics", "definition", "hover"],
                    "description": "LSP action to perform"
                },
                "path": {
                    "type": "string",
                    "description": "File path to query"
                },
                "line": {
                    "type": "integer",
                    "minimum": 0,
                    "description": "Line number (0-indexed)"
                },
                "character": {
                    "type": "integer",
                    "minimum": 0,
                    "description": "Character position (0-indexed)"
                },
                "query": {
                    "type": "string",
                    "description": "Search query for references/hover"
                }
            },
            "required": ["action"]
        })
    }

    async fn execute(&self, input: serde_json::Value) -> Result<ToolOutput, ToolError> {
        let action = input["action"].as_str()
            .ok_or_else(|| ToolError {
                message: "action is required".to_string(),
                code: Some("missing_action".to_string()),
            })?;

        let path = input["path"].as_str().unwrap_or("");
        let line = input["line"].as_u64().unwrap_or(0) as u32;
        let character = input["character"].as_u64().unwrap_or(0) as u32;
        let query = input["query"].as_str().unwrap_or("");

        let result = match action {
            "symbols" => {
                let symbols = self.handle_symbols(path).await?;
                serde_json::json!({
                    "success": true,
                    "symbols": symbols,
                    "count": symbols.len()
                }).to_string()
            }
            "references" => {
                let references = self.handle_references(path, query).await?;
                serde_json::json!({
                    "success": true,
                    "references": references,
                    "count": references.len()
                }).to_string()
            }
            "diagnostics" => {
                let result = self.handle_diagnostics(path).await?;
                serde_json::json!({
                    "success": true,
                    "diagnostics": result
                }).to_string()
            }
            "definition" => {
                let location = self.handle_definition(path, line, character).await?;
                serde_json::json!({
                    "success": true,
                    "location": location
                }).to_string()
            }
            "hover" => {
                let result = self.handle_hover(path, line, character, query).await?;
                serde_json::json!({
                    "success": true,
                    "hover": result
                }).to_string()
            }
            _ => return Err(ToolError {
                message: format!("Unknown action: {}", action),
                code: Some("invalid_action".to_string()),
            }),
        };

        Ok(ToolOutput {
            output_type: "json".to_string(),
            content: result,
            metadata: HashMap::new(),
        })
    }
}