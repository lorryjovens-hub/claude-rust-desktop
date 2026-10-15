//! Rate Limiter - Rate limit tracking for inference API responses
//!
//! Captures x-ratelimit-* headers from provider responses and provides
//! formatted display for usage monitoring.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateLimitBucket {
    pub limit: usize,
    pub remaining: usize,
    pub reset_seconds: f64,
    pub captured_at: f64,
}

impl RateLimitBucket {
    pub fn new() -> Self {
        Self {
            limit: 0,
            remaining: 0,
            reset_seconds: 0.0,
            captured_at: 0.0,
        }
    }

    pub fn used(&self) -> usize {
        self.limit.saturating_sub(self.remaining)
    }

    pub fn usage_pct(&self) -> f64 {
        if self.limit == 0 {
            return 0.0;
        }
        (self.used() as f64 / self.limit as f64) * 100.0
    }

    pub fn remaining_seconds_now(&self, now: f64) -> f64 {
        let elapsed = now - self.captured_at;
        (self.reset_seconds - elapsed).max(0.0)
    }
}

impl Default for RateLimitBucket {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateLimitState {
    pub requests_min: RateLimitBucket,
    pub requests_hour: RateLimitBucket,
    pub tokens_min: RateLimitBucket,
    pub tokens_hour: RateLimitBucket,
    pub captured_at: f64,
    pub provider: String,
}

impl RateLimitState {
    pub fn new() -> Self {
        Self {
            requests_min: RateLimitBucket::new(),
            requests_hour: RateLimitBucket::new(),
            tokens_min: RateLimitBucket::new(),
            tokens_hour: RateLimitBucket::new(),
            captured_at: 0.0,
            provider: String::new(),
        }
    }

    pub fn has_data(&self) -> bool {
        self.captured_at > 0.0
    }

    pub fn age_seconds(&self, now: f64) -> f64 {
        if !self.has_data() {
            return f64::INFINITY;
        }
        now - self.captured_at
    }
}

impl Default for RateLimitState {
    fn default() -> Self {
        Self::new()
    }
}

fn parse_ratelimit_value(value: &str) -> Option<usize> {
    value.parse().ok()
}

fn parse_float_value(value: &str) -> Option<f64> {
    value.parse().ok()
}

pub struct RateLimiter {
    states: Arc<RwLock<HashMap<String, RateLimitState>>>,
}

impl RateLimiter {
    pub fn new() -> Self {
        Self {
            states: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn update_from_headers(
        &self,
        provider: &str,
        headers: &HashMap<String, String>,
    ) {
        let now = chrono::Utc::now().timestamp_millis() as f64 / 1000.0;

        let mut state = RateLimitState::new();
        state.captured_at = now;
        state.provider = provider.to_string();

        let lowered: HashMap<String, String> = headers
            .keys()
            .map(|k| (k.to_lowercase(), headers.get(k).cloned().unwrap_or_default()))
            .collect();

        if let Some(v) = lowered.get("x-ratelimit-limit-requests") {
            state.requests_min.limit = parse_ratelimit_value(v).unwrap_or(0);
        }
        if let Some(v) = lowered.get("x-ratelimit-remaining-requests") {
            state.requests_min.remaining = parse_ratelimit_value(v).unwrap_or(0);
        }
        if let Some(v) = lowered.get("x-ratelimit-reset-requests") {
            state.requests_min.reset_seconds = parse_float_value(v).unwrap_or(0.0);
        }

        if let Some(v) = lowered.get("x-ratelimit-limit-requests-1h") {
            state.requests_hour.limit = parse_ratelimit_value(v).unwrap_or(0);
        }
        if let Some(v) = lowered.get("x-ratelimit-remaining-requests-1h") {
            state.requests_hour.remaining = parse_ratelimit_value(v).unwrap_or(0);
        }
        if let Some(v) = lowered.get("x-ratelimit-reset-requests-1h") {
            state.requests_hour.reset_seconds = parse_float_value(v).unwrap_or(0.0);
        }

        if let Some(v) = lowered.get("x-ratelimit-limit-tokens") {
            state.tokens_min.limit = parse_ratelimit_value(v).unwrap_or(0);
        }
        if let Some(v) = lowered.get("x-ratelimit-remaining-tokens") {
            state.tokens_min.remaining = parse_ratelimit_value(v).unwrap_or(0);
        }
        if let Some(v) = lowered.get("x-ratelimit-reset-tokens") {
            state.tokens_min.reset_seconds = parse_float_value(v).unwrap_or(0.0);
        }

        if let Some(v) = lowered.get("x-ratelimit-limit-tokens-1h") {
            state.tokens_hour.limit = parse_ratelimit_value(v).unwrap_or(0);
        }
        if let Some(v) = lowered.get("x-ratelimit-remaining-tokens-1h") {
            state.tokens_hour.remaining = parse_ratelimit_value(v).unwrap_or(0);
        }
        if let Some(v) = lowered.get("x-ratelimit-reset-tokens-1h") {
            state.tokens_hour.reset_seconds = parse_float_value(v).unwrap_or(0.0);
        }

        let mut states = self.states.write().await;
        states.insert(provider.to_string(), state);
    }

    pub async fn get_state(&self, provider: &str) -> Option<RateLimitState> {
        let states = self.states.read().await;
        states.get(provider).cloned()
    }

    pub async fn format_display(&self, provider: &str) -> String {
        let state = self.get_state(provider).await;

        let state = match state {
            Some(s) => s,
            None => return "No rate limit data yet — make an API request first.".to_string(),
        };

        let now = chrono::Utc::now().timestamp_millis() as f64 / 1000.0;
        let age = state.age_seconds(now);
        let freshness = if age < 5.0 {
            "just now".to_string()
        } else if age < 60.0 {
            format!("{}s ago", age as usize)
        } else {
            format_seconds(age)
        };

        let provider_label = if state.provider.is_empty() {
            "Provider".to_string()
        } else {
            state.provider.chars().next().unwrap().to_uppercase().collect::<String>()
                + &state.provider[1..]
        };

        let mut lines = vec![
            format!("{} Rate Limits (captured {}):", provider_label, freshness),
            String::new(),
        ];

        lines.push(format_bucket_line("Requests/min", &state.requests_min, now));
        lines.push(format_bucket_line("Requests/hour", &state.requests_hour, now));
        lines.push(format_bucket_line("Tokens/min", &state.tokens_min, now));
        lines.push(format_bucket_line("Tokens/hour", &state.tokens_hour, now));

        lines.join("\n")
    }
}

impl Default for RateLimiter {
    fn default() -> Self {
        Self::new()
    }
}

fn format_count(n: usize) -> String {
    if n >= 1_000_000 {
        format!("{:.1}M", n as f64 / 1_000_000.0)
    } else if n >= 10_000 {
        format!("{:.1}K", n as f64 / 1_000.0)
    } else if n >= 1_000 {
        format!("{:.1}K", n as f64 / 1_000.0)
    } else {
        n.to_string()
    }
}

fn format_seconds(seconds: f64) -> String {
    let s = seconds as usize;
    if s < 60 {
        format!("{}s", s)
    } else if s < 3600 {
        let m = s / 60;
        let sec = s % 60;
        if sec == 0 {
            format!("{}m", m)
        } else {
            format!("{}m {}s", m, sec)
        }
    } else {
        let h = s / 3600;
        let m = (s % 3600) / 60;
        if m == 0 {
            format!("{}h", h)
        } else {
            format!("{}h {}m", h, m)
        }
    }
}

fn format_bar(pct: f64, width: usize) -> String {
    let filled = ((pct / 100.0) * width as f64) as usize;
    let filled = filled.min(width);
    let empty = width - filled;
    format!("[{}{}]", "█".repeat(filled), "░".repeat(empty))
}

fn format_bucket_line(label: &str, bucket: &RateLimitBucket, now: f64) -> String {
    if bucket.limit == 0 {
        return format!("  {:<14}  (no data)", label);
    }

    let pct = bucket.usage_pct();
    let used = format_count(bucket.used());
    let limit = format_count(bucket.limit);
    let remaining = format_count(bucket.remaining);
    let reset = format_seconds(bucket.remaining_seconds_now(now));

    format!(
        "  {:<14} {} {:5.1}%  {}/{} used  ({} left, resets in {})",
        label,
        format_bar(pct, 20),
        pct,
        used,
        limit,
        remaining,
        reset
    )
}
