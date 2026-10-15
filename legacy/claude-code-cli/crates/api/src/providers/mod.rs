//! API Providers Module
//!
//! Routes model requests to appropriate API providers.

use crate::ApiError;

#[derive(Debug, Clone)]
pub enum ModelProvider {
    Anthropic,
    OpenAI,
    Kimi,
    Doubao,
    DashScope,
    XAI,
    Unknown,
}

pub fn get_provider_for_model(model: &str) -> ModelProvider {
    let model_lower = model.to_lowercase();

    if model_lower.starts_with("claude") || model_lower.starts_with("anthropic") {
        ModelProvider::Anthropic
    } else if model_lower.starts_with("gpt") || model_lower.starts_with("openai") {
        ModelProvider::OpenAI
    } else if model_lower.starts_with("moonshot") || model_lower.starts_with("kimi") {
        ModelProvider::Kimi
    } else if model_lower.starts_with("doubao") || model_lower.contains("seed") {
        ModelProvider::Doubao
    } else if model_lower.starts_with("qwen") || model_lower.starts_with("dashscope") {
        ModelProvider::DashScope
    } else if model_lower.starts_with("xai") || model_lower.starts_with("grok") {
        ModelProvider::XAI
    } else {
        ModelProvider::Unknown
    }
}

pub fn get_auth_env_for_provider(provider: &ModelProvider) -> &'static str {
    match provider {
        ModelProvider::Anthropic => "ANTHROPIC_API_KEY",
        ModelProvider::OpenAI => "OPENAI_API_KEY",
        ModelProvider::Kimi => "OPENAI_API_KEY",
        ModelProvider::Doubao => "OPENAI_API_KEY",
        ModelProvider::DashScope => "DASHSCOPE_API_KEY",
        ModelProvider::XAI => "XAI_API_KEY",
        ModelProvider::Unknown => "OPENAI_API_KEY",
    }
}

pub fn get_base_url_env_for_provider(provider: &ModelProvider) -> &'static str {
    match provider {
        ModelProvider::Anthropic => "ANTHROPIC_BASE_URL",
        ModelProvider::Kimi => "OPENAI_BASE_URL",
        ModelProvider::Doubao => "ARK_BASE_URL",
        ModelProvider::DashScope => "DASHSCOPE_BASE_URL",
        ModelProvider::XAI => "XAI_BASE_URL",
        _ => "OPENAI_BASE_URL",
    }
}

pub fn get_default_base_url(provider: &ModelProvider) -> &'static str {
    match provider {
        ModelProvider::Anthropic => "https://api.anthropic.com",
        ModelProvider::Kimi => "https://api.moonshot.cn/v1",
        ModelProvider::Doubao => "https://ark.cn-beijing.volces.com/api/v3",
        ModelProvider::DashScope => "https://dashscope.aliyuncs.com/compatible-mode/v1",
        ModelProvider::XAI => "https://api.x.ai/v1",
        _ => "https://api.openai.com/v1",
    }
}