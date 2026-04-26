use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;
use std::process::Stdio;
use tokio::process::Command;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
}

pub fn get_tool_definitions() -> Vec<ToolDefinition> {
    vec![
        ToolDefinition {
            name: "Read".to_string(),
            description: "Read a file from the local filesystem. Returns content with line numbers.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "file_path": { "type": "string", "description": "The absolute or relative path to the file to read" },
                    "offset": { "type": "number", "description": "Line number to start reading from (1-based)" },
                    "limit": { "type": "number", "description": "Max number of lines to read" }
                },
                "required": ["file_path"]
            }),
        },
        ToolDefinition {
            name: "Write".to_string(),
            description: "Write content to a file. Creates the file and parent directories if needed.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "file_path": { "type": "string", "description": "The path to the file" },
                    "content": { "type": "string", "description": "The full content to write" }
                },
                "required": ["file_path", "content"]
            }),
        },
        ToolDefinition {
            name: "Edit".to_string(),
            description: "Make an exact string replacement in a file.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "file_path": { "type": "string", "description": "The path to the file" },
                    "old_string": { "type": "string", "description": "The exact text to find" },
                    "new_string": { "type": "string", "description": "The replacement text" },
                    "replace_all": { "type": "boolean", "description": "If true, replace ALL occurrences" }
                },
                "required": ["file_path", "old_string", "new_string"]
            }),
        },
        ToolDefinition {
            name: "Bash".to_string(),
            description: "Execute a shell command and return stdout/stderr.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "command": { "type": "string", "description": "The shell command to execute" },
                    "timeout": { "type": "number", "description": "Timeout in seconds (default: 60)" }
                },
                "required": ["command"]
            }),
        },
        ToolDefinition {
            name: "Glob".to_string(),
            description: "Find files matching a glob pattern.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "pattern": { "type": "string", "description": "Glob pattern" },
                    "path": { "type": "string", "description": "Base directory to search in" }
                },
                "required": ["pattern"]
            }),
        },
        ToolDefinition {
            name: "Grep".to_string(),
            description: "Search file contents using regex.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "pattern": { "type": "string", "description": "Regex pattern to search for" },
                    "path": { "type": "string", "description": "File or directory to search in" },
                    "include": { "type": "string", "description": "Glob to filter files" }
                },
                "required": ["pattern"]
            }),
        },
        ToolDefinition {
            name: "ListDir".to_string(),
            description: "List the contents of a directory.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Directory path to list" }
                },
                "required": ["path"]
            }),
        },
        ToolDefinition {
            name: "WebFetch".to_string(),
            description: "Fetch content from a URL.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "url": { "type": "string", "description": "The URL to fetch" },
                    "headers": { "type": "object", "description": "Optional HTTP headers" }
                },
                "required": ["url"]
            }),
        },
        ToolDefinition {
            name: "WebSearch".to_string(),
            description: "Search the web for information.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "query": { "type": "string", "description": "The search query" }
                },
                "required": ["query"]
            }),
        },
        ToolDefinition {
            name: "MultiEdit".to_string(),
            description: "Make multiple string replacements in a file at once.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "file_path": { "type": "string", "description": "The path to the file" },
                    "edits": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "old_string": { "type": "string" },
                                "new_string": { "type": "string" }
                            }
                        }
                    }
                },
                "required": ["file_path", "edits"]
            }),
        },
    ]
}

pub fn execute_tool(name: &str, input: serde_json::Value, cwd: &str) -> Result<serde_json::Value> {
    match name {
        "Read" => tool_read(input, cwd),
        "Write" => tool_write(input, cwd),
        "Edit" => tool_edit(input, cwd),
        "Bash" => tool_bash(input, cwd),
        "Glob" => tool_glob(input, cwd),
        "Grep" => tool_grep(input, cwd),
        "ListDir" => tool_list_dir(input, cwd),
        "WebFetch" => tool_web_fetch_blocking(input),
        "WebSearch" => tool_web_search_blocking(input),
        "MultiEdit" => tool_multi_edit(input, cwd),
        _ => Ok(serde_json::json!({ "error": format!("Unknown tool: {}", name) })),
    }
}

pub async fn execute_tool_async(name: &str, input: serde_json::Value, cwd: &str) -> Result<serde_json::Value> {
    match name {
        "Read" => tool_read(input, cwd),
        "Write" => tool_write(input, cwd),
        "Edit" => tool_edit(input, cwd),
        "Bash" => tool_bash_async(input, cwd).await,
        "Glob" => tool_glob(input, cwd),
        "Grep" => tool_grep(input, cwd),
        "ListDir" => tool_list_dir(input, cwd),
        "WebFetch" => tool_web_fetch_async(input).await,
        "WebSearch" => tool_web_search_async(input).await,
        "MultiEdit" => tool_multi_edit(input, cwd),
        _ => Ok(serde_json::json!({ "error": format!("Unknown tool: {}", name) })),
    }
}

fn resolve_path(file_path: &str, cwd: &str) -> String {
    if Path::new(file_path).is_absolute() {
        file_path.to_string()
    } else {
        Path::new(cwd).join(file_path).to_string_lossy().to_string()
    }
}

fn tool_read(input: serde_json::Value, cwd: &str) -> Result<serde_json::Value> {
    let file_path = input["file_path"].as_str().ok_or_else(|| anyhow!("file_path required"))?;
    let path = resolve_path(file_path, cwd);

    if !Path::new(&path).exists() {
        return Ok(serde_json::json!({ "content": format!("File not found: {}", path), "is_error": true }));
    }

    let content = fs::read_to_string(&path)?;
    let offset = input["offset"].as_u64().unwrap_or(1) as usize;
    let limit = input["limit"].as_u64().unwrap_or(2000) as usize;

    let lines: Vec<&str> = content.lines().collect();
    let total_lines = lines.len();
    let selected: Vec<String> = lines
        .iter()
        .skip(offset.saturating_sub(1))
        .take(limit)
        .enumerate()
        .map(|(i, line)| format!("{:>6}\t{}", i + offset, line))
        .collect();

    Ok(serde_json::json!({
        "content": selected.join("\n"),
        "lines": total_lines,
        "truncated": total_lines > limit
    }))
}

fn tool_write(input: serde_json::Value, cwd: &str) -> Result<serde_json::Value> {
    let file_path = input["file_path"].as_str().ok_or_else(|| anyhow!("file_path required"))?;
    let content = input["content"].as_str().ok_or_else(|| anyhow!("content required"))?;
    let path = resolve_path(file_path, cwd);

    if let Some(parent) = Path::new(&path).parent() {
        fs::create_dir_all(parent)?;
    }

    fs::write(&path, content)?;

    Ok(serde_json::json!({
        "success": true,
        "content": format!("Successfully wrote to {}", path),
        "bytes_written": content.len()
    }))
}

fn tool_edit(input: serde_json::Value, cwd: &str) -> Result<serde_json::Value> {
    let file_path = input["file_path"].as_str().ok_or_else(|| anyhow!("file_path required"))?;
    let old_string = input["old_string"].as_str().ok_or_else(|| anyhow!("old_string required"))?;
    let new_string = input["new_string"].as_str().ok_or_else(|| anyhow!("new_string required"))?;
    let replace_all = input["replace_all"].as_bool().unwrap_or(false);
    let path = resolve_path(file_path, cwd);

    let content = fs::read_to_string(&path)?;

    let (new_content, replacements) = if replace_all {
        let count = content.matches(old_string).count();
        (content.replace(old_string, new_string), count)
    } else {
        let count = content.matches(old_string).count();
        if count == 0 {
            return Ok(serde_json::json!({
                "success": false,
                "error": "old_string not found in file"
            }));
        }
        if count > 1 {
            return Ok(serde_json::json!({
                "success": false,
                "error": format!("old_string found {} times, use replace_all=true", count)
            }));
        }
        (content.replacen(old_string, new_string, 1), 1)
    };

    fs::write(&path, new_content)?;

    Ok(serde_json::json!({
        "success": true,
        "content": format!("Successfully replaced {} occurrence(s) in {}", replacements, path),
        "replacements": replacements
    }))
}

fn tool_multi_edit(input: serde_json::Value, cwd: &str) -> Result<serde_json::Value> {
    let file_path = input["file_path"].as_str().ok_or_else(|| anyhow!("file_path required"))?;
    let edits = input["edits"].as_array().ok_or_else(|| anyhow!("edits array required"))?;
    let path = resolve_path(file_path, cwd);

    let content = fs::read_to_string(&path)?;
    let mut new_content = content;
    let mut total_replacements = 0;
    let mut failed_edits: Vec<String> = Vec::new();

    for edit in edits {
        let old_string = edit.get("old_string").and_then(|s| s.as_str());
        let new_string = edit.get("new_string").and_then(|s| s.as_str());

        if let (Some(old), Some(new)) = (old_string, new_string) {
            if new_content.contains(old) {
                new_content = new_content.replace(old, new);
                total_replacements += 1;
            } else {
                failed_edits.push(old.to_string());
            }
        }
    }

    if !failed_edits.is_empty() {
        return Ok(serde_json::json!({
            "success": false,
            "error": format!("Some edits failed: {:?}", failed_edits),
            "replacements": total_replacements
        }));
    }

    fs::write(&path, new_content)?;

    Ok(serde_json::json!({
        "success": true,
        "content": format!("Successfully applied {} edits to {}", total_replacements, path),
        "replacements": total_replacements
    }))
}

async fn tool_bash_async(input: serde_json::Value, cwd: &str) -> Result<serde_json::Value> {
    let command = input["command"].as_str().ok_or_else(|| anyhow!("command required"))?;
    let timeout_secs = input["timeout"].as_u64().unwrap_or(60);

    let (shell, flag) = if cfg!(target_os = "windows") {
        let git_bash = find_git_bash();
        if let Some(git_bash_path) = git_bash {
            (git_bash_path, "-c".to_string())
        } else {
            ("cmd".to_string(), "/C".to_string())
        }
    } else {
        ("sh".to_string(), "-c".to_string())
    };

    let mut cmd = if cfg!(target_os = "windows") && find_git_bash().is_some() {
        Command::new(&shell)
    } else {
        Command::new(&shell)
    };

    if shell == "cmd" {
        cmd.arg(flag);
    } else {
        cmd.arg(&flag);
    }
    cmd.arg(command)
        .current_dir(cwd)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let output = tokio::time::timeout(
        std::time::Duration::from_secs(timeout_secs),
        cmd.output()
    ).await;

    match output {
        Ok(Ok(output)) => {
            let stdout = String::from_utf8_lossy(&output.stdout).to_string();
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            let exit_code = output.status.code().unwrap_or(-1);

            Ok(serde_json::json!({
                "stdout": stdout,
                "stderr": stderr,
                "exit_code": exit_code,
                "success": output.status.success()
            }))
        }
        Ok(Err(e)) => Ok(serde_json::json!({
            "error": format!("Command failed: {}", e),
            "is_error": true
        })),
        Err(_) => Ok(serde_json::json!({
            "error": format!("Command timed out after {} seconds", timeout_secs),
            "is_error": true,
            "timed_out": true
        })),
    }
}

fn tool_bash(input: serde_json::Value, cwd: &str) -> Result<serde_json::Value> {
    let command = input["command"].as_str().ok_or_else(|| anyhow!("command required"))?;
    let timeout_secs = input["timeout"].as_u64().unwrap_or(60);

    let (shell, flag) = if cfg!(target_os = "windows") {
        let git_bash = find_git_bash();
        if let Some(git_bash_path) = git_bash {
            (git_bash_path, "-c".to_string())
        } else {
            ("cmd".to_string(), "/C".to_string())
        }
    } else {
        ("sh".to_string(), "-c".to_string())
    };

    let mut cmd = if cfg!(target_os = "windows") && find_git_bash().is_some() {
        std::process::Command::new(&shell)
    } else {
        std::process::Command::new(&shell)
    };

    if shell == "cmd" {
        cmd.arg(flag);
    } else {
        cmd.arg(&flag);
    }
    cmd.arg(command)
        .current_dir(cwd)
        .output();

    match cmd.output() {
        Ok(output) => {
            let stdout = String::from_utf8_lossy(&output.stdout).to_string();
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            let exit_code = output.status.code().unwrap_or(-1);

            Ok(serde_json::json!({
                "stdout": stdout,
                "stderr": stderr,
                "exit_code": exit_code,
                "success": output.status.success()
            }))
        }
        Err(e) => Ok(serde_json::json!({
            "error": format!("Command failed: {}", e),
            "is_error": true
        })),
    }
}

fn find_git_bash() -> Option<String> {
    let candidates: Vec<String> = if cfg!(target_os = "windows") {
        vec![
            r"C:\Program Files\Git\bin\bash.exe".to_string(),
            r"C:\Program Files (x86)\Git\bin\bash.exe".to_string(),
        ]
    } else {
        vec!["/usr/bin/bash".to_string(), "/bin/bash".to_string()]
    };

    for path in &candidates {
        if std::path::Path::new(path).exists() {
            return Some(path.clone());
        }
    }

    None
}

fn tool_glob(input: serde_json::Value, cwd: &str) -> Result<serde_json::Value> {
    let pattern = input["pattern"].as_str().ok_or_else(|| anyhow!("pattern required"))?;
    let base_path = input["path"].as_str().unwrap_or(cwd);

    let mut matches: Vec<String> = Vec::new();
    for entry in walkdir::WalkDir::new(base_path).into_iter().filter_map(|e| e.ok()) {
        let path = entry.path();
        if let Some(path_str) = path.to_str() {
            if let Ok(glob_pattern) = glob::Pattern::new(pattern) {
                if glob_pattern.matches(path_str) {
                    matches.push(path_str.to_string());
                }
            }
        }
    }

    Ok(serde_json::json!({
        "files": matches,
        "count": matches.len()
    }))
}

fn tool_grep(input: serde_json::Value, cwd: &str) -> Result<serde_json::Value> {
    let pattern = input["pattern"].as_str().ok_or_else(|| anyhow!("pattern required"))?;
    let search_path = input["path"].as_str().unwrap_or(cwd);

    let re = regex::Regex::new(pattern)?;
    let mut results: Vec<serde_json::Value> = Vec::new();

    for entry in walkdir::WalkDir::new(search_path).into_iter().filter_map(|e| e.ok()) {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }

        if let Ok(content) = fs::read_to_string(path) {
            for (line_num, line) in content.lines().enumerate() {
                if re.is_match(line) {
                    results.push(serde_json::json!({
                        "file": path.to_string_lossy(),
                        "line": line_num + 1,
                        "content": line
                    }));
                }
            }
        }
    }

    Ok(serde_json::json!({
        "matches": results,
        "count": results.len()
    }))
}

fn tool_list_dir(input: serde_json::Value, _cwd: &str) -> Result<serde_json::Value> {
    let dir_path = input["path"].as_str().ok_or_else(|| anyhow!("path required"))?;

    let mut entries: Vec<serde_json::Value> = Vec::new();
    for entry in fs::read_dir(dir_path)? {
        let entry = entry?;
        let metadata = entry.metadata()?;
        entries.push(serde_json::json!({
            "name": entry.file_name().to_string_lossy(),
            "is_dir": metadata.is_dir(),
            "size": metadata.len()
        }));
    }

    Ok(serde_json::json!({ "entries": entries }))
}

fn tool_web_fetch_blocking(input: serde_json::Value) -> Result<serde_json::Value> {
    let url = input["url"].as_str().ok_or_else(|| anyhow!("url required"))?;

    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()?;

    let mut request = client.get(url);
    if let Some(headers) = input.get("headers").and_then(|h| h.as_object()) {
        for (key, value) in headers {
            if let Some(value_str) = value.as_str() {
                request = request.header(key.as_str(), value_str);
            }
        }
    }

    let response = request.send()?;

    if !response.status().is_success() {
        return Ok(serde_json::json!({
            "error": format!("HTTP error: {}", response.status()),
            "status": response.status().as_u16()
        }));
    }

    let content_type = response.headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("text/plain")
        .to_string();

    let body = response.text()?;

    Ok(serde_json::json!({
        "content": body,
        "content_type": content_type,
        "url": url
    }))
}

async fn tool_web_fetch_async(input: serde_json::Value) -> Result<serde_json::Value> {
    let url = input["url"].as_str().ok_or_else(|| anyhow!("url required"))?;

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()?;

    let mut request = client.get(url);
    if let Some(headers) = input.get("headers").and_then(|h| h.as_object()) {
        for (key, value) in headers {
            if let Some(value_str) = value.as_str() {
                request = request.header(key.as_str(), value_str);
            }
        }
    }

    let response = request.send().await?;

    if !response.status().is_success() {
        return Ok(serde_json::json!({
            "error": format!("HTTP error: {}", response.status()),
            "status": response.status().as_u16()
        }));
    }

    let content_type = response.headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("text/plain")
        .to_string();

    let body = response.text().await?;

    Ok(serde_json::json!({
        "content": body,
        "content_type": content_type,
        "url": url
    }))
}

fn tool_web_search_blocking(input: serde_json::Value) -> Result<serde_json::Value> {
    let query = input["query"].as_str().ok_or_else(|| anyhow!("query required"))?;

    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()?;

    let search_url = format!(
        "https://api.duckduckgo.com/?q={}&format=json&no_html=1",
        urlencoding::encode(query)
    );

    let response = client.get(&search_url).send()?;

    if !response.status().is_success() {
        return Ok(serde_json::json!({
            "error": format!("Search failed: {}", response.status()),
            "results": []
        }));
    }

    #[derive(Deserialize)]
    struct DuckDuckGoResponse {
        RelatedTopics: Vec<RelatedTopic>,
    }

    #[derive(Deserialize)]
    struct RelatedTopic {
        Text: Option<String>,
        URL: Option<String>,
    }

    match response.json::<DuckDuckGoResponse>() {
        Ok(data) => {
            let results: Vec<serde_json::Value> = data.RelatedTopics
                .iter()
                .filter(|t| t.Text.is_some())
                .take(10)
                .map(|t| serde_json::json!({
                    "title": t.Text.as_deref().unwrap_or(""),
                    "url": t.URL.as_deref().unwrap_or("")
                }))
                .collect();

            Ok(serde_json::json!({
                "results": results,
                "query": query
            }))
        }
        Err(_) => Ok(serde_json::json!({
            "error": "Failed to parse search response",
            "results": []
        })),
    }
}

async fn tool_web_search_async(input: serde_json::Value) -> Result<serde_json::Value> {
    let query = input["query"].as_str().ok_or_else(|| anyhow!("query required"))?;

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()?;

    let search_url = format!(
        "https://api.duckduckgo.com/?q={}&format=json&no_html=1",
        urlencoding::encode(query)
    );

    let response = client.get(&search_url).send().await?;

    if !response.status().is_success() {
        return Ok(serde_json::json!({
            "error": format!("Search failed: {}", response.status()),
            "results": []
        }));
    }

    #[derive(Deserialize)]
    struct DuckDuckGoResponse {
        RelatedTopics: Vec<RelatedTopic>,
    }

    #[derive(Deserialize)]
    struct RelatedTopic {
        Text: Option<String>,
        URL: Option<String>,
    }

    match response.json::<DuckDuckGoResponse>().await {
        Ok(data) => {
            let results: Vec<serde_json::Value> = data.RelatedTopics
                .iter()
                .filter(|t| t.Text.is_some())
                .take(10)
                .map(|t| serde_json::json!({
                    "title": t.Text.as_deref().unwrap_or(""),
                    "url": t.URL.as_deref().unwrap_or("")
                }))
                .collect();

            Ok(serde_json::json!({
                "results": results,
                "query": query
            }))
        }
        Err(_) => Ok(serde_json::json!({
            "error": "Failed to parse search response",
            "results": []
        })),
    }
}
