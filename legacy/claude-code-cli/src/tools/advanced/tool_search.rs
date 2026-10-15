//! ToolSearch Tool - Search and discover available tools
//!
//! Provides tool search capabilities with filtering and recommendations.

use crate::tools::{Tool, ToolOutput, ToolError};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolInfo {
    pub name: String,
    pub description: String,
    pub category: String,
    pub tags: Vec<String>,
    pub usage_count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub tool: ToolInfo,
    pub relevance_score: f32,
    pub match_reason: String,
}

pub struct ToolSearchTool {
    tool_index: Arc<RwLock<HashMap<String, ToolInfo>>>,
}

impl Default for ToolSearchTool {
    fn default() -> Self {
        Self::new()
    }
}

impl ToolSearchTool {
    pub fn new() -> Self {
        let mut tool_index = HashMap::new();

        let default_tools = vec![
            ("file_read", "Read file contents", "file", vec!["read", "file", "view"]),
            ("file_edit", "Edit file contents", "file", vec!["edit", "modify", "change"]),
            ("file_write", "Write content to files", "file", vec!["write", "create", "new"]),
            ("execute_command", "Execute shell commands", "system", vec!["execute", "run", "command", "shell"]),
            ("search", "Search within files", "search", vec!["search", "find", "grep"]),
            ("list_files", "List directory contents", "file", vec!["list", "ls", "dir"]),
            ("git_operations", "Perform git operations", "vcs", vec!["git", "version", "control"]),
            ("task_management", "Manage tasks", "productivity", vec!["task", "todo", "manage"]),
            ("note_edit", "Edit notes", "productivity", vec!["note", "edit"]),
            ("smart_edit", "Smart editing with AI", "ai", vec!["smart", "edit", "ai"]),
            ("glob_tool", "Glob pattern matching", "search", vec!["glob", "pattern", "match"]),
            ("grep_tool", "Grep search tool", "search", vec!["grep", "search", "find"]),
            ("worker", "Agent worker management", "agent", vec!["worker", "agent", "boot"]),
            ("team", "Multi-agent team management", "agent", vec!["team", "multi", "agent"]),
            ("cron", "Scheduled task management", "system", vec!["cron", "schedule", "task"]),
            ("lsp", "Language Server Protocol", "developer", vec!["lsp", "language", "server"]),
            ("mcp", "MCP protocol bridge", "protocol", vec!["mcp", "protocol", "bridge"]),
            ("web_fetch", "Fetch web content", "web", vec!["web", "fetch", "http"]),
            ("web_search", "Search the web", "web", vec!["web", "search", "internet"]),
            ("ask_question", "Ask user questions", "interaction", vec!["ask", "question", "user"]),
            ("permission", "Permission checking", "security", vec!["permission", "security"]),
            ("bash_security", "Bash security validation", "security", vec!["bash", "security", "validate"]),
            ("sandbox", "Sandboxed execution", "security", vec!["sandbox", "security", "safe"]),
            ("agent", "Agent lifecycle management", "agent", vec!["agent", "fork", "run", "resume"]),
            ("worktree", "Git worktree management", "vcs", vec!["git", "worktree", "branch"]),
            ("plan_mode", "Strategic planning", "productivity", vec!["plan", "planning", "strategy"]),
            ("brief", "Generate summaries", "productivity", vec!["brief", "summary", "summarize"]),
            ("todo_write", "Todo list management", "productivity", vec!["todo", "list", "task"]),
            ("tool_search", "Search available tools", "system", vec!["tool", "search", "find"]),
        ];

        for (name, desc, category, tags) in default_tools {
            tool_index.insert(name.to_string(), ToolInfo {
                name: name.to_string(),
                description: desc.to_string(),
                category: category.to_string(),
                tags: tags.into_iter().map(String::from).collect(),
                usage_count: 0,
            });
        }

        Self {
            tool_index: Arc::new(RwLock::new(tool_index)),
        }
    }

    fn calculate_relevance(&self, query: &str, tool: &ToolInfo) -> (f32, String) {
        let query_lower = query.to_lowercase();
        let query_terms: Vec<&str> = query_lower.split_whitespace().collect();

        let mut score = 0.0f32;
        let mut reasons = Vec::new();

        for term in &query_terms {
            if tool.name.to_lowercase().contains(term) {
                score += 3.0;
                reasons.push(format!("name matches '{}'", term));
            }
            if tool.description.to_lowercase().contains(term) {
                score += 2.0;
                reasons.push(format!("description matches '{}'", term));
            }
            if tool.category.to_lowercase().contains(term) {
                score += 1.5;
                reasons.push(format!("category matches '{}'", term));
            }
            for tag in &tool.tags {
                if tag.to_lowercase().contains(term) {
                    score += 1.0;
                    reasons.push(format!("tag matches '{}'", term));
                }
            }
        }

        (score, reasons.join(", "))
    }

    async fn handle_search(&self, query: &str, category: Option<&str>, limit: usize) -> Result<Vec<SearchResult>, ToolError> {
        let index = self.tool_index.read().await;

        let mut results: Vec<SearchResult> = index.values()
            .filter(|tool| {
                if let Some(cat) = category {
                    tool.category == cat
                } else {
                    true
                }
            })
            .filter_map(|tool| {
                let (score, reason) = self.calculate_relevance(query, tool);
                if score > 0.0 {
                    Some(SearchResult {
                        tool: tool.clone(),
                        relevance_score: score,
                        match_reason: reason,
                    })
                } else {
                    None
                }
            })
            .collect();

        results.sort_by(|a, b| b.relevance_score.partial_cmp(&a.relevance_score).unwrap_or(std::cmp::Ordering::Equal));

        results.truncate(limit);

        Ok(results)
    }

    async fn handle_list_categories(&self) -> Result<Vec<String>, ToolError> {
        let index = self.tool_index.read().await;
        let mut categories: Vec<String> = index.values()
            .map(|t| t.category.clone())
            .collect();
        categories.sort();
        categories.dedup();
        Ok(categories)
    }

    async fn handle_list_all(&self) -> Result<Vec<ToolInfo>, ToolError> {
        let index = self.tool_index.read().await;
        let list: Vec<ToolInfo> = index.values().cloned().collect();
        Ok(list)
    }

    async fn handle_get(&self, name: &str) -> Result<ToolInfo, ToolError> {
        let index = self.tool_index.read().await;
        index.get(name)
            .cloned()
            .ok_or_else(|| ToolError {
                message: format!("Tool not found: {}", name),
                code: Some("tool_not_found".to_string()),
            })
    }

    async fn handle_register(&self, name: &str, description: &str, category: &str, tags: Vec<String>) -> Result<ToolInfo, ToolError> {
        let info = ToolInfo {
            name: name.to_string(),
            description: description.to_string(),
            category: category.to_string(),
            tags,
            usage_count: 0,
        };

        let mut index = self.tool_index.write().await;
        index.insert(name.to_string(), info.clone());

        Ok(info)
    }
}

#[async_trait]
impl Tool for ToolSearchTool {
    fn name(&self) -> &str {
        "tool_search"
    }

    fn description(&self) -> &str {
        "Search and discover available tools with filtering"
    }

    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "operation": {
                    "type": "string",
                    "enum": ["search", "list_categories", "list_all", "get", "register"]
                },
                "query": { "type": "string", "description": "Search query" },
                "category": { "type": "string", "description": "Filter by category" },
                "limit": {
                    "type": "number",
                    "description": "Maximum results to return"
                },
                "name": { "type": "string", "description": "Tool name" },
                "description": { "type": "string", "description": "Tool description" },
                "tags": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Tool tags"
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
            "search" => {
                let query = input["query"].as_str()
                    .ok_or_else(|| ToolError {
                        message: "query is required".to_string(),
                        code: Some("missing_query".to_string()),
                    })?;
                let category = input["category"].as_str();
                let limit = input["limit"].as_u64().unwrap_or(10) as usize;

                let results = self.handle_search(query, category, limit).await?;
                serde_json::json!({
                    "success": true,
                    "results": results,
                    "count": results.len()
                }).to_string()
            }
            "list_categories" => {
                let categories = self.handle_list_categories().await?;
                serde_json::json!({
                    "success": true,
                    "categories": categories
                }).to_string()
            }
            "list_all" => {
                let tools = self.handle_list_all().await?;
                serde_json::json!({
                    "success": true,
                    "tools": tools,
                    "count": tools.len()
                }).to_string()
            }
            "get" => {
                let name = input["name"].as_str()
                    .ok_or_else(|| ToolError {
                        message: "name is required".to_string(),
                        code: Some("missing_name".to_string()),
                    })?;
                let tool = self.handle_get(name).await?;
                serde_json::json!({
                    "success": true,
                    "tool": tool
                }).to_string()
            }
            "register" => {
                let name = input["name"].as_str()
                    .ok_or_else(|| ToolError {
                        message: "name is required".to_string(),
                        code: Some("missing_name".to_string()),
                    })?;
                let description = input["description"].as_str()
                    .ok_or_else(|| ToolError {
                        message: "description is required".to_string(),
                        code: Some("missing_description".to_string()),
                    })?;
                let category = input["category"].as_str().unwrap_or("custom");
                let tags: Vec<String> = input["tags"]
                    .as_array()
                    .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
                    .unwrap_or_default();

                let tool = self.handle_register(name, description, category, tags).await?;
                serde_json::json!({
                    "success": true,
                    "tool": tool
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