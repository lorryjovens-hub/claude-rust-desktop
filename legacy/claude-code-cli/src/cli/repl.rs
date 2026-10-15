//! REPL Module - Interactive Read-Eval-Print Loop (Complete)
//!
//! Beautiful REPL interface with:
//! - Streaming API support with typewriter effect
//! - MCP tool integration
//! - Basic memory management
//! - Beautiful UI matching original Claude Code

use crate::api::{ApiClient, ChatMessage, ToolDefinition};
use crate::cli::ui;
use crate::mcp::tools::ToolRegistry;
use crate::state::AppState;
use colored::Colorize;
use std::io::{self, BufRead, Write};
use std::sync::Arc;

pub struct Repl {
    state: AppState,
    conversation_history: Vec<ChatMessage>,
    tool_registry: Arc<ToolRegistry>,
}

impl Repl {
    pub fn new(state: AppState) -> Self {
        ui::init_terminal();
        let tool_registry = Arc::new(ToolRegistry::new());

        Self {
            state,
            conversation_history: Vec::new(),
            tool_registry,
        }
    }

    pub async fn start(&mut self, initial_prompt: Option<String>) -> anyhow::Result<()> {
        ui::print_welcome();

        // Register built-in tools
        self.tool_registry.register_builtin_tools().await;

        if let Some(prompt) = initial_prompt {
            self.process_input(&prompt).await?;
        }

        let stdin = io::stdin();
        let mut stdout = io::stdout();

        loop {
            ui::print_prompt();
            stdout.flush()?;

            let mut input = String::new();
            stdin.lock().read_line(&mut input)?;
            let input = input.trim();

            if input.is_empty() {
                continue;
            }

            match input {
                "exit" | "quit" | ".exit" | ":q" => {
                    println!("\n  {} {}\n",
                        "👋".yellow(),
                        "Goodbye!".truecolor(255, 140, 66).bold()
                    );
                    break;
                }
                "help" | ".help" | ":h" => ui::print_help(),
                "status" | ".status" => self.print_status(),
                "clear" | ".clear" | ":c" => ui::clear_screen(),
                "history" | ".history" => self.print_history(),
                "reset" | ".reset" => self.reset_conversation(),
                "config" | ".config" => self.print_config(),
                "tools" | ".tools" => self.list_tools().await,
                _ => self.process_input(input).await?,
            }
        }

        Ok(())
    }

    async fn list_tools(&self) {
        println!();
        println!("  {} {}",
            "🛠️".truecolor(147, 112, 219),
            "Available Tools".truecolor(147, 112, 219).bold()
        );
        println!();

        let tools = self.tool_registry.list().await;
        if tools.is_empty() {
            println!("  {} {}",
                "◦".truecolor(100, 100, 100),
                "No tools available".bright_black()
            );
        } else {
            for tool in tools {
                println!("  {} {}",
                    "▸".truecolor(147, 112, 219),
                    tool.name.bright_cyan().bold()
                );
                println!("      {}", tool.description.bright_white());
                println!();
            }
        }
        println!();
    }

    async fn process_input(&mut self, input: &str) -> anyhow::Result<()> {
        ui::print_user_message(input);

        let client = ApiClient::new(self.state.settings.clone());

        let api_key = match client.get_api_key() {
            Some(key) => key,
            None => {
                ui::print_error("API key not configured\n\nSet it with:\n  claude-code config set api_key \"your-api-key\"");
                return Ok(());
            }
        };

        self.conversation_history.push(ChatMessage::user(input));

        // Show typing indicator
        ui::print_typing_indicator();

        // Get available tools
        let tools = self.get_tool_definitions().await;

        // Try streaming first with typewriter effect
        match self.process_streaming_with_tools(&client, &api_key, tools.clone()).await {
            Ok(_) => {}
            Err(_) => {
                // Fall back to non-streaming with tools
                self.process_with_tools(&client, &api_key, tools).await?;
            }
        }

        Ok(())
    }

    async fn get_tool_definitions(&self) -> Vec<ToolDefinition> {
        let tools = self.tool_registry.list().await;
        tools
            .into_iter()
            .map(|tool| {
                ToolDefinition::new(
                    &tool.name,
                    &tool.description,
                    tool.input_schema,
                )
            })
            .collect()
    }

    async fn process_streaming_with_tools(
        &mut self,
        client: &ApiClient,
        api_key: &str,
        tools: Vec<ToolDefinition>,
    ) -> anyhow::Result<()> {
        let messages = self.conversation_history.clone();
        let base_url = client.get_base_url();
        let model = client.get_model().to_string();
        let max_tokens = self.state.settings.api.max_tokens;

        let request_body = serde_json::json!({
            "model": model,
            "messages": messages,
            "max_tokens": max_tokens,
            "stream": true,
            "temperature": 0.7,
            "tools": if tools.is_empty() { serde_json::Value::Null } else { serde_json::json!(tools) }
        });

        let http_client = reqwest::Client::new();
        let url = format!("{}/v1/chat/completions", base_url);

        let response = http_client
            .post(&url)
            .header("Authorization", format!("Bearer {}", api_key))
            .header("Content-Type", "application/json")
            .json(&request_body)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(anyhow::anyhow!("API error ({}): {}", status, body));
        }

        // Start streaming message UI
        let mut streaming_message = ui::StreamingMessage::new();
        let mut full_content = String::new();

        // Get response bytes and parse
        let bytes = response.bytes().await?;
        let text = String::from_utf8_lossy(&bytes);

        // Check if response contains tool calls
        if text.contains("tool_calls") || text.contains("toolCalls") {
            return Err(anyhow::anyhow!("Tool calls detected, switching to non-streaming"));
        }

        // Parse and display with typewriter effect
        let mut has_content = false;
        for line in text.lines() {
            if line.starts_with("data: ") {
                let data = &line[6..];
                if data == "[DONE]" {
                    continue;
                }

                if let Ok(json) = serde_json::from_str::<serde_json::Value>(data) {
                    if let Some(choices) = json.get("choices").and_then(|c| c.as_array()) {
                        if let Some(choice) = choices.first() {
                            if let Some(delta) = choice.get("delta") {
                                if let Some(content) = delta.get("content").and_then(|c| c.as_str()) {
                                    if !has_content {
                                        has_content = true;
                                    }
                                    // Typewriter effect - output character by character
                                    for c in content.chars() {
                                        streaming_message.add_chunk(&c.to_string());
                                        full_content.push(c);
                                        tokio::time::sleep(tokio::time::Duration::from_millis(5)).await;
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        // If no streaming content, try to parse as full response
        if !has_content {
            if let Ok(json) = serde_json::from_str::<serde_json::Value>(&text) {
                if let Some(choices) = json.get("choices").and_then(|c| c.as_array()) {
                    if let Some(choice) = choices.first() {
                        if let Some(msg) = choice.get("message") {
                            if let Some(content) = msg.get("content").and_then(|c| c.as_str()) {
                                streaming_message.add_chunk(content);
                                full_content = content.to_string();
                                has_content = true;
                            }
                        }
                    }
                }
            }
        }

        if has_content {
            streaming_message.finish();
            self.conversation_history.push(ChatMessage::assistant(full_content));
            Ok(())
        } else {
            Err(anyhow::anyhow!("No content received"))
        }
    }

    async fn process_with_tools(
        &mut self,
        client: &ApiClient,
        api_key: &str,
        tools: Vec<ToolDefinition>,
    ) -> anyhow::Result<()> {
        let messages = self.conversation_history.clone();
        let base_url = client.get_base_url();
        let model = client.get_model().to_string();
        let max_tokens = self.state.settings.api.max_tokens;

        let request_body = serde_json::json!({
            "model": model,
            "messages": messages,
            "max_tokens": max_tokens,
            "stream": false,
            "temperature": 0.7,
            "tools": if tools.is_empty() { serde_json::Value::Null } else { serde_json::json!(tools) }
        });

        let http_client = reqwest::Client::new();
        let url = format!("{}/v1/chat/completions", base_url);

        let response = http_client
            .post(&url)
            .header("Authorization", format!("Bearer {}", api_key))
            .header("Content-Type", "application/json")
            .json(&request_body)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            ui::print_error(&format!("API error ({}): {}", status, body));
            return Ok(());
        }

        let json: serde_json::Value = response.json().await?;

        if let Some(choices) = json.get("choices").and_then(|c| c.as_array()) {
            if let Some(choice) = choices.first() {
                let message = choice.get("message");

                if let Some(msg) = message {
                    if let Some(tool_calls) = msg.get("tool_calls").and_then(|t| t.as_array()) {
                        if !tool_calls.is_empty() {
                            self.handle_tool_calls(
                                client,
                                api_key,
                                msg,
                                tool_calls,
                                &model,
                                max_tokens,
                            ).await?;
                            return Ok(());
                        }
                    }

                    if let Some(content) = msg.get("content").and_then(|c| c.as_str()) {
                        ui::print_claude_message(content);
                        self.conversation_history.push(ChatMessage::assistant(content.to_string()));

                        if let Some(usage) = json.get("usage") {
                            if let (Some(prompt), Some(completion)) = (
                                usage.get("prompt_tokens").and_then(|t| t.as_u64()),
                                usage.get("completion_tokens").and_then(|t| t.as_u64()),
                            ) {
                                let total = prompt + completion;
                                println!("  {} {} prompt · {} generated · {} total",
                                    "◦".truecolor(100, 100, 100),
                                    prompt.to_string().truecolor(150, 150, 150),
                                    completion.to_string().truecolor(150, 150, 150),
                                    total.to_string().truecolor(180, 180, 180)
                                );
                            }
                        }
                    }
                }
            }
        }

        Ok(())
    }

    async fn handle_tool_calls(
        &mut self,
        client: &ApiClient,
        api_key: &str,
        assistant_msg: &serde_json::Value,
        tool_calls: &[serde_json::Value],
        model: &str,
        max_tokens: usize,
    ) -> anyhow::Result<()> {
        let assistant_message = ChatMessage {
            role: "assistant".to_string(),
            content: assistant_msg.get("content").and_then(|c| c.as_str()).map(|s| s.to_string()),
            tool_calls: Some(
                tool_calls
                    .iter()
                    .filter_map(|tc| {
                        let id = tc.get("id")?.as_str()?.to_string();
                        let function = tc.get("function")?;
                        let name = function.get("name")?.as_str()?.to_string();
                        let arguments = function.get("arguments")?.as_str()?.to_string();

                        Some(crate::api::ToolCall {
                            id,
                            r#type: "function".to_string(),
                            function: crate::api::ToolCallFunction {
                                name,
                                arguments,
                            },
                        })
                    })
                    .collect(),
            ),
            tool_call_id: None,
        };

        self.conversation_history.push(assistant_message);

        println!();
        print!("  {}", "●".truecolor(147, 112, 219).bold());
        println!(" {}", "Claude".truecolor(200, 150, 255).bold());
        println!();
        println!("  {} {}",
            "🛠️".yellow(),
            "Using tools...".yellow()
        );
        println!();

        for tool_call in tool_calls {
            if let (Some(id), Some(function)) = (
                tool_call.get("id").and_then(|i| i.as_str()),
                tool_call.get("function"),
            ) {
                if let (Some(name), Some(args)) = (
                    function.get("name").and_then(|n| n.as_str()),
                    function.get("arguments").and_then(|a| a.as_str()),
                ) {
                    println!("  {} {} {}",
                        "▸".truecolor(147, 112, 219),
                        "Executing".truecolor(147, 112, 219),
                        name.bright_cyan().bold()
                    );

                    let params: serde_json::Value = match serde_json::from_str(args) {
                        Ok(p) => p,
                        Err(_) => serde_json::json!({}),
                    };

                    let result = self.tool_registry.execute(name, params).await;

                    let result_str = match result {
                        Ok(r) => serde_json::to_string(&r).unwrap_or_else(|_| "{}".to_string()),
                        Err(e) => serde_json::json!({ "error": e.to_string() }).to_string(),
                    };

                    println!("  {} {}",
                        "✓".green(),
                        "Tool execution complete".green()
                    );

                    self.conversation_history.push(ChatMessage::tool(id, result_str));
                }
            }
        }

        println!();

        let messages = self.conversation_history.clone();
        let base_url = client.get_base_url();
        let tools = self.get_tool_definitions().await;

        let request_body = serde_json::json!({
            "model": model,
            "messages": messages,
            "max_tokens": max_tokens,
            "stream": false,
            "temperature": 0.7,
            "tools": if tools.is_empty() { serde_json::Value::Null } else { serde_json::json!(tools) }
        });

        let http_client = reqwest::Client::new();
        let url = format!("{}/v1/chat/completions", base_url);

        let response = http_client
            .post(&url)
            .header("Authorization", format!("Bearer {}", api_key))
            .header("Content-Type", "application/json")
            .json(&request_body)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            ui::print_error(&format!("API error ({}): {}", status, body));
            return Ok(());
        }

        let json: serde_json::Value = response.json().await?;

        if let Some(choices) = json.get("choices").and_then(|c| c.as_array()) {
            if let Some(choice) = choices.first() {
                if let Some(content) = choice.get("message")
                    .and_then(|m| m.get("content"))
                    .and_then(|c| c.as_str())
                {
                    ui::print_claude_message(content);
                    self.conversation_history.push(ChatMessage::assistant(content.to_string()));

                    if let Some(usage) = json.get("usage") {
                        if let (Some(prompt), Some(completion)) = (
                            usage.get("prompt_tokens").and_then(|t| t.as_u64()),
                            usage.get("completion_tokens").and_then(|t| t.as_u64()),
                        ) {
                            let total = prompt + completion;
                            println!("  {} {} prompt · {} generated · {} total",
                                "◦".truecolor(100, 100, 100),
                                prompt.to_string().truecolor(150, 150, 150),
                                completion.to_string().truecolor(150, 150, 150),
                                total.to_string().truecolor(180, 180, 180)
                            );
                        }
                    }
                }
            }
        }

        Ok(())
    }

    fn print_status(&self) {
        let status = ui::StatusInfo {
            model: self.state.settings.model.clone(),
            api_base: self.state.settings.api.base_url.clone(),
            max_tokens: self.state.settings.api.max_tokens.to_string(),
            timeout: self.state.settings.api.timeout,
            streaming: self.state.settings.api.streaming,
            message_count: self.conversation_history.len(),
            api_key_set: self.state.settings.api.get_api_key(&self.state.settings.model).is_some(),
        };
        ui::print_status(&status);
    }

    fn print_history(&self) {
        println!();
        if self.conversation_history.is_empty() {
            println!("  {} {}",
                "◦".truecolor(100, 100, 100),
                "No conversation history".bright_black()
            );
        } else {
            println!("  {} {}",
                "◦".truecolor(147, 112, 219),
                format!("Conversation history ({} messages)", self.conversation_history.len())
                    .truecolor(147, 112, 219).bold()
            );
            println!();

            for (i, msg) in self.conversation_history.iter().enumerate() {
                let role_label = match msg.role.as_str() {
                    "user" => "You".truecolor(255, 180, 100),
                    "assistant" => "Claude".truecolor(200, 150, 255),
                    "tool" => "Tool".truecolor(100, 200, 100),
                    _ => "Unknown".bright_black(),
                };

                let content = msg.content.as_deref().unwrap_or("");
                let preview: String = if content.is_empty() {
                    if let Some(tool_calls) = &msg.tool_calls {
                        format!("[{} tool call(s)]", tool_calls.len())
                    } else {
                        "[empty]".to_string()
                    }
                } else {
                    content.chars().take(50).collect()
                };
                let suffix = if content.len() > 50 { "..." } else { "" };

                println!("  {}. {}  {}{}",
                    (i + 1).to_string().truecolor(100, 100, 100),
                    role_label,
                    preview.bright_white(),
                    suffix.bright_black()
                );
            }
        }
        println!();
    }

    fn print_config(&self) {
        println!();
        println!("  {} {}",
            "⚙".truecolor(147, 112, 219),
            "Configuration".truecolor(147, 112, 219).bold()
        );
        println!();

        match serde_json::to_string_pretty(&self.state.settings) {
            Ok(json) => {
                for line in json.lines() {
                    println!("  {}", line.bright_white());
                }
            }
            Err(_) => {
                ui::print_error("Failed to serialize configuration");
            }
        }
        println!();
    }

    fn reset_conversation(&mut self) {
        self.conversation_history.clear();
        ui::print_success("Conversation reset");
        println!();
    }
}
