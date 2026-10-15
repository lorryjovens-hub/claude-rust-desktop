//! Bounded, tool-free generation using the same transports as interactive turns.
use super::{anthropic_client::*, openai_client::*, provider_manager::ResolvedProvider};
use anyhow::{ensure, Context, Result};

pub async fn generate(provider: ResolvedProvider, system: &str, input: &str) -> Result<String> {
    ensure!(
        provider.model.supports_text,
        "Selected model does not accept text"
    );
    let max_tokens = provider.model.max_tokens.unwrap_or(4096).min(4096);
    ensure!(max_tokens > 0, "Model output budget must be positive");
    if let Some(limit) = provider.model.context_window {
        ensure!(
            (system.len() + input.len()) as u64 + max_tokens as u64 <= limit as u64,
            "Generation input exceeds the configured context budget"
        );
    }
    let operation = async {
        let text = if provider.provider.builtin_id.is_some()
            || matches!(
                provider.provider.api_format,
                super::provider_manager::ApiFormat::OpenAI
            ) {
            let response = OpenAIClient::new()
                .send_message(
                    &provider,
                    vec![OpenAIMessage {
                        role: "user".into(),
                        content: OpenAIContent::Text(input.into()),
                        tool_calls: None,
                        tool_call_id: None,
                        reasoning_content: None,
                        provider_content: None,
                    }],
                    Some(system),
                    vec![],
                    max_tokens,
                )
                .await?;
            let choice = response
                .choices
                .into_iter()
                .next()
                .context("Model returned no choices")?;
            ensure!(
                choice.finish_reason.as_deref() == Some("stop"),
                "Generation did not finish normally: {:?}",
                choice.finish_reason
            );
            ensure!(
                choice.message.tool_calls.as_ref().is_none_or(Vec::is_empty),
                "Unexpected tool request"
            );
            match choice.message.content {
                OpenAIContent::Text(text) => text,
                _ => anyhow::bail!("Expected text response"),
            }
        } else {
            let response = AnthropicClient::new()
                .send_message(
                    &provider,
                    vec![AnthropicMessage {
                        role: "user".into(),
                        content: AnthropicContent::Text(input.into()),
                    }],
                    Some(system),
                    vec![],
                    max_tokens,
                )
                .await?;
            ensure!(
                matches!(
                    response.stop_reason.as_deref(),
                    Some("end_turn" | "stop_sequence")
                ),
                "Generation did not finish normally: {:?}",
                response.stop_reason
            );
            let mut text = String::new();
            for block in response.content {
                match block {
                    ContentBlock::Text { text: part } => text.push_str(&part),
                    ContentBlock::ToolUse { .. } => anyhow::bail!("Unexpected tool request"),
                    _ => {}
                }
            }
            text
        };
        ensure!(
            !text.trim().is_empty() && text.chars().count() <= 32_000,
            "Empty or oversized generation result"
        );
        Ok(text)
    };
    tokio::time::timeout(std::time::Duration::from_secs(120), operation)
        .await
        .context("Generation timed out")?
}
