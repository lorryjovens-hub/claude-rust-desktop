//! Provider-hosted web search. Only structured search sources become results;
//! generated prose is never parsed for URLs, and failures never change providers.
use crate::native_engine::provider_manager::{ApiFormat, ResolvedProvider};
use anyhow::{anyhow, bail, Result};
use serde_json::{json, Value};
use std::{
    collections::{HashMap, HashSet},
    time::Duration,
};

const MAX_BODY_BYTES: usize = 2 * 1024 * 1024;
const MAX_RESULTS: usize = 8;

#[derive(Clone, Copy, Debug, PartialEq)]
enum Strategy {
    OpenAI,
    Anthropic,
    DeepSeek,
}

impl Strategy {
    fn name(self) -> &'static str {
        match self {
            Self::OpenAI => "openai_native",
            Self::Anthropic => "anthropic_native",
            Self::DeepSeek => "deepseek_native",
        }
    }
    fn api_format(self) -> &'static str {
        if self == Self::OpenAI {
            "openai"
        } else {
            "anthropic"
        }
    }
}

pub fn uses_public_search(provider: &ResolvedProvider) -> bool {
    provider.provider.web_search_strategy.as_deref() == Some("public")
}

fn strategy(provider: &ResolvedProvider, base: &reqwest::Url) -> Result<Strategy> {
    match provider.provider.web_search_strategy.as_deref().unwrap_or("auto") {
        "openai_native" | "openai" => Ok(Strategy::OpenAI),
        "anthropic_native" | "anthropic" => Ok(Strategy::Anthropic),
        "deepseek_native" | "deepseek" => Ok(Strategy::DeepSeek),
        "auto" | "" => {
            if base.host_str() == Some("api.deepseek.com") { return Ok(Strategy::DeepSeek); }
            if provider.provider.builtin_id.as_deref() == Some("anthropic") || provider.model.api.as_deref() == Some("anthropic-messages") {
                return Ok(Strategy::Anthropic);
            }
            Ok(match provider.provider.api_format { ApiFormat::OpenAI => Strategy::OpenAI, ApiFormat::Anthropic => Strategy::Anthropic })
        }
        "disabled" => bail!("WebSearch is disabled for the selected provider. Enable it in provider settings."),
        "public" => bail!("Public WebSearch must use the explicitly selected public search adapter."),
        _ => bail!("This provider's configured WebSearch strategy is not supported. Select a supported native search API in provider settings."),
    }
}

fn endpoint(provider: &ResolvedProvider) -> Result<(Strategy, reqwest::Url)> {
    let base = if provider.provider.base_url.trim().is_empty() {
        match provider.provider.builtin_id.as_deref() {
            Some("openai") => "https://api.openai.com/v1",
            Some("anthropic") => "https://api.anthropic.com",
            Some("deepseek") => "https://api.deepseek.com",
            _ => bail!("Native WebSearch needs an explicit API base URL for this provider. Configure it in provider settings."),
        }
    } else {
        provider.provider.base_url.trim()
    };
    let mut url = reqwest::Url::parse(base)
        .map_err(|_| anyhow!("The selected provider has an invalid API base URL."))?;
    if !matches!(url.scheme(), "https" | "http")
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        bail!("The provider API base URL must be HTTP(S), without embedded credentials, query parameters or a fragment.");
    }
    let selected = strategy(provider, &url)?;
    let mut path = url.path().trim_end_matches('/').to_string();
    let mut explicit_endpoint = false;
    for suffix in ["/chat/completions", "/responses", "/messages"] {
        if path.ends_with(suffix) {
            path.truncate(path.len() - suffix.len());
            explicit_endpoint = true;
            break;
        }
    }
    if selected == Strategy::DeepSeek && url.host_str() == Some("api.deepseek.com") {
        // Known official chat roots map to the same origin's documented search API.
        // A custom gateway always retains its own configured origin and prefix.
        if matches!(path.as_str(), "" | "/v1" | "/anthropic" | "/anthropic/v1") {
            path = "/anthropic/v1".into();
        }
    }
    // Infer v1 only for a bare origin. Gateways can mount their API at any
    // prefix/version or expose an explicit endpoint directly at the root.
    if path.is_empty() && !explicit_endpoint {
        path.push_str("/v1");
    }
    path.push_str(if selected == Strategy::OpenAI {
        "/responses"
    } else {
        "/messages"
    });
    url.set_path(&path);
    Ok((selected, url))
}

fn credentials(provider: &ResolvedProvider) -> Result<(String, bool)> {
    let configured = provider.provider.api_key.trim();
    if !configured.is_empty() {
        if configured.starts_with("sk-ant-oat") {
            bail!("Native WebSearch currently requires an API key; Claude subscription OAuth credentials are not supported by this adapter.");
        }
        return Ok((configured.into(), false));
    }
    let env_value = |name| std::env::var(name).ok().filter(|v| !v.trim().is_empty());
    let key = match provider.provider.builtin_id.as_deref() {
        Some("openai") => env_value("OPENAI_API_KEY"),
        Some("deepseek") => env_value("DEEPSEEK_API_KEY"),
        Some("anthropic") => {
            if let Some(token) = env_value("ANTHROPIC_AUTH_TOKEN") {
                return Ok((token, true));
            }
            if env_value("ANTHROPIC_OAUTH_TOKEN").is_some() {
                bail!("Native WebSearch currently requires an API key; Claude subscription OAuth credentials are not supported by this adapter.");
            }
            env_value("ANTHROPIC_API_KEY")
        }
        _ => None,
    };
    key.map(|key| (key, false)).ok_or_else(|| anyhow!("The selected provider has no API key configured for native WebSearch. Set its API key in provider settings; subscription authentication is not supported by this adapter."))
}

fn request_body(strategy: Strategy, query: &str, model: &str) -> Value {
    let prompt = format!("Use web search to find sources for this query: {query}\nReturn a brief summary with citations. You must perform web search; do not answer from memory.");
    if strategy == Strategy::OpenAI {
        json!({"model": model, "input": prompt, "tools": [{"type":"web_search"}],
            "tool_choice":"required", "include":["web_search_call.action.sources"],
            "max_output_tokens":2048, "store":false, "stream":false})
    } else {
        let mut body = json!({"model": model, "max_tokens":2048, "stream":false,
            "messages":[{"role":"user", "content":prompt}],
            "tools":[{"type":"web_search_20250305", "name":"web_search", "max_uses":3}],
            "tool_choice":{"type":"tool", "name":"web_search"}});
        // Match the official dsh plugin: DeepSeek decides when to invoke the
        // server tool; forced tool choice is incompatible with some thinking models.
        if strategy == Strategy::DeepSeek {
            body.as_object_mut().unwrap().remove("tool_choice");
        }
        body
    }
}

fn clipped(value: &str, limit: usize) -> String {
    value.chars().take(limit).collect()
}

fn add_result(
    results: &mut Vec<Value>,
    seen: &mut HashSet<String>,
    source: &Value,
    snippet: Option<&str>,
) {
    if results.len() >= MAX_RESULTS {
        return;
    }
    let Some(url) = source.get("url").and_then(Value::as_str) else {
        return;
    };
    if url.len() > 4096 {
        return;
    }
    let Ok(parsed) = reqwest::Url::parse(url) else {
        return;
    };
    if !matches!(parsed.scheme(), "http" | "https")
        || parsed.host_str().is_none()
        || !parsed.username().is_empty()
        || parsed.password().is_some()
        || !seen.insert(url.into())
    {
        return;
    }
    results.push(
        json!({"title":clipped(source.get("title").and_then(Value::as_str).unwrap_or(url), 300),
        "url":url, "snippet":clipped(snippet.unwrap_or(""), 1500)}),
    );
}

fn parse_anthropic(response: &Value) -> Result<Vec<Value>> {
    let blocks = response
        .get("content")
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow!("The provider returned no structured search content."))?;
    let mut snippets = HashMap::new();
    for block in blocks.iter().filter(|b| b["type"] == "text") {
        if let Some(citations) = block["citations"].as_array() {
            for cite in citations {
                if let (Some(url), Some(text)) = (cite["url"].as_str(), cite["cited_text"].as_str())
                {
                    snippets.entry(url).or_insert(text);
                }
            }
        }
    }
    let mut results = Vec::new();
    let mut seen = HashSet::new();
    let mut searched = false;
    for block in blocks
        .iter()
        .filter(|b| b["type"] == "web_search_tool_result")
    {
        searched = true;
        if block["content"]["type"] == "web_search_tool_result_error" {
            let code = match block["content"]["error_code"].as_str() {
                Some("invalid_tool_input") => "invalid tool input",
                Some("unavailable") => "search unavailable",
                Some("max_uses_exceeded") => "search limit exceeded",
                Some("too_many_requests") => "rate limited",
                Some("query_too_long") => "query too long",
                _ => "native search error",
            };
            bail!("The provider's WebSearch tool failed ({code}).");
        }
        if let Some(sources) = block["content"].as_array() {
            for source in sources.iter().filter(|s| s["type"] == "web_search_result") {
                let snippet = source["url"]
                    .as_str()
                    .and_then(|u| snippets.get(u).copied());
                add_result(&mut results, &mut seen, source, snippet);
            }
        }
    }
    if !searched {
        bail!("The provider returned no native web search results. The selected model or endpoint may not support WebSearch.");
    }
    Ok(results)
}

fn parse_openai(response: &Value) -> Result<Vec<Value>> {
    let output = response["output"]
        .as_array()
        .ok_or_else(|| anyhow!("The provider returned no structured search output."))?;
    if !output
        .iter()
        .any(|item| item["type"] == "web_search_call" && item["status"] == "completed")
    {
        bail!("The provider did not complete a native web search. The selected model or endpoint may not support WebSearch.");
    }
    let mut results = Vec::new();
    let mut seen = HashSet::new();
    // Prefer citation metadata, which generally includes titles. No prose URL extraction.
    for item in output.iter().filter(|i| i["type"] == "message") {
        if let Some(content) = item["content"].as_array() {
            for block in content {
                if let Some(annotations) = block["annotations"].as_array() {
                    for source in annotations.iter().filter(|a| a["type"] == "url_citation") {
                        add_result(&mut results, &mut seen, source, None);
                    }
                }
            }
        }
    }
    for item in output
        .iter()
        .filter(|i| i["type"] == "web_search_call" && i["status"] == "completed")
    {
        if let Some(sources) = item["action"]["sources"].as_array() {
            for source in sources {
                add_result(&mut results, &mut seen, source, source["snippet"].as_str());
            }
        }
    }
    Ok(results)
}

fn normalized_usage(response: &Value, strategy: Strategy) -> Value {
    let usage = &response["usage"];
    let count = |name: &str| usage[name].as_u64();
    let cached = if strategy == Strategy::OpenAI {
        usage["input_tokens_details"]["cached_tokens"].as_u64()
    } else {
        count("cache_read_input_tokens")
    };
    let searches = if strategy == Strategy::OpenAI {
        response["output"]
            .as_array()
            .map(|a| a.iter().filter(|v| v["type"] == "web_search_call").count() as u64)
    } else {
        usage["server_tool_use"]["web_search_requests"].as_u64()
    };
    if strategy == Strategy::OpenAI {
        // Responses input_tokens already includes cached tokens. Use the chat
        // usage vocabulary understood by RequestTrace to avoid counting twice.
        json!({"prompt_tokens":usage["input_tokens"], "completion_tokens":usage["output_tokens"],
            "total_tokens":usage["total_tokens"], "prompt_tokens_details":{"cached_tokens":cached,
                "cache_write_tokens":usage["input_tokens_details"]["cache_write_tokens"].as_u64()},
            "completion_tokens_details":usage["output_tokens_details"], "web_search_requests":searches})
    } else {
        json!({"input_tokens":usage["input_tokens"], "output_tokens":usage["output_tokens"],
            "cache_read_input_tokens":cached, "cache_creation_input_tokens":count("cache_creation_input_tokens"),
            "web_search_requests":searches})
    }
}

/// An auxiliary model request can incur tokens even when its native search
/// fails. The caller can downcast this error to retain authoritative usage.
#[derive(Debug, thiserror::Error)]
#[error("{message}")]
pub struct SearchResponseError {
    message: String,
    pub usage: Value,
    pub model: String,
    pub api_format: &'static str,
    pub response_id: Value,
}

fn response_results(body: &Value, selected: Strategy, model: &str) -> Result<Vec<Value>> {
    let result = (|| {
        if body.get("error").is_some_and(|e| !e.is_null()) {
            bail!("The provider rejected the native WebSearch request. Check its model and native search configuration.");
        }
        let results = if selected == Strategy::OpenAI {
            parse_openai(body)?
        } else {
            parse_anthropic(body)?
        };
        if results.is_empty() {
            bail!("Provider WebSearch completed without usable sources. Try a more specific query or verify the provider's search support.");
        }
        Ok(results)
    })();
    result.map_err(|error: anyhow::Error| {
        SearchResponseError {
            message: error.to_string(),
            usage: normalized_usage(body, selected),
            model: body["model"].as_str().unwrap_or(model).to_string(),
            api_format: selected.api_format(),
            response_id: body["id"].clone(),
        }
        .into()
    })
}

pub async fn search(input: Value, provider: &ResolvedProvider) -> Result<Value> {
    let query = input["query"]
        .as_str()
        .map(str::trim)
        .filter(|q| !q.is_empty())
        .ok_or_else(|| anyhow!("WebSearch requires a non-empty query."))?;
    if query.chars().count() > 2000 {
        bail!("WebSearch query must be at most 2000 characters.");
    }
    let (selected, url) = endpoint(provider)?;
    let (api_key, bearer_only) = credentials(provider)?;
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(45))
        .connect_timeout(Duration::from_secs(10))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|_| anyhow!("Could not initialize the provider search HTTP client."))?;
    let mut request = client
        .post(url)
        .header("accept", "application/json")
        .bearer_auth(&api_key)
        .json(&request_body(selected, query, &provider.model.id));
    if selected != Strategy::OpenAI {
        request = request.header("anthropic-version", "2023-06-01");
        if !bearer_only {
            request = request.header("x-api-key", &api_key);
        }
    }
    let mut response = request.send().await.map_err(|e| if e.is_timeout() {
        anyhow!("Provider WebSearch timed out after 45 seconds.")
    } else { anyhow!("Could not reach the configured provider's native search API. Check its endpoint and connection.") })?;
    if !response.status().is_success() {
        bail!("Provider WebSearch returned HTTP {}. Check the API key, selected model and native search support for the configured endpoint. No public search fallback was used.", response.status().as_u16());
    }
    if response
        .content_length()
        .is_some_and(|n| n > MAX_BODY_BYTES as u64)
    {
        bail!("Provider WebSearch response exceeded the 2 MiB limit.");
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|_| {
        anyhow!("Could not read the provider WebSearch response within its time limit.")
    })? {
        if bytes.len() + chunk.len() > MAX_BODY_BYTES {
            bail!("Provider WebSearch response exceeded the 2 MiB limit.");
        }
        bytes.extend_from_slice(&chunk);
    }
    let body: Value = serde_json::from_slice(&bytes)
        .map_err(|_| anyhow!("Provider WebSearch returned invalid JSON."))?;
    let results = response_results(&body, selected, &provider.model.id)?;
    Ok(
        json!({"query":query, "provider":selected.name(), "provider_id":provider.provider.id,
        "provider_name":provider.provider.name, "model":body["model"].as_str().unwrap_or(&provider.model.id),
        "requested_model":provider.model.id, "response_id":body["id"], "api_format":selected.api_format(),
        "results":results, "usage":normalized_usage(&body, selected)}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::native_engine::provider_manager::{ModelConfig, Provider};

    fn provider(base: &str, strategy: Option<&str>) -> ResolvedProvider {
        ResolvedProvider {
            provider: Provider {
                id: "p".into(),
                name: "Test provider".into(),
                base_url: base.into(),
                api_key: "test-key".into(),
                api_format: ApiFormat::OpenAI,
                builtin_id: None,
                models: vec![],
                enabled: true,
                web_search_strategy: strategy.map(String::from),
            },
            model: ModelConfig {
                id: "test-model".into(),
                name: "Test".into(),
                enabled: true,
                max_tokens: None,
                supports_vision: false,
                supports_web_search: false,
                context_window: None,
                supports_text: true,
                api: None,
                effort: None,
            },
        }
    }

    #[test]
    fn endpoint_preserves_gateway_origin_and_prefix() {
        for (base, expected) in [
            (
                "https://gateway.example/proxy/v1/chat/completions",
                "https://gateway.example/proxy/v1/responses",
            ),
            (
                "https://gateway.example",
                "https://gateway.example/v1/responses",
            ),
            (
                "https://gateway.example/v1/",
                "https://gateway.example/v1/responses",
            ),
        ] {
            assert_eq!(
                endpoint(&provider(base, None)).unwrap().1.as_str(),
                expected
            );
        }
        assert_eq!(
            endpoint(&provider(
                "https://gateway.example/proxy/v1",
                Some("deepseek_native")
            ))
            .unwrap()
            .1
            .as_str(),
            "https://gateway.example/proxy/v1/messages"
        );
        assert_eq!(
            endpoint(&provider("https://api.deepseek.com/v1", None)).unwrap(),
            (
                Strategy::DeepSeek,
                reqwest::Url::parse("https://api.deepseek.com/anthropic/v1/messages").unwrap()
            )
        );
        assert_eq!(
            endpoint(&provider("https://api.deepseek.com.evil.example/v1", None))
                .unwrap()
                .0,
            Strategy::OpenAI
        );
        assert!(endpoint(&provider("https://user:key@example.com", None)).is_err());
        assert!(endpoint(&provider("https://example.com", Some("disabled"))).is_err());
    }

    #[test]
    fn anthropic_joins_citations_and_rejects_unstructured_answers() {
        let body = json!({"content":[{"type":"text", "text":"See https://invented.example", "citations":[{"url":"https://real.example", "cited_text":"Actual excerpt"}]},
            {"type":"web_search_tool_result", "content":[{"type":"web_search_result", "url":"https://real.example", "title":"Real"}, {"type":"web_search_result", "url":"https://real.example"}, {"type":"web_search_result", "url":"javascript:alert(1)"}]}]});
        let results = parse_anthropic(&body).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0]["snippet"], "Actual excerpt");
        assert!(parse_anthropic(
            &json!({"content":[{"type":"text", "text":"https://invented.example"}]})
        )
        .is_err());
        assert!(parse_anthropic(&json!({"content":[{"type":"web_search_tool_result", "content":{"type":"web_search_tool_result_error", "error_code":"unavailable"}}]})).is_err());
    }

    #[test]
    fn openai_requires_completed_search_and_uses_only_structured_sources() {
        let body = json!({"output":[{"type":"web_search_call", "status":"completed", "action":{"sources":[{"url":"https://real.example"}]}},
            {"type":"message", "content":[{"type":"output_text", "text":"https://invented.example", "annotations":[{"type":"url_citation", "url":"https://real.example", "title":"Real"}]}]}]});
        let results = parse_openai(&body).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0]["title"], "Real");
        assert!(
            parse_openai(&json!({"output":[{"type":"web_search_call", "status":"failed"}]}))
                .is_err()
        );
        let usage = normalized_usage(
            &json!({"usage":{"input_tokens":100,"output_tokens":10,"input_tokens_details":{"cached_tokens":40}}}),
            Strategy::OpenAI,
        );
        assert_eq!(usage["prompt_tokens"], 100);
        assert_eq!(usage["prompt_tokens_details"]["cached_tokens"], 40);
    }

    #[test]
    fn explicit_endpoints_and_custom_versions_do_not_gain_an_extra_v1() {
        for (base, strategy, expected) in [
            (
                "https://gateway.example/anthropic/messages",
                "anthropic_native",
                "https://gateway.example/anthropic/messages",
            ),
            (
                "https://gateway.example/proxy/responses",
                "openai_native",
                "https://gateway.example/proxy/responses",
            ),
            (
                "https://gateway.example/v2",
                "deepseek_native",
                "https://gateway.example/v2/messages",
            ),
            (
                "https://gateway.example/proxy",
                "openai_native",
                "https://gateway.example/proxy/responses",
            ),
            (
                "https://gateway.example/messages",
                "anthropic_native",
                "https://gateway.example/messages",
            ),
            (
                "https://gateway.example/responses/",
                "openai_native",
                "https://gateway.example/responses",
            ),
            (
                "https://gateway.example/proxy/chat/completions",
                "openai_native",
                "https://gateway.example/proxy/responses",
            ),
            (
                "https://gateway.example/anthropic",
                "deepseek_native",
                "https://gateway.example/anthropic/messages",
            ),
            (
                "https://api.deepseek.com/anthropic",
                "deepseek_native",
                "https://api.deepseek.com/anthropic/v1/messages",
            ),
        ] {
            assert_eq!(
                endpoint(&provider(base, Some(strategy)))
                    .unwrap()
                    .1
                    .as_str(),
                expected
            );
        }
    }

    #[test]
    fn builtins_resolve_documented_defaults_and_respect_overrides() {
        for (id, expected_strategy, expected_url) in [
            (
                "openai",
                Strategy::OpenAI,
                "https://api.openai.com/v1/responses",
            ),
            (
                "anthropic",
                Strategy::Anthropic,
                "https://api.anthropic.com/v1/messages",
            ),
            (
                "deepseek",
                Strategy::DeepSeek,
                "https://api.deepseek.com/anthropic/v1/messages",
            ),
        ] {
            let mut p = provider("", None);
            p.provider.builtin_id = Some(id.into());
            let (selected, url) = endpoint(&p).unwrap();
            assert_eq!(selected, expected_strategy);
            assert_eq!(url.as_str(), expected_url);
            p.provider.base_url = "https://custom.example/proxy/v1".into();
            assert_eq!(endpoint(&p).unwrap().1.host_str(), Some("custom.example"));
        }
        assert!(
            request_body(Strategy::DeepSeek, "test", "deepseek-v4-flash")
                .get("tool_choice")
                .is_none()
        );
        let mut p = provider("https://example.com", None);
        p.provider.api_key = "sk-ant-oat-test".into();
        assert!(credentials(&p).is_err());
    }

    #[test]
    fn unsuccessful_search_preserves_reported_usage_without_inventing_counts() {
        let response = json!({"id":"response-id", "model":"actual-model", "content":[{"type":"tool_use","name":"web_search"}],
            "usage":{"input_tokens":40,"output_tokens":12,"cache_read_input_tokens":8}});
        let error = response_results(&response, Strategy::DeepSeek, "requested").unwrap_err();
        let search_error = error.downcast_ref::<SearchResponseError>().unwrap();
        assert_eq!(search_error.usage["input_tokens"], 40);
        assert_eq!(search_error.model, "actual-model");
        assert_eq!(search_error.api_format, "anthropic");
        assert_eq!(search_error.response_id, "response-id");
        assert!(normalized_usage(&json!({}), Strategy::OpenAI)["prompt_tokens"].is_null());
        assert!(normalized_usage(&json!({}), Strategy::DeepSeek)["input_tokens"].is_null());
        let omitted = normalized_usage(&json!({}), Strategy::OpenAI);
        assert!(omitted["prompt_tokens_details"]["cached_tokens"].is_null());
        assert!(omitted["prompt_tokens_details"]["cache_write_tokens"].is_null());
        let omitted = normalized_usage(&json!({}), Strategy::Anthropic);
        assert!(omitted["cache_read_input_tokens"].is_null());
        assert!(omitted["cache_creation_input_tokens"].is_null());
        let supplied = normalized_usage(
            &json!({"usage":{"input_tokens_details":{"cached_tokens":0,"cache_write_tokens":5}}}),
            Strategy::OpenAI,
        );
        assert_eq!(supplied["prompt_tokens_details"]["cached_tokens"], 0);
        assert_eq!(supplied["prompt_tokens_details"]["cache_write_tokens"], 5);
    }

    #[tokio::test]
    async fn http_failures_do_not_expose_response_bodies_or_follow_redirects() {
        use axum::{
            http::{header, StatusCode},
            routing::post,
            Router,
        };
        let app = Router::new().route(
            "/v1/responses",
            post(|| async {
                (
                    StatusCode::TEMPORARY_REDIRECT,
                    [(header::LOCATION, "https://untrusted.example/")],
                    "secret test-key diagnostic",
                )
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        let error = search(
            json!({"query":"test"}),
            &provider(&format!("http://{address}/v1"), None),
        )
        .await
        .unwrap_err()
        .to_string();
        assert!(error.contains("HTTP 307"));
        assert!(!error.contains("test-key"));
        assert!(!error.contains("untrusted"));
        server.abort();
    }

    #[tokio::test]
    #[ignore = "Requires explicitly configured provider credentials and incurs model/search usage"]
    async fn live_native_provider_search_smoke() {
        let required = |name: &str| {
            std::env::var(name)
                .ok()
                .filter(|value| !value.trim().is_empty())
                .unwrap_or_else(|| panic!("Set {name} before running this ignored live test"))
        };
        let base = required("WEB_SEARCH_SMOKE_BASE_URL");
        let selected_strategy = required("WEB_SEARCH_SMOKE_STRATEGY");
        let mut configured = provider(&base, Some(&selected_strategy));
        configured.provider.api_key = required("WEB_SEARCH_SMOKE_API_KEY");
        configured.model.id = required("WEB_SEARCH_SMOKE_MODEL");
        let started = std::time::Instant::now();
        let result = search(
            json!({"query":"OpenAI Codex changelog official documentation"}),
            &configured,
        )
        .await
        .unwrap_or_else(|error| panic!("Native provider smoke failed: {error}"));
        let results = result["results"]
            .as_array()
            .expect("Structured source results");
        assert!(!results.is_empty());
        assert!(results
            .iter()
            .all(|source| source["url"].as_str().is_some()));
        let usage = &result["usage"];
        println!(
            "{}",
            json!({
                "strategy":result["provider"], "model":result["model"], "result_count":results.len(),
                "input_tokens":usage.get("prompt_tokens").or_else(|| usage.get("input_tokens")),
                "output_tokens":usage.get("completion_tokens").or_else(|| usage.get("output_tokens")),
                "total_tokens":usage["total_tokens"], "elapsed_ms":started.elapsed().as_millis(),
            })
        );
    }

    #[tokio::test]
    async fn native_request_uses_configured_endpoint_credentials_and_reports_usage() {
        use axum::{
            extract::State,
            http::{HeaderMap, Uri},
            routing::post,
            Json, Router,
        };
        let (tx, mut rx) = tokio::sync::mpsc::channel(1);
        let app = Router::new().route("/gateway/v1/responses", post(|State(tx): State<tokio::sync::mpsc::Sender<(Uri, HeaderMap, Value)>>, uri:Uri, headers:HeaderMap, Json(body):Json<Value>| async move {
            tx.send((uri, headers, body)).await.unwrap();
            Json(json!({"output":[{"type":"web_search_call", "status":"completed", "action":{"sources":[{"url":"https://example.com", "title":"Example"}]}}],"usage":{"input_tokens":12,"output_tokens":3}}))
        })).with_state(tx);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        let result = search(
            json!({"query":"test search"}),
            &provider(&format!("http://{address}/gateway/v1"), None),
        )
        .await
        .unwrap();
        let (uri, headers, body) = rx.recv().await.unwrap();
        assert_eq!(uri.path(), "/gateway/v1/responses");
        assert_eq!(headers["authorization"], "Bearer test-key");
        assert_eq!(body["tools"][0]["type"], "web_search");
        assert_eq!(body["model"], "test-model");
        assert_eq!(result["api_format"], "openai");
        assert_eq!(result["usage"]["prompt_tokens"], 12);
        assert_eq!(result["results"][0]["url"], "https://example.com");
        server.abort();
    }
}
