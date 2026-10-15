//! Native built-in provider transport. No child process or JavaScript runtime is used.
use super::{openai_client::OpenAIMessage, provider_manager::ResolvedProvider};
use crate::{
    performance::{checked_response, retry_with_backoff},
    tools::ToolDefinition,
};
use anyhow::{bail, Context, Result};
use base64::Engine;
use futures::{Stream, StreamExt};
use serde_json::Value;
use std::{pin::Pin, sync::LazyLock};

mod bedrock;
mod cloud_auth;
mod events;
mod messages;
#[cfg(test)]
mod tests;

static CATALOG: LazyLock<Value> = LazyLock::new(|| {
    serde_json::from_str(include_str!("../../../../src/data/providerCatalog.json"))
        .expect("embedded provider catalog")
});
pub type ProviderStream = Pin<Box<dyn Stream<Item = Result<String>> + Send>>;

pub(super) fn metadata(provider: &ResolvedProvider) -> Result<(&'static Value, &'static Value)> {
    let id = provider
        .provider
        .builtin_id
        .as_deref()
        .context("Built-in provider ID is missing")?;
    let entry = CATALOG["providers"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["id"] == id)
        .with_context(|| format!("Unknown built-in provider: {id}"))?;
    let model = entry["models"]
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["id"] == provider.model.id)
        .unwrap_or(&Value::Null);
    Ok((entry, model))
}

pub async fn stream(
    provider: &ResolvedProvider,
    messages: Vec<OpenAIMessage>,
    system: Option<&str>,
    tools: Vec<ToolDefinition>,
    max_tokens: u32,
) -> Result<ProviderStream> {
    let (entry, model) = metadata(provider)?;
    let protocol = provider
        .model
        .api
        .as_deref()
        .or_else(|| model["api"].as_str())
        .context("Select a catalog model or specify its protocol")?;
    let api = match protocol {
        "mistral-conversations" => "openai-completions",
        "azure-openai-responses" | "openai-codex-responses" => "openai-responses",
        "google-vertex" => "google-generative-ai",
        "bedrock-converse-stream" => protocol,
        "openai-completions" | "anthropic-messages" | "openai-responses" | "google-generative-ai" => protocol,
        _ => bail!("Built-in protocol '{protocol}' is not implemented by the native Rust transport; no fallback protocol or Node process was used"),
    };
    let key = credential(provider, entry["id"].as_str().unwrap_or_default());
    let configured = provider.provider.base_url.trim_end_matches('/');
    let region = if protocol == "bedrock-converse-stream" {
        provider
            .model
            .id
            .strip_prefix("arn:aws:bedrock:")
            .and_then(|s| s.split(':').next())
            .map(str::to_string)
            .or_else(|| std::env::var("AWS_REGION").ok())
            .or_else(|| std::env::var("AWS_DEFAULT_REGION").ok())
            .or_else(|| {
                reqwest::Url::parse(if configured.is_empty() {
                    model["baseUrl"].as_str().unwrap_or_default()
                } else {
                    configured
                })
                .ok()
                .and_then(|u| {
                    u.host_str()
                        .and_then(|h| h.strip_prefix("bedrock-runtime."))
                        .and_then(|h| h.split('.').next())
                        .map(str::to_string)
                })
            })
            .unwrap_or_else(|| "us-east-1".into())
    } else {
        String::new()
    };
    let base = if configured.is_empty()
        || Some(configured) == entry["baseUrl"].as_str().map(|s| s.trim_end_matches('/'))
    {
        model["baseUrl"]
            .as_str()
            .or_else(|| entry["baseUrl"].as_str())
            .unwrap_or(configured)
    } else {
        configured
    }
    .trim_end_matches('/');
    let base = if protocol == "bedrock-converse-stream"
        && (configured.is_empty() || configured.contains(".amazonaws.com"))
    {
        format!(
            "https://bedrock-runtime.{region}.amazonaws.com{}",
            if region.starts_with("cn-") { ".cn" } else { "" }
        )
    } else if protocol == "google-vertex" && (base.is_empty() || base.contains("{location}")) {
        if !key.is_empty() && key != "gcp-vertex-credentials" {
            "https://aiplatform.googleapis.com/v1/publishers/google".to_string()
        } else {
            let project = std::env::var("GOOGLE_CLOUD_PROJECT")
                .or_else(|_| std::env::var("GCLOUD_PROJECT"))
                .context("Vertex ADC requires GOOGLE_CLOUD_PROJECT")?;
            let location = std::env::var("GOOGLE_CLOUD_LOCATION")
                .context("Vertex ADC requires GOOGLE_CLOUD_LOCATION")?;
            let host = if location == "global" {
                "aiplatform.googleapis.com".to_string()
            } else {
                format!("{location}-aiplatform.googleapis.com")
            };
            format!(
                "https://{host}/v1/projects/{}/locations/{}/publishers/google",
                urlencoding::encode(&project),
                urlencoding::encode(&location)
            )
        }
    } else if protocol == "azure-openai-responses" && base.is_empty() {
        std::env::var("AZURE_OPENAI_BASE_URL")
            .or_else(|_| {
                std::env::var("AZURE_OPENAI_RESOURCE_NAME")
                    .map(|r| format!("https://{r}.openai.azure.com/openai/v1"))
            })
            .unwrap_or_default()
    } else {
        base.to_string()
    };
    if base.is_empty() {
        bail!("Provider {} requires an explicit base URL", entry["id"]);
    }
    let mut base = base;
    for name in ["CLOUDFLARE_ACCOUNT_ID", "CLOUDFLARE_GATEWAY_ID"] {
        let placeholder = format!("{{{name}}}");
        if base.contains(&placeholder) {
            base = base.replace(
                &placeholder,
                &urlencoding::encode(
                    &std::env::var(name)
                        .with_context(|| format!("Provider endpoint requires {name}"))?,
                ),
            );
        }
    }
    let base = base.trim_end_matches('/');
    let url = match api {
        "openai-completions" => {
            if protocol == "mistral-conversations" && !base.ends_with("/v1") {
                endpoint(base, "/v1/chat/completions")
            } else {
                endpoint(base, "/chat/completions")
            }
        }
        "anthropic-messages" => {
            if base.ends_with("/messages") {
                base.to_owned()
            } else if base.ends_with("/v1") {
                format!("{base}/messages")
            } else {
                format!("{base}/v1/messages")
            }
        }
        "openai-responses" if protocol == "openai-codex-responses" => {
            if base.ends_with("/codex") || base.ends_with("/codex/responses") {
                endpoint(base, "/responses")
            } else {
                endpoint(base, "/codex/responses")
            }
        }
        "openai-responses" if protocol == "azure-openai-responses" => {
            let parsed = reqwest::Url::parse(base).context("Invalid Azure base URL")?;
            let base = if parsed.path().trim_matches('/').is_empty() {
                format!("{base}/openai/v1")
            } else if parsed.path() == "/openai" {
                format!("{base}/v1")
            } else {
                base.to_string()
            };
            format!(
                "{}?api-version={}",
                endpoint(&base, "/responses"),
                urlencoding::encode(
                    &std::env::var("AZURE_OPENAI_API_VERSION").unwrap_or_else(|_| "v1".into())
                )
            )
        }
        "openai-responses" => endpoint(base, "/responses"),
        "bedrock-converse-stream" => format!(
            "{base}/model/{}/converse-stream",
            urlencoding::encode(&provider.model.id)
        ),
        _ => {
            let parsed = reqwest::Url::parse(base).context("Invalid Gemini base URL")?;
            let base = if protocol == "google-generative-ai"
                && parsed.path().trim_matches('/').is_empty()
            {
                format!("{base}/v1beta")
            } else {
                base.to_string()
            };
            format!(
                "{base}/models/{}:streamGenerateContent?alt=sse",
                urlencoding::encode(&provider.model.id)
            )
        }
    };
    let initiator = if messages.last().is_some_and(|m| m.role != "user") {
        "agent"
    } else {
        "user"
    };
    let vision = messages.iter().any(|m| matches!(&m.content, super::openai_client::OpenAIContent::Multi(parts) if parts.iter().any(|p| matches!(p,super::openai_client::OpenAIContentPart::Image{..}))));
    let mut body = messages::request(api, provider, model, messages, system, tools, max_tokens)?;
    if protocol == "openai-codex-responses" {
        body.as_object_mut().unwrap().remove("max_output_tokens");
    }
    if protocol == "mistral-conversations" {
        body.as_object_mut().unwrap().remove("stream_options");
        if let Some(messages) = body["messages"].as_array_mut() {
            for message in messages {
                if let Some(reasoning) = message
                    .as_object_mut()
                    .and_then(|m| m.remove("reasoning_content"))
                    .and_then(|v| v.as_str().map(str::to_string))
                    .filter(|s| !s.is_empty())
                {
                    let mut parts = vec![
                        serde_json::json!({"type":"thinking","thinking":[{"type":"text","text":reasoning}]}),
                    ];
                    match &message["content"] {
                        Value::String(text) if !text.is_empty() => {
                            parts.push(serde_json::json!({"type":"text","text":text}))
                        }
                        Value::Array(content) => parts.extend(content.clone()),
                        _ => {}
                    }
                    message["content"] = serde_json::json!(parts);
                }
            }
        }
        if provider.model.effort.as_deref() == Some("off") {
            body.as_object_mut().unwrap().remove("reasoning_effort");
        }
        if provider.model.effort.as_deref().is_some_and(|e| e != "off")
            && !matches!(
                provider.model.id.as_str(),
                "mistral-small-2603" | "mistral-small-latest" | "mistral-medium-3.5"
            )
        {
            body.as_object_mut().unwrap().remove("reasoning_effort");
            body["prompt_mode"] = serde_json::json!("reasoning");
        }
    }
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(300))
        .redirect(reqwest::redirect::Policy::none())
        .build()?;
    let google_token =
        if protocol == "google-vertex" && (key.is_empty() || key == "gcp-vertex-credentials") {
            Some(cloud_auth::google_token(&client).await?)
        } else {
            None
        };
    let encoded_body = serde_json::to_string(&body)?;
    let account = if protocol == "openai-codex-responses" {
        Some(codex_account(&key)?)
    } else {
        None
    };
    let response = retry_with_backoff(|| async {
        let mut request = client
            .post(&url)
            .header("content-type", "application/json")
            .body(encoded_body.clone());
        if entry["id"] == "github-copilot" {
            request = request
                .header("X-Initiator", initiator)
                .header("Openai-Intent", "conversation-edits");
            if vision {
                request = request.header("Copilot-Vision-Request", "true");
            }
        }
        if protocol == "azure-openai-responses" {
            request = request.header("api-key", &key);
        }
        if let Some(account) = &account {
            request = request
                .header("chatgpt-account-id", account)
                .header("OpenAI-Beta", "responses=experimental")
                .header("originator", "claude-rust");
        }
        match api {
            "anthropic-messages" => {
                request = request.header("anthropic-version", "2023-06-01");
                if entry["id"] == "github-copilot" || key.starts_with("sk-ant-oat") {
                    request = request.bearer_auth(&key);
                } else if entry["id"] != "cloudflare-ai-gateway" {
                    request = request.header("x-api-key", &key);
                }
            }
            "google-generative-ai" => {
                if let Some(token) = &google_token {
                    request = request.bearer_auth(token);
                } else {
                    request = request.header("x-goog-api-key", &key);
                }
            }
            "bedrock-converse-stream" => {
                if key.is_empty() {
                    request =
                        request.headers(cloud_auth::aws_headers(&url, &region, &encoded_body)?);
                } else {
                    request = request.bearer_auth(&key);
                }
            }
            _ => {
                if !key.is_empty()
                    && protocol != "azure-openai-responses"
                    && entry["id"] != "cloudflare-ai-gateway"
                {
                    request = request.bearer_auth(&key);
                }
            }
        }
        if entry["id"] == "cloudflare-ai-gateway" {
            request = request.header("cf-aig-authorization", format!("Bearer {key}"));
        }
        checked_response(request.send().await?).await
    })
    .await?;
    let api = api.to_string();
    let mut bytes = response.bytes_stream();
    Ok(Box::pin(async_stream::try_stream! {
        let mut pending = Vec::new();
        let mut state = events::State::default();
        while let Some(part) = bytes.next().await {
            pending.extend_from_slice(&part?);
            if pending.len() > 16 * 1024 * 1024 { Err(anyhow::anyhow!("Provider SSE frame exceeds 16 MiB"))?; }
            if api == "bedrock-converse-stream" {
                while let Some(value)=bedrock::next(&mut pending)? {for chunk in state.convert(&api,value)? {yield format!("data: {}\n\n",chunk);}}
                continue;
            }
            while let Some((end, separator)) = frame_end(&pending) {
                let frame = String::from_utf8(pending[..end].to_vec()).context("Provider returned invalid UTF-8")?;
                pending.drain(..end + separator);
                let data = frame.lines().filter_map(|line| line.strip_prefix("data:").map(|v| v.strip_prefix(' ').unwrap_or(v))).collect::<Vec<_>>().join("\n");
                if data.is_empty() { continue; }
                if data == "[DONE]" { state.finished = true; continue; }
                let value: Value = serde_json::from_str(&data).context("Invalid provider SSE JSON")?;
                for chunk in state.convert(&api, value)? { yield format!("data: {}\n\n", chunk); }
            }
        }
        if !pending.iter().all(u8::is_ascii_whitespace) { Err(anyhow::anyhow!("Provider stream ended inside an SSE frame"))?; }
        if !state.finished { Err(anyhow::anyhow!("Provider stream ended before completion"))?; }
        if let Some(chunk)=state.complete(&api) {yield format!("data: {}\n\n",chunk);}
        yield "data: [DONE]\n\n".to_string();
    }))
}

fn endpoint(base: &str, suffix: &str) -> String {
    if base.ends_with(suffix) {
        base.to_owned()
    } else {
        format!("{base}{suffix}")
    }
}

fn frame_end(bytes: &[u8]) -> Option<(usize, usize)> {
    (0..bytes.len()).find_map(|i| {
        if bytes[i..].starts_with(b"\r\n\r\n") {
            Some((i, 4))
        } else if bytes[i..].starts_with(b"\n\n") {
            Some((i, 2))
        } else {
            None
        }
    })
}

fn credential(provider: &ResolvedProvider, id: &str) -> String {
    if !provider.provider.api_key.is_empty() {
        return provider.provider.api_key.clone();
    }
    std::env::var(credential_env(id)).unwrap_or_default()
}

pub fn credential_env(id: &str) -> String {
    match id {
        "google" => "GEMINI_API_KEY".to_string(),
        "google-vertex" => "GOOGLE_CLOUD_API_KEY".to_string(),
        "amazon-bedrock" => "AWS_BEARER_TOKEN_BEDROCK".to_string(),
        "zai" | "zai-coding-cn" => "ZAI_API_KEY".to_string(),
        "moonshotai" | "moonshotai-cn" => "MOONSHOT_API_KEY".to_string(),
        "minimax-cn" => "MINIMAX_CN_API_KEY".to_string(),
        "kimi-coding" => "KIMI_API_KEY".to_string(),
        "huggingface" => "HF_TOKEN".to_string(),
        "opencode-go" => "OPENCODE_API_KEY".to_string(),
        "vercel-ai-gateway" => "AI_GATEWAY_API_KEY".to_string(),
        "cloudflare-ai-gateway" | "cloudflare-workers-ai" => "CLOUDFLARE_API_KEY".to_string(),
        "qwen-token-plan-cn" | "qwen-token-plan-individual" => {
            "QWEN_TOKEN_PLAN_API_KEY".to_string()
        }
        "github-copilot" => "COPILOT_GITHUB_TOKEN".to_string(),
        "azure-openai-responses" => "AZURE_OPENAI_API_KEY".to_string(),
        "openai-codex" => "OPENAI_CODEX_TOKEN".to_string(),
        _ => format!("{}_API_KEY", id.replace('-', "_").to_uppercase()),
    }
}

fn codex_account(token: &str) -> Result<String> {
    let payload = token
        .split('.')
        .nth(1)
        .context("Codex requires an OAuth access token with an account claim")?;
    let data = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(payload.trim_end_matches('='))
        .context("Invalid Codex token encoding")?;
    let payload: Value = serde_json::from_slice(&data).context("Invalid Codex token payload")?;
    payload["https://api.openai.com/auth"]["chatgpt_account_id"]
        .as_str()
        .map(str::to_string)
        .context("Codex token has no account claim")
}
