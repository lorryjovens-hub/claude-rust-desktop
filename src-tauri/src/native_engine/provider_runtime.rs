use super::{openai_client::OpenAIMessage, provider_manager::ResolvedProvider};
use crate::tools::ToolDefinition;
use anyhow::{Context, Result};
use futures::Stream;
use serde_json::json;
use std::{pin::Pin, process::Stdio};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

pub async fn stream(provider: &ResolvedProvider, messages: Vec<OpenAIMessage>, system: Option<&str>, tools: Vec<ToolDefinition>, max_tokens: u32) -> Result<Pin<Box<dyn Stream<Item = Result<String>> + Send>>> {
    let dev_script = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../scripts/provider-runtime.mjs");
    let packaged_script = std::env::current_exe()?.parent().context("Application directory unavailable")?.join("runtime/provider-runtime.mjs");
    let script = if cfg!(debug_assertions) && dev_script.exists() { dev_script } else { packaged_script };
    let mut command = tokio::process::Command::new("node");
    command.arg(script).env("CLAUDE_PROVIDER_WORKER", "1").stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null()).kill_on_drop(true);
    #[cfg(windows)]
    command.creation_flags(0x08000000);
    let mut child = command.spawn().context("Built-in providers require Node.js 22+ to be installed")?;
    let payload = json!({"provider": provider.provider, "model": provider.model, "effort": provider.model.effort,
        "messages": messages, "system_prompt": system, "tools": tools, "max_tokens": max_tokens});
    let mut stdin = child.stdin.take().context("Provider stdin unavailable")?;
    stdin.write_all(serde_json::to_string(&payload)?.as_bytes()).await?;
    stdin.shutdown().await?;
    drop(stdin);
    let mut lines = BufReader::new(child.stdout.take().context("Provider stdout unavailable")?).lines();
    Ok(Box::pin(async_stream::try_stream! {
        while let Some(line) = tokio::time::timeout(std::time::Duration::from_secs(300), lines.next_line()).await?? {
            let value: serde_json::Value = serde_json::from_str(&line)?;
            if let Some(error) = value.get("error").and_then(|v| v.as_str()) { Err(anyhow::anyhow!("{}", error))?; }
            yield format!("data: {}\n\n", line);
        }
        let status = child.wait().await?;
        if !status.success() { Err(anyhow::anyhow!("Provider runtime exited with {}", status))?; }
        yield "data: [DONE]\n\n".to_string();
    }))
}
