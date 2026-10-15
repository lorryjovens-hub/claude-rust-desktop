//! NotebookEdit Tool - Jupyter notebook editing
//!
//! Provides comprehensive Jupyter notebook (.ipynb) editing capabilities.

use crate::tools::{Tool, ToolOutput, ToolError};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotebookCell {
    pub cell_type: CellType,
    pub source: Vec<String>,
    pub outputs: Option<Vec<CellOutput>>,
    pub metadata: Option<CellMetadata>,
    pub execution_count: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CellType {
    Code,
    Markdown,
    Raw,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CellOutput {
    pub output_type: String,
    pub text: Option<Vec<String>>,
    pub data: Option<HashMap<String, serde_json::Value>>,
    pub metadata: Option<HashMap<String, serde_json::Value>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CellMetadata {
    pub collapsed: Option<bool>,
    pub scrolled: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Notebook {
    pub nbformat: u32,
    pub nbformat_minor: u32,
    pub metadata: NotebookMetadata,
    pub cells: Vec<NotebookCell>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotebookMetadata {
    pub kernelspec: Option<KernelSpec>,
    pub language_info: Option<LanguageInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KernelSpec {
    pub name: String,
    pub display_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LanguageInfo {
    pub name: String,
    pub version: String,
    pub file_extension: String,
}

#[derive(Debug, Clone)]
pub struct NotebookEditTool {
    workspace_path: Option<String>,
}

impl Default for NotebookEditTool {
    fn default() -> Self {
        Self::new()
    }
}

impl NotebookEditTool {
    pub fn new() -> Self {
        Self {
            workspace_path: None,
        }
    }

    pub fn with_workspace(mut self, path: &str) -> Self {
        self.workspace_path = Some(path.to_string());
        self
    }

    fn load_notebook(&self, path: &str) -> Result<Notebook, ToolError> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| ToolError {
                message: format!("Failed to read notebook: {}", e),
                code: Some("read_error".to_string()),
            })?;

        let notebook: Notebook = serde_json::from_str(&content)
            .map_err(|e| ToolError {
                message: format!("Failed to parse notebook JSON: {}", e),
                code: Some("parse_error".to_string()),
            })?;

        Ok(notebook)
    }

    fn save_notebook(&self, path: &str, notebook: &Notebook) -> Result<(), ToolError> {
        let content = serde_json::to_string_pretty(notebook)
            .map_err(|e| ToolError {
                message: format!("Failed to serialize notebook: {}", e),
                code: Some("serialize_error".to_string()),
            })?;

        std::fs::write(path, content)
            .map_err(|e| ToolError {
                message: format!("Failed to write notebook: {}", e),
                code: Some("write_error".to_string()),
            })?;

        Ok(())
    }

    fn find_cell_by_index(cells: &[NotebookCell], index: usize) -> Option<(usize, &NotebookCell)> {
        if index < cells.len() {
            Some((index, &cells[index]))
        } else {
            None
        }
    }

    fn validate_path(&self, path: &str) -> Result<String, ToolError> {
        if !path.ends_with(".ipynb") {
            return Err(ToolError {
                message: "File must have .ipynb extension".to_string(),
                code: Some("invalid_extension".to_string()),
            });
        }

        if let Some(ref workspace) = self.workspace_path {
            let full_path = Path::new(workspace).join(path);
            let full_path_str = full_path.to_string_lossy().to_string();
            if !full_path_str.starts_with(workspace) {
                return Err(ToolError {
                    message: "Path outside workspace".to_string(),
                    code: Some("path_outside_workspace".to_string()),
                });
            }
            Ok(full_path_str)
        } else {
            Ok(path.to_string())
        }
    }
}

#[derive(Debug, Deserialize)]
struct EditInput {
    path: String,
    operation: String,
    #[serde(default)]
    cell_index: Option<usize>,
    #[serde(default)]
    cell_type: Option<String>,
    #[serde(default)]
    source: Option<Vec<String>>,
    #[serde(default)]
    output: Option<serde_json::Value>,
    #[serde(default)]
    metadata: Option<HashMap<String, serde_json::Value>>,
}

#[async_trait]
impl Tool for NotebookEditTool {
    fn name(&self) -> &str {
        "NotebookEdit"
    }

    fn description(&self) -> &str {
        "Edit Jupyter notebook (.ipynb) files. Supports operations: read, create, add_cell, edit_cell, delete_cell, move_cell, clear_outputs"
    }

    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Path to the .ipynb file"
                },
                "operation": {
                    "type": "string",
                    "enum": ["read", "create", "add_cell", "edit_cell", "delete_cell", "move_cell", "clear_outputs", "list_cells"],
                    "description": "Operation to perform"
                },
                "cell_index": {
                    "type": "integer",
                    "description": "Cell index for operations"
                },
                "cell_type": {
                    "type": "string",
                    "enum": ["code", "markdown", "raw"],
                    "description": "Type of cell for add_cell operation"
                },
                "source": {
                    "type": "array",
                    "items": {"type": "string"},
                    "description": "Cell source code/content"
                },
                "output": {
                    "type": "object",
                    "description": "Cell output to set"
                },
                "metadata": {
                    "type": "object",
                    "description": "Additional cell metadata"
                }
            },
            "required": ["path", "operation"]
        })
    }

    async fn execute(&self, input: serde_json::Value) -> Result<ToolOutput, ToolError> {
        let input: EditInput = serde_json::from_value(input)
            .map_err(|e| ToolError {
                message: format!("Invalid input: {}", e),
                code: Some("invalid_input".to_string()),
            })?;

        let path = self.validate_path(&input.path)?;

        match input.operation.as_str() {
            "read" => self.read_notebook(&path).await,
            "create" => self.create_notebook(&path).await,
            "add_cell" => self.add_cell(&path, input.cell_index, input.cell_type, input.source).await,
            "edit_cell" => self.edit_cell(&path, input.cell_index, input.source, input.metadata).await,
            "delete_cell" => self.delete_cell(&path, input.cell_index).await,
            "move_cell" => self.move_cell(&path, input.cell_index, input.metadata.as_ref().and_then(|m| m.get("new_index")).and_then(|v| v.as_u64()).map(|v| v as usize)).await,
            "clear_outputs" => self.clear_outputs(&path, input.cell_index).await,
            "list_cells" => self.list_cells(&path).await,
            _ => Err(ToolError {
                message: format!("Unknown operation: {}", input.operation),
                code: Some("unknown_operation".to_string()),
            }),
        }
    }
}

impl NotebookEditTool {
    async fn read_notebook(&self, path: &str) -> Result<ToolOutput, ToolError> {
        let notebook = self.load_notebook(path)?;

        let result = serde_json::json!({
            "path": path,
            "format_version": format!("{}.{}", notebook.nbformat, notebook.nbformat_minor),
            "cells_count": notebook.cells.len(),
            "cells": notebook.cells.iter().enumerate().map(|(i, c)| {
                serde_json::json!({
                    "index": i,
                    "type": c.cell_type,
                    "source": c.source.join(""),
                    "has_outputs": c.outputs.as_ref().map(|o| !o.is_empty()).unwrap_or(false),
                    "execution_count": c.execution_count,
                })
            }).collect::<Vec<_>>(),
            "metadata": notebook.metadata,
        });

        Ok(ToolOutput {
            output_type: "json".to_string(),
            content: serde_json::to_string_pretty(&result).unwrap(),
            metadata: HashMap::new(),
        })
    }

    async fn create_notebook(&self, path: &str) -> Result<ToolOutput, ToolError> {
        if Path::new(path).exists() {
            return Err(ToolError {
                message: "Notebook already exists".to_string(),
                code: Some("already_exists".to_string()),
            });
        }

        let notebook = Notebook {
            nbformat: 4,
            nbformat_minor: 5,
            metadata: NotebookMetadata {
                kernelspec: Some(KernelSpec {
                    name: "python".to_string(),
                    display_name: "Python 3".to_string(),
                }),
                language_info: Some(LanguageInfo {
                    name: "python".to_string(),
                    version: "3.9".to_string(),
                    file_extension: ".py".to_string(),
                }),
            },
            cells: Vec::new(),
        };

        self.save_notebook(path, &notebook)?;

        Ok(ToolOutput {
            output_type: "json".to_string(),
            content: serde_json::to_string_pretty(&serde_json::json!({
                "message": "Notebook created successfully",
                "path": path,
            })).unwrap(),
            metadata: HashMap::new(),
        })
    }

    async fn add_cell(
        &self,
        path: &str,
        index: Option<usize>,
        cell_type: Option<String>,
        source: Option<Vec<String>>,
    ) -> Result<ToolOutput, ToolError> {
        let mut notebook = self.load_notebook(path)?;

        let new_cell = NotebookCell {
            cell_type: match cell_type.as_deref() {
                Some("markdown") => CellType::Markdown,
                Some("raw") => CellType::Raw,
                _ => CellType::Code,
            },
            source: source.unwrap_or_default(),
            outputs: None,
            metadata: None,
            execution_count: None,
        };

        let insert_index = index.unwrap_or(notebook.cells.len());
        notebook.cells.insert(insert_index.min(notebook.cells.len()), new_cell);

        self.save_notebook(path, &notebook)?;

        Ok(ToolOutput {
            output_type: "json".to_string(),
            content: serde_json::to_string_pretty(&serde_json::json!({
                "message": "Cell added successfully",
                "cell_index": insert_index,
            })).unwrap(),
            metadata: HashMap::new(),
        })
    }

    async fn edit_cell(
        &self,
        path: &str,
        index: Option<usize>,
        source: Option<Vec<String>>,
        metadata: Option<HashMap<String, serde_json::Value>>,
    ) -> Result<ToolOutput, ToolError> {
        let mut notebook = self.load_notebook(path)?;
        let idx = index.ok_or_else(|| ToolError {
            message: "cell_index is required".to_string(),
            code: Some("missing_index".to_string()),
        })?;

        let cell = notebook.cells.get_mut(idx).ok_or_else(|| ToolError {
            message: format!("Cell at index {} not found", idx),
            code: Some("cell_not_found".to_string()),
        })?;

        if let Some(src) = source {
            cell.source = src;
        }

        if let Some(meta) = metadata {
            cell.metadata = Some(CellMetadata {
                collapsed: meta.get("collapsed").and_then(|v| v.as_bool()),
                scrolled: meta.get("scrolled").and_then(|v| v.as_bool()),
            });
        }

        self.save_notebook(path, &notebook)?;

        Ok(ToolOutput {
            output_type: "json".to_string(),
            content: serde_json::to_string_pretty(&serde_json::json!({
                "message": "Cell edited successfully",
                "cell_index": idx,
            })).unwrap(),
            metadata: HashMap::new(),
        })
    }

    async fn delete_cell(&self, path: &str, index: Option<usize>) -> Result<ToolOutput, ToolError> {
        let mut notebook = self.load_notebook(path)?;
        let idx = index.ok_or_else(|| ToolError {
            message: "cell_index is required".to_string(),
            code: Some("missing_index".to_string()),
        })?;

        if idx >= notebook.cells.len() {
            return Err(ToolError {
                message: format!("Cell at index {} not found", idx),
                code: Some("cell_not_found".to_string()),
            });
        }

        notebook.cells.remove(idx);

        self.save_notebook(path, &notebook)?;

        Ok(ToolOutput {
            output_type: "json".to_string(),
            content: serde_json::to_string_pretty(&serde_json::json!({
                "message": "Cell deleted successfully",
                "cells_remaining": notebook.cells.len(),
            })).unwrap(),
            metadata: HashMap::new(),
        })
    }

    async fn move_cell(&self, path: &str, index: Option<usize>, new_index: Option<usize>) -> Result<ToolOutput, ToolError> {
        let mut notebook = self.load_notebook(path)?;
        let idx = index.ok_or_else(|| ToolError {
            message: "cell_index is required".to_string(),
            code: Some("missing_index".to_string()),
        })?;
        let new_idx = new_index.ok_or_else(|| ToolError {
            message: "new_index is required (in metadata)".to_string(),
            code: Some("missing_new_index".to_string()),
        })?;

        if idx >= notebook.cells.len() || new_idx >= notebook.cells.len() {
            return Err(ToolError {
                message: "Invalid cell index".to_string(),
                code: Some("invalid_index".to_string()),
            });
        }

        let cell = notebook.cells.remove(idx);
        notebook.cells.insert(new_idx, cell);

        self.save_notebook(path, &notebook)?;

        Ok(ToolOutput {
            output_type: "json".to_string(),
            content: serde_json::to_string_pretty(&serde_json::json!({
                "message": "Cell moved successfully",
                "from_index": idx,
                "to_index": new_idx,
            })).unwrap(),
            metadata: HashMap::new(),
        })
    }

    async fn clear_outputs(&self, path: &str, index: Option<usize>) -> Result<ToolOutput, ToolError> {
        let mut notebook = self.load_notebook(path)?;

        if let Some(idx) = index {
            let cell = notebook.cells.get_mut(idx).ok_or_else(|| ToolError {
                message: format!("Cell at index {} not found", idx),
                code: Some("cell_not_found".to_string()),
            })?;
            cell.outputs = None;
            cell.execution_count = None;
        } else {
            for cell in &mut notebook.cells {
                if matches!(cell.cell_type, CellType::Code) {
                    cell.outputs = None;
                    cell.execution_count = None;
                }
            }
        }

        self.save_notebook(path, &notebook)?;

        Ok(ToolOutput {
            output_type: "json".to_string(),
            content: serde_json::to_string_pretty(&serde_json::json!({
                "message": "Outputs cleared successfully",
            })).unwrap(),
            metadata: HashMap::new(),
        })
    }

    async fn list_cells(&self, path: &str) -> Result<ToolOutput, ToolError> {
        let notebook = self.load_notebook(path)?;

        let cells: Vec<serde_json::Value> = notebook.cells.iter().enumerate().map(|(i, c)| {
            serde_json::json!({
                "index": i,
                "type": match c.cell_type {
                    CellType::Code => "code",
                    CellType::Markdown => "markdown",
                    CellType::Raw => "raw",
                },
                "source_preview": c.source.join("").chars().take(100).collect::<String>(),
                "source_lines": c.source.len(),
                "has_outputs": c.outputs.as_ref().map(|o| !o.is_empty()).unwrap_or(false),
                "execution_count": c.execution_count,
            })
        }).collect();

        Ok(ToolOutput {
            output_type: "json".to_string(),
            content: serde_json::to_string_pretty(&cells).unwrap(),
            metadata: HashMap::new(),
        })
    }
}