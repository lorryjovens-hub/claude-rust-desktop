//! Grep Tool - Powerful file content search
//!
//! This tool provides ripgrep-like functionality for searching file contents.

use super::{Tool, ToolOutput, ToolError};
use async_trait::async_trait;
use serde_json;
use std::path::Path;

pub struct GrepTool;

impl Default for GrepTool {
    fn default() -> Self {
        Self::new()
    }
}

impl GrepTool {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Tool for GrepTool {
    fn name(&self) -> &str {
        "grep"
    }

    fn description(&self) -> &str {
        "Search file contents using regular expressions (like ripgrep)"
    }

    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "pattern": {
                    "type": "string",
                    "description": "The regular expression pattern to search for in file contents"
                },
                "path": {
                    "type": "string",
                    "description": "File or directory to search in. Defaults to current working directory."
                },
                "glob": {
                    "type": "string",
                    "description": "Glob pattern to filter files (e.g. \"*.js\", \"*.{ts,tsx}\")"
                },
                "output_mode": {
                    "type": "string",
                    "enum": ["content", "files_with_matches", "count"],
                    "description": "Output mode: \"content\" shows matching lines, \"files_with_matches\" shows file paths, \"count\" shows match counts. Defaults to \"files_with_matches\"."
                },
                "context_before": {
                    "type": "integer",
                    "description": "Number of lines to show before each match. Requires output_mode: \"content\"."
                },
                "context_after": {
                    "type": "integer",
                    "description": "Number of lines to show after each match. Requires output_mode: \"content\"."
                },
                "context": {
                    "type": "integer",
                    "description": "Number of lines to show before and after each match. Requires output_mode: \"content\"."
                },
                "show_line_numbers": {
                    "type": "boolean",
                    "description": "Show line numbers in output. Requires output_mode: \"content\". Defaults to true."
                },
                "case_insensitive": {
                    "type": "boolean",
                    "description": "Case insensitive search"
                },
                "file_type": {
                    "type": "string",
                    "description": "File type to search (e.g., rs, js, py). More efficient than glob for standard file types."
                },
                "head_limit": {
                    "type": "integer",
                    "description": "Limit output to first N lines/entries. Defaults to 250. Pass 0 for unlimited."
                },
                "offset": {
                    "type": "integer",
                    "description": "Skip first N lines/entries before applying head_limit. Defaults to 0."
                },
                "multiline": {
                    "type": "boolean",
                    "description": "Enable multiline mode where . matches newlines. Default: false."
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
        let output_mode = input["output_mode"].as_str().unwrap_or("files_with_matches");
        let glob_pattern = input["glob"].as_str();
        let case_insensitive = input["case_insensitive"].as_bool().unwrap_or(false);
        let show_line_numbers = input["show_line_numbers"].as_bool().unwrap_or(true);
        let file_type = input["file_type"].as_str();
        let head_limit = input["head_limit"].as_u64().map(|n| n as usize);
        let offset = input["offset"].as_u64().unwrap_or(0) as usize;
        let _multiline = input["multiline"].as_bool().unwrap_or(false);

        // Context parameters
        let context = input["context"].as_u64().map(|n| n as usize);
        let context_before = input["context_before"].as_u64().map(|n| n as usize);
        let context_after = input["context_after"].as_u64().map(|n| n as usize);

        let (ctx_before, ctx_after) = match context {
            Some(ctx) => (ctx, ctx),
            None => (context_before.unwrap_or(0), context_after.unwrap_or(0)),
        };

        let search_path = Path::new(base_path);

        if !search_path.exists() {
            return Err(ToolError {
                message: format!("Path does not exist: {}", base_path),
                code: Some("path_not_found".to_string()),
            });
        }

        // Build regex with case insensitivity option
        let regex_pattern = if case_insensitive {
            format!("(?i){}", pattern)
        } else {
            pattern.to_string()
        };

        let regex = regex::Regex::new(&regex_pattern)
            .map_err(|e| ToolError {
                message: format!("Invalid regex pattern: {}", e),
                code: Some("invalid_pattern".to_string()),
            })?;

        // File type extensions mapping
        let type_extensions = match file_type {
            Some("rs") => Some(vec!["rs"]),
            Some("js") => Some(vec!["js", "jsx"]),
            Some("ts") => Some(vec!["ts", "tsx"]),
            Some("py") => Some(vec!["py"]),
            Some("go") => Some(vec!["go"]),
            Some("java") => Some(vec!["java"]),
            Some("cpp") => Some(vec!["cpp", "cc", "cxx", "hpp", "hh", "hxx", "h"]),
            Some("c") => Some(vec!["c", "h"]),
            Some("md") => Some(vec!["md", "markdown"]),
            Some("html") => Some(vec!["html", "htm"]),
            Some("css") => Some(vec!["css", "scss", "sass", "less"]),
            Some("json") => Some(vec!["json"]),
            Some("toml") => Some(vec!["toml"]),
            Some("yaml") => Some(vec!["yaml", "yml"]),
            Some("xml") => Some(vec!["xml"]),
            Some("sh") => Some(vec!["sh", "bash", "zsh"]),
            Some("rb") => Some(vec!["rb"]),
            Some("php") => Some(vec!["php"]),
            Some("swift") => Some(vec!["swift"]),
            Some("kt") => Some(vec!["kt", "kts"]),
            Some("scala") => Some(vec!["scala"]),
            Some("sql") => Some(vec!["sql"]),
            _ => None,
        };

        let mut results = Vec::new();
        let mut files_with_matches = std::collections::HashSet::new();
        let mut match_counts = std::collections::HashMap::new();

        // Walk the directory
        for entry in walkdir::WalkDir::new(search_path)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            let entry_path = entry.path();

            if entry_path.is_file() {
                // Check if we should skip this file based on glob or type
                let mut skip = false;

                // Skip VCS directories
                if let Some(components) = entry_path.components().next_back() {
                    let name = components.as_os_str().to_string_lossy();
                    if name == ".git" || name == ".svn" || name == ".hg" || name == ".bzr" {
                        skip = true;
                    }
                }

                // Check file extension for type filtering
                if !skip && type_extensions.is_some() {
                    if let Some(ext) = entry_path.extension() {
                        let ext_str = ext.to_string_lossy().to_lowercase();
                        if !type_extensions.as_ref().unwrap().contains(&ext_str.as_str()) {
                            skip = true;
                        }
                    } else {
                        skip = true;
                    }
                }

                // Check glob pattern
                if !skip && glob_pattern.is_some() {
                    if let Some(file_name) = entry_path.file_name() {
                        let file_name_str = file_name.to_string_lossy();
                        if let Ok(glob) = glob::Pattern::new(glob_pattern.unwrap()) {
                            if !glob.matches(&file_name_str) {
                                skip = true;
                            }
                        }
                    }
                }

                if skip {
                    continue;
                }

                // Try to read and search
                if let Ok(content) = std::fs::read_to_string(entry_path) {
                    let mut file_has_match = false;
                    let mut file_match_count = 0;
                    let lines: Vec<&str> = content.lines().collect();

                    for (line_num, line) in lines.iter().enumerate() {
                        if regex.is_match(line) {
                            file_has_match = true;
                            file_match_count += 1;
                            files_with_matches.insert(entry_path.to_path_buf());

                            if output_mode == "content" {
                                // Add context before
                                let start_line = if ctx_before > 0 {
                                    line_num.saturating_sub(ctx_before)
                                } else {
                                    line_num
                                };

                                // Add context after
                                let end_line = std::cmp::min(line_num + ctx_after + 1, lines.len());

                                for ctx_line_num in start_line..end_line {
                                    let is_match = ctx_line_num == line_num;
                                    let prefix = if show_line_numbers {
                                        format!("{}:{}", entry_path.display(), ctx_line_num + 1)
                                    } else {
                                        format!("{}", entry_path.display())
                                    };
                                    let marker = if is_match { ">" } else { "|" };
                                    results.push(format!("{} {} {}", prefix, marker, lines[ctx_line_num]));
                                }
                                // Add a separator between match groups
                                if ctx_before > 0 || ctx_after > 0 {
                                    results.push("--".to_string());
                                }
                            }
                        }
                    }

                    if file_has_match {
                        *match_counts.entry(entry_path.to_path_buf()).or_insert(0) += file_match_count;
                    }
                }
            }
        }

        // Apply offset and limit
        let effective_limit = head_limit.unwrap_or(250);
        let mut applied_limit = None;

        let final_results = if effective_limit == 0 {
            // Unlimited
            results.into_iter().skip(offset).collect::<Vec<_>>()
        } else {
            let skipped = results.into_iter().skip(offset).collect::<Vec<_>>();
            if skipped.len() > effective_limit {
                applied_limit = Some(effective_limit);
                skipped.into_iter().take(effective_limit).collect()
            } else {
                skipped
            }
        };

        // Prepare output based on mode
        let (content, num_files) = match output_mode {
            "content" => {
                // Remove trailing separator
                let mut output = final_results;
                if let Some(last) = output.last() {
                    if last == "--" {
                        output.pop();
                    }
                }
                let content = if output.is_empty() {
                    "No matches found".to_string()
                } else {
                    output.join("\n")
                };
                (content, files_with_matches.len())
            }
            "count" => {
                let mut count_lines = Vec::new();
                let mut total_matches = 0;
                for (path, count) in match_counts {
                    let display_path = if let Ok(rel_path) = path.strip_prefix(".") {
                        rel_path.to_string_lossy().to_string()
                    } else {
                        path.to_string_lossy().to_string()
                    };
                    count_lines.push(format!("{}:{}", display_path, count));
                    total_matches += count;
                }
                count_lines.sort();
                let content = if count_lines.is_empty() {
                    "No matches found".to_string()
                } else {
                    let mut output = count_lines.join("\n");
                    output.push_str(&format!("\n\nFound {} total occurrences across {} files.", total_matches, count_lines.len()));
                    output
                };
                (content, count_lines.len())
            }
            "files_with_matches" | _ => {
                let mut files: Vec<String> = files_with_matches
                    .into_iter()
                    .map(|p| {
                        if let Ok(rel_path) = p.strip_prefix(".") {
                            rel_path.to_string_lossy().to_string()
                        } else {
                            p.to_string_lossy().to_string()
                        }
                    })
                    .collect();
                files.sort();

                // Apply offset and limit for files too
                let files = if effective_limit == 0 {
                    files.into_iter().skip(offset).collect()
                } else {
                    let skipped: Vec<_> = files.into_iter().skip(offset).collect();
                    if skipped.len() > effective_limit {
                        applied_limit = Some(effective_limit);
                        skipped.into_iter().take(effective_limit).collect()
                    } else {
                        skipped
                    }
                };

                let content = if files.is_empty() {
                    "No files found".to_string()
                } else {
                    let mut output = format!("Found {} file{}\n", files.len(), if files.len() == 1 { "" } else { "s" });
                    output.push_str(&files.join("\n"));
                    output
                };
                (content, files.len())
            }
        };

        // Add limit info if applicable
        let final_content = if let Some(limit) = applied_limit {
            let limit_info = if offset > 0 {
                format!("\n\n[Showing results with pagination = limit: {}, offset: {}]", limit, offset)
            } else {
                format!("\n\n[Showing results with pagination = limit: {}]", limit)
            };
            format!("{}{}", content, limit_info)
        } else {
            content
        };

        Ok(ToolOutput {
            output_type: "text".to_string(),
            content: final_content,
            metadata: {
                let mut meta = std::collections::HashMap::new();
                meta.insert("num_files".to_string(), serde_json::json!(num_files));
                meta.insert("mode".to_string(), serde_json::json!(output_mode));
                if let Some(limit) = applied_limit {
                    meta.insert("applied_limit".to_string(), serde_json::json!(limit));
                }
                if offset > 0 {
                    meta.insert("applied_offset".to_string(), serde_json::json!(offset));
                }
                meta
            },
        })
    }
}
