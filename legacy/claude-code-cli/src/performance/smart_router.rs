//! Smart Router - Intelligent model routing for cost optimization
//!
//! Automatically routes simple requests to cheap/fast models while
//! reserving expensive models for complex tasks.
//!
//! Detection heuristics:
//! - Message length limits
//! - Keyword analysis (code/debugging = complex)
//! - URL detection (likely simple lookup)

use serde::{Deserialize, Serialize};
use std::collections::HashSet;

const COMPLEX_KEYWORDS: &[&str] = &[
    "debug", "debugging", "implement", "implementation", "refactor", "patch",
    "traceback", "stacktrace", "exception", "error", "analyze", "analysis",
    "investigate", "architecture", "design", "compare", "benchmark",
    "optimize", "optimise", "review", "terminal", "shell", "tool", "tools",
    "pytest", "test", "tests", "plan", "planning", "delegate", "subagent",
    "cron", "docker", "kubernetes",
];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheapModel {
    pub provider: String,
    pub model: String,
    pub api_key_env: Option<String>,
    pub base_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouterConfig {
    pub enabled: bool,
    pub cheap_model: Option<CheapModel>,
    pub max_simple_chars: usize,
    pub max_simple_words: usize,
}

impl Default for RouterConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            cheap_model: None,
            max_simple_chars: 160,
            max_simple_words: 28,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouteDecision {
    pub model: String,
    pub provider: String,
    pub routing_reason: Option<String>,
    pub is_cheap_route: bool,
}

pub struct SmartRouter {
    config: RouterConfig,
}

impl SmartRouter {
    pub fn new(config: RouterConfig) -> Self {
        Self { config }
    }

    fn contains_url(text: &str) -> bool {
        text.contains("http://") || text.contains("https://") || text.contains("www.")
    }

    fn contains_complex_keywords(text: &str) -> bool {
        let lowered = text.to_lowercase();
        let words: HashSet<&str> = lowered
            .split(|c: char| !c.is_alphanumeric())
            .filter(|s| !s.is_empty())
            .collect();

        COMPLEX_KEYWORDS.iter().any(|kw| words.contains(kw))
    }

    fn is_simple_message(text: &str) -> bool {
        let trimmed = text.trim();
        if trimmed.is_empty() {
            return false;
        }

        if trimmed.len() > 160 {
            return false;
        }

        if trimmed.split_whitespace().count() > 28 {
            return false;
        }

        if trimmed.lines().count() > 1 {
            return false;
        }

        if trimmed.contains("```") || trimmed.contains('`') {
            return false;
        }

        if Self::contains_url(trimmed) {
            return false;
        }

        if Self::contains_complex_keywords(trimmed) {
            return false;
        }

        true
    }

    pub fn choose_route(&self, user_message: &str) -> Option<RouteDecision> {
        if !self.config.enabled {
            return None;
        }

        let cheap = self.config.cheap_model.as_ref()?;

        if !Self::is_simple_message(user_message) {
            return None;
        }

        Some(RouteDecision {
            model: cheap.model.clone(),
            provider: cheap.provider.clone(),
            routing_reason: Some("simple_turn".to_string()),
            is_cheap_route: true,
        })
    }

    pub fn resolve_route(
        &self,
        user_message: &str,
        primary_model: &str,
        primary_provider: &str,
    ) -> RouteDecision {
        if let Some(route) = self.choose_route(user_message) {
            return route;
        }

        RouteDecision {
            model: primary_model.to_string(),
            provider: primary_provider.to_string(),
            routing_reason: None,
            is_cheap_route: false,
        }
    }
}

impl Default for SmartRouter {
    fn default() -> Self {
        Self::new(RouterConfig::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_message() {
        assert!(SmartRouter::is_simple_message("Hello, how are you?"));
        assert!(SmartRouter::is_simple_message("What's the weather?"));
        assert!(!SmartRouter::is_simple_message("debug this code"));
        assert!(!SmartRouter::is_simple_message("implement feature x"));
    }

    #[test]
    fn test_url_detection() {
        assert!(SmartRouter::contains_url("https://example.com"));
        assert!(SmartRouter::contains_url("www.example.com"));
        assert!(!SmartRouter::contains_url("example"));
    }
}
