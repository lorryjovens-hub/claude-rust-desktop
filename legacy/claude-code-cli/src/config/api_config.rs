//! API Configuration
//! 
//! Supports multiple providers: Anthropic, OpenAI, Kimi (Moonshot), Doubao, 
//! DashScope (Qwen), xAI (Grok), and custom OpenAI-compatible endpoints.

use serde::{Deserialize, Serialize};

/// Provider kind enum
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderKind {
    Anthropic,
    OpenAi,
    Xai,
}

/// Provider metadata
#[derive(Debug, Clone, Copy)]
pub struct ProviderMetadata {
    pub provider: ProviderKind,
    pub auth_env: &'static str,
    pub base_url_env: &'static str,
    pub default_base_url: &'static str,
}

/// API configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiConfig {
    /// API key (can be set via environment variable)
    pub api_key: Option<String>,
    /// Base URL for API requests
    pub base_url: String,
    /// Maximum tokens per request
    pub max_tokens: usize,
    /// Request timeout in seconds
    pub timeout: u64,
    /// Enable streaming responses
    pub streaming: bool,
    /// Beta headers to include
    pub beta_headers: Vec<String>,
}

impl Default for ApiConfig {
    fn default() -> Self {
        Self {
            api_key: None,
            base_url: "https://api.anthropic.com".to_string(),
            max_tokens: 4096,
            timeout: 120,
            streaming: true,
            beta_headers: vec![],
        }
    }
}

impl ApiConfig {
    /// Resolve model alias to canonical model name
    pub fn resolve_model_alias(model: &str) -> String {
        let trimmed = model.trim();
        let lower = trimmed.to_ascii_lowercase();
        
        match lower.as_str() {
            "opus" => "claude-opus-4-6".to_string(),
            "sonnet" => "claude-sonnet-4-6".to_string(),
            "haiku" => "claude-haiku-4-5-20251213".to_string(),
            "grok" => "grok-3".to_string(),
            "grok-mini" => "grok-3-mini".to_string(),
            _ => trimmed.to_string(),
        }
    }

    /// Get provider metadata for a model
    pub fn get_provider_metadata(model: &str) -> Option<ProviderMetadata> {
        let canonical = Self::resolve_model_alias(model);
        
        // Anthropic models
        if canonical.starts_with("claude") {
            return Some(ProviderMetadata {
                provider: ProviderKind::Anthropic,
                auth_env: "ANTHROPIC_API_KEY",
                base_url_env: "ANTHROPIC_BASE_URL",
                default_base_url: "https://api.anthropic.com",
            });
        }
        
        // xAI Grok models
        if canonical.starts_with("grok") {
            return Some(ProviderMetadata {
                provider: ProviderKind::Xai,
                auth_env: "XAI_API_KEY",
                base_url_env: "XAI_BASE_URL",
                default_base_url: "https://api.x.ai/v1",
            });
        }
        
        // OpenAI models
        if canonical.starts_with("openai/") || canonical.starts_with("gpt-") {
            return Some(ProviderMetadata {
                provider: ProviderKind::OpenAi,
                auth_env: "OPENAI_API_KEY",
                base_url_env: "OPENAI_BASE_URL",
                default_base_url: "https://api.openai.com/v1",
            });
        }
        
        // DashScope Qwen models
        if canonical.starts_with("qwen/") || canonical.starts_with("qwen-") {
            return Some(ProviderMetadata {
                provider: ProviderKind::OpenAi,
                auth_env: "DASHSCOPE_API_KEY",
                base_url_env: "DASHSCOPE_BASE_URL",
                default_base_url: "https://dashscope.aliyuncs.com/compatible-mode/v1",
            });
        }
        
        // Kimi Code (Moonshot) models
        if canonical.starts_with("moonshot/") || canonical.starts_with("moonshot-") {
            return Some(ProviderMetadata {
                provider: ProviderKind::OpenAi,
                auth_env: "OPENAI_API_KEY",
                base_url_env: "OPENAI_BASE_URL",
                default_base_url: "https://api.moonshot.cn/v1",
            });
        }
        
        // Doubao models
        if canonical.starts_with("doubao/") || canonical.starts_with("doubao-") {
            return Some(ProviderMetadata {
                provider: ProviderKind::OpenAi,
                auth_env: "OPENAI_API_KEY",
                base_url_env: "OPENAI_BASE_URL",
                default_base_url: "https://api.doubao.com/v1",
            });
        }
        
        None
    }

    /// Detect provider kind for a model
    pub fn detect_provider_kind(model: &str) -> ProviderKind {
        if let Some(metadata) = Self::get_provider_metadata(model) {
            return metadata.provider;
        }
        
        // Check environment variables for fallback
        if std::env::var_os("OPENAI_BASE_URL").is_some() && Self::has_api_key("OPENAI_API_KEY") {
            return ProviderKind::OpenAi;
        }
        if Self::has_auth_from_env("ANTHROPIC_API_KEY") {
            return ProviderKind::Anthropic;
        }
        if Self::has_api_key("OPENAI_API_KEY") {
            return ProviderKind::OpenAi;
        }
        if Self::has_api_key("XAI_API_KEY") {
            return ProviderKind::Xai;
        }
        if std::env::var_os("OPENAI_BASE_URL").is_some() {
            return ProviderKind::OpenAi;
        }
        
        ProviderKind::Anthropic
    }

    /// Check if an API key environment variable is set
    fn has_api_key(env_var: &str) -> bool {
        std::env::var(env_var).map(|v| !v.is_empty()).unwrap_or(false)
    }

    /// Check if Anthropic auth is available
    fn has_auth_from_env(env_var: &str) -> bool {
        Self::has_api_key(env_var)
    }

    /// Get the API key for a provider
    pub fn get_api_key(&self, model: &str) -> Option<String> {
        let metadata = Self::get_provider_metadata(model);
        
        if let Some(meta) = metadata {
            std::env::var(meta.auth_env).ok()
        } else {
            // Fallback to common env vars
            std::env::var("ANTHROPIC_API_KEY").ok()
                .or(std::env::var("OPENAI_API_KEY").ok())
                .or(std::env::var("DASHSCOPE_API_KEY").ok())
                .or(self.api_key.clone())
        }
    }

    /// Get the API key (backward compatible - checks all common env vars)
    pub fn get_api_key_legacy(&self) -> Option<String> {
        std::env::var("ANTHROPIC_API_KEY").ok()
            .or(std::env::var("OPENAI_API_KEY").ok())
            .or(std::env::var("DASHSCOPE_API_KEY").ok())
            .or(std::env::var("XAI_API_KEY").ok())
            .or(self.api_key.clone())
    }

    /// Get the base URL for a provider
    pub fn get_base_url(&self, model: &str) -> String {
        let metadata = Self::get_provider_metadata(model);
        
        if let Some(meta) = metadata {
            std::env::var(meta.base_url_env)
                .unwrap_or_else(|_| meta.default_base_url.to_string())
        } else {
            std::env::var("API_BASE_URL")
                .unwrap_or_else(|_| self.base_url.clone())
        }
    }

    /// Get the base URL (backward compatible)
    pub fn get_base_url_legacy(&self) -> String {
        std::env::var("API_BASE_URL")
            .or_else(|_| std::env::var("ANTHROPIC_BASE_URL"))
            .or_else(|_| std::env::var("OPENAI_BASE_URL"))
            .unwrap_or_else(|_| self.base_url.clone())
    }

    /// Get the model ID for the given model name
    pub fn get_model_id(&self, model: &str) -> String {
        let canonical = Self::resolve_model_alias(model);
        
        // For Anthropic models, use the resolved canonical name
        if canonical.starts_with("claude") {
            match canonical.as_str() {
                "claude-opus-4-6" => "claude-3-opus-20240229".to_string(),
                "claude-sonnet-4-6" => "claude-3-5-sonnet-20241022".to_string(),
                "claude-haiku-4-5-20251213" => "claude-3-5-haiku-20241022".to_string(),
                _ => canonical,
            }
        } else {
            canonical
        }
    }

    /// Get max tokens for a model
    pub fn get_max_tokens_for_model(&self, model: &str) -> usize {
        let canonical = Self::resolve_model_alias(model);
        
        if canonical.contains("opus") {
            32_000
        } else if canonical.contains("grok") {
            64_000
        } else {
            self.max_tokens
        }
    }
}