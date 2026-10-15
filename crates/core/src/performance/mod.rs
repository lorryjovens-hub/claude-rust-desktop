//! Shared transport policy. Retries end before a response stream is exposed.
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ErrorClass {
    Authentication,
    RateLimit,
    ContextLimit,
    Transient,
    Permanent,
}

#[derive(Debug)]
pub struct ApiError {
    pub status: u16,
    pub retry_after: Option<Duration>,
    pub message: String,
}
impl std::fmt::Display for ApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Model API error {}: {}", self.status, self.message)
    }
}
impl std::error::Error for ApiError {}

pub fn classify(error: &anyhow::Error) -> ErrorClass {
    if let Some(api) = error.downcast_ref::<ApiError>() {
        return match api.status {
            401 | 403 => ErrorClass::Authentication,
            429 => ErrorClass::RateLimit,
            408 | 500..=599 => ErrorClass::Transient,
            400 | 413 if api.message.to_lowercase().contains("context") || api.status == 413 => {
                ErrorClass::ContextLimit
            }
            _ => ErrorClass::Permanent,
        };
    }
    if let Some(http) = error.downcast_ref::<reqwest::Error>() {
        if http.is_timeout() || http.is_connect() {
            return ErrorClass::Transient;
        }
    }
    ErrorClass::Permanent
}

pub async fn checked_response(response: reqwest::Response) -> Result<reqwest::Response> {
    observe_rate_limits(&response);
    if response.status().is_success() {
        return Ok(response);
    }
    let status = response.status().as_u16();
    let retry_after = response
        .headers()
        .get("retry-after")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<u64>().ok())
        .map(|seconds| Duration::from_secs(seconds.min(300)));
    let message = response.text().await.unwrap_or_default();
    Err(ApiError {
        status,
        retry_after,
        message,
    }
    .into())
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RateLimitState {
    pub origin: String,
    pub requests_remaining: Option<u64>,
    pub tokens_remaining: Option<u64>,
    pub requests_reset: Option<String>,
    pub observed_at: String,
}
static RATE_LIMITS: std::sync::LazyLock<
    std::sync::Mutex<std::collections::BTreeMap<String, RateLimitState>>,
> = std::sync::LazyLock::new(Default::default);

fn observe_rate_limits(response: &reqwest::Response) {
    let headers = response.headers();
    let number = |names: &[&str]| {
        names
            .iter()
            .find_map(|name| headers.get(*name)?.to_str().ok()?.parse::<u64>().ok())
    };
    let requests_remaining = number(&[
        "anthropic-ratelimit-requests-remaining",
        "x-ratelimit-remaining-requests",
    ]);
    let tokens_remaining = number(&[
        "anthropic-ratelimit-tokens-remaining",
        "x-ratelimit-remaining-tokens",
    ]);
    if requests_remaining.is_none() && tokens_remaining.is_none() {
        return;
    }
    let origin = response.url().origin().ascii_serialization();
    let requests_reset = [
        "anthropic-ratelimit-requests-reset",
        "x-ratelimit-reset-requests",
    ]
    .iter()
    .find_map(|name| {
        headers
            .get(*name)?
            .to_str()
            .ok()
            .map(|v| v.chars().take(80).collect())
    });
    if let Ok(mut states) = RATE_LIMITS.lock() {
        if states.len() >= 128 && !states.contains_key(&origin) {
            states.pop_first();
        }
        states.insert(
            origin.clone(),
            RateLimitState {
                origin,
                requests_remaining,
                tokens_remaining,
                requests_reset,
                observed_at: chrono::Utc::now().to_rfc3339(),
            },
        );
    }
}
pub fn rate_limits() -> Vec<RateLimitState> {
    RATE_LIMITS
        .lock()
        .map(|states| states.values().cloned().collect())
        .unwrap_or_default()
}

pub async fn retry_with_backoff<F, Fut, T>(mut f: F) -> Result<T>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Result<T>>,
{
    crate::config::OrchestrationConfig::init_global()?;
    let max = crate::config::OrchestrationConfig::max_retries();
    let base = crate::config::OrchestrationConfig::retry_base_backoff_ms();
    for attempt in 0..=max {
        match f().await {
            Ok(value) => return Ok(value),
            Err(error) => {
                if attempt == max
                    || !matches!(
                        classify(&error),
                        ErrorClass::RateLimit | ErrorClass::Transient
                    )
                {
                    return Err(error);
                }
                let delay = error
                    .downcast_ref::<ApiError>()
                    .and_then(|e| e.retry_after)
                    .unwrap_or_else(|| {
                        Duration::from_millis(
                            base.saturating_mul(1u64 << attempt.min(16)).min(300_000),
                        )
                    });
                // Do not log remote bodies: they can echo prompt or credential data.
                tracing::warn!(attempt, ?delay, "Retrying transient model request");
                tokio::time::sleep(delay).await;
            }
        }
    }
    unreachable!()
}

/// Mark a stable system prefix only. No cache of generated answers is created.
pub fn apply_prompt_cache(body: &mut serde_json::Value) {
    if let Some(system) = body.get("system").and_then(|s| s.as_str()) {
        if system.len() >= 4096 {
            body["system"] = serde_json::json!([{"type":"text","text":system,"cache_control":{"type":"ephemeral"}}]);
        }
    }
}

pub fn retain_tool_output(root: &std::path::Path, output: &str, preview: &str) -> String {
    const LIMIT: usize = 16_000;
    if output == preview && output.len() <= LIMIT {
        return output.to_string();
    }
    let persist = || -> std::io::Result<std::path::PathBuf> {
        use std::io::Write;
        let directory = root.join("tool-outputs");
        std::fs::create_dir_all(&directory)?;
        let path = directory.join(format!("{}.txt", uuid::Uuid::new_v4()));
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)?;
        file.write_all(output.as_bytes())?;
        file.sync_all()?;
        Ok(path)
    };
    match persist() {
        Ok(path) => {
            let mut end = preview.len().min(LIMIT);
            while !preview.is_char_boundary(end) {
                end -= 1;
            }
            format!(
                "{}\nFull tool output ({} bytes) saved at: {}",
                &preview[..end],
                output.len(),
                path.display()
            )
        }
        Err(_) => output.to_string(), // Never discard the only complete copy.
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RouterConfig {
    pub enabled: bool,
    pub provider_id: String,
    pub model_id: String,
}
pub fn validate_router(ctx: &crate::services::ServiceContext, config: &RouterConfig) -> Result<()> {
    if config.enabled {
        anyhow::ensure!(
            ctx.paths
                .providers()?
                .iter()
                .any(|p| p.id == config.provider_id
                    && p.enabled
                    && p.models
                        .iter()
                        .any(|m| m.id == config.model_id && m.enabled)),
            "Routing target must be an enabled configured provider and model"
        );
    }
    Ok(())
}
pub fn configure_router(
    ctx: &crate::services::ServiceContext,
    config: &RouterConfig,
) -> Result<()> {
    validate_router(ctx, config)?;
    ctx.save_setting("performance.router", config)
}
pub fn simple_turn(text: &str) -> bool {
    let text = text.trim();
    if text.is_empty()
        || text.chars().count() > 160
        || text.split_whitespace().count() > 28
        || text.lines().count() > 1
        || text.contains('`')
        || text.contains("http")
        || text.contains("www.")
    {
        return false;
    }
    let lower = text.to_lowercase();
    ![
        "debug",
        "implement",
        "refactor",
        "error",
        "analy",
        "design",
        "review",
        "test",
        "plan",
        "tool",
        "shell",
        "write",
        "read",
        "file",
        "delete",
        "代码",
        "修改",
        "文件",
        "删除",
        "分析",
        "实现",
        "测试",
    ]
    .iter()
    .any(|word| lower.contains(word))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn permanent_errors_containing_five_are_not_retried() {
        for status in [400, 401, 403, 404, 422] {
            let error = ApiError {
                status,
                retry_after: None,
                message: "model 5 rejected input 500".into(),
            }
            .into();
            assert!(!matches!(
                classify(&error),
                ErrorClass::Transient | ErrorClass::RateLimit
            ));
        }
        assert_eq!(
            classify(
                &ApiError {
                    status: 503,
                    retry_after: None,
                    message: String::new()
                }
                .into()
            ),
            ErrorClass::Transient
        );
    }
    #[tokio::test]
    async fn permanent_error_runs_once() {
        let count = std::sync::atomic::AtomicUsize::new(0);
        let result: Result<()> = retry_with_backoff(|| {
            count.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            async { Err(anyhow::anyhow!("invalid model 5")) }
        })
        .await;
        assert!(result.is_err());
        assert_eq!(count.load(std::sync::atomic::Ordering::SeqCst), 1);
    }
    #[test]
    fn cache_preserves_system_text() {
        let text = "你好".repeat(1000);
        let mut body = serde_json::json!({"system":text,"messages":[]});
        apply_prompt_cache(&mut body);
        assert_eq!(body["system"][0]["text"], text);
        assert_eq!(body["system"][0]["cache_control"]["type"], "ephemeral");
    }
}
