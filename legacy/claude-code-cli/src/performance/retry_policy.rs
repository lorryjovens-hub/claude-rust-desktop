//! Retry Policy - Jittered backoff for decorrelated retries
//!
//! Replaces fixed exponential backoff with jittered delays to prevent
//! thundering-herd retry spikes when multiple sessions hit the same
//! rate-limited provider concurrently.

use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetryConfig {
    pub max_attempts: usize,
    pub base_delay_secs: f64,
    pub max_delay_secs: f64,
    pub jitter_ratio: f64,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_attempts: 5,
            base_delay_secs: 5.0,
            max_delay_secs: 120.0,
            jitter_ratio: 0.5,
        }
    }
}

pub struct RetryPolicy {
    config: RetryConfig,
    jitter_counter: Arc<AtomicU64>,
}

impl RetryPolicy {
    pub fn new(config: RetryConfig) -> Self {
        Self {
            config,
            jitter_counter: Arc::new(AtomicU64::new(0)),
        }
    }

    pub fn should_retry(&self, attempt: usize, error: &str) -> bool {
        if attempt >= self.config.max_attempts {
            return false;
        }

        let lower = error.to_lowercase();
        if lower.contains("rate limit") || lower.contains("429") {
            return true;
        }

        if lower.contains("timeout") || lower.contains("connection") {
            return true;
        }

        if lower.contains("server error") || lower.contains("500") {
            return true;
        }

        if lower.contains("service unavailable") || lower.contains("503") {
            return true;
        }

        attempt < self.config.max_attempts
    }

    pub fn calculate_delay(&self, attempt: usize) -> f64 {
        let tick = self.jitter_counter.fetch_add(1, Ordering::SeqCst);

        let exponent = attempt.saturating_sub(1).max(0);
        let delay = if exponent >= 63 || self.config.base_delay_secs <= 0.0 {
            self.config.max_delay_secs
        } else {
            (self.config.base_delay_secs * 2_f64.powi(exponent as i32))
                .min(self.config.max_delay_secs)
        };

        let seed = (std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0)
            ^ (tick.wrapping_mul(0x9E3779B9)))
        & 0xFFFFFFFF;

        let jitter_range = delay * self.config.jitter_ratio;
        let jitter = Self::uniform_f64(seed, jitter_range);

        delay + jitter
    }

    fn uniform_f64(seed: u64, range: f64) -> f64 {
        use std::hash::{Hash, Hasher};
        use std::collections::hash_map::DefaultHasher;

        if range <= 0.0 {
            return 0.0;
        }

        let mut hasher = DefaultHasher::new();
        seed.hash(&mut hasher);
        let hash = hasher.finish() as f64 / u64::MAX as f64;

        hash * range
    }

    pub async fn execute_with_retry<F, Fut, T>(&self, mut f: F) -> Result<T, String>
    where
        F: FnMut() -> Fut,
        Fut: std::future::Future<Output = Result<T, String>>,
    {
        let mut attempt = 0;
        let mut last_error = String::new();

        loop {
            match f().await {
                Ok(result) => return Ok(result),
                Err(e) => {
                    last_error = e;
                    if !self.should_retry(attempt, &last_error) {
                        return Err(last_error);
                    }

                    let delay = self.calculate_delay(attempt);
                    tokio::time::sleep(tokio::time::Duration::from_secs_f64(delay)).await;
                    attempt += 1;
                }
            }
        }
    }
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self::new(RetryConfig::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_retry_logic() {
        let policy = RetryPolicy::default();

        assert!(policy.should_retry(0, "rate limit exceeded"));
        assert!(policy.should_retry(4, "connection timeout"));
        assert!(!policy.should_retry(5, "rate limit exceeded"));
        assert!(!policy.should_retry(0, "invalid api key"));
    }

    #[test]
    fn test_delay_increases() {
        let policy = RetryPolicy::default();
        let delay0 = policy.calculate_delay(0);
        let delay1 = policy.calculate_delay(1);
        assert!(delay1 >= delay0);
    }
}
