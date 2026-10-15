//! API Module - Multi-provider API client support
//!
//! Supports: Anthropic, OpenAI, Kimi (Moonshot), Doubao, DashScope (Qwen), xAI (Grok)

pub mod providers;

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ApiError {
    #[error("Request failed: {0}")]
    RequestError(String),
    #[error("Authentication failed")]
    AuthError,
    #[error("Invalid response: {0}")]
    InvalidResponse(String),
    #[error("Model not found: {0}")]
    ModelNotFound(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiConfig {
    pub api_key: String,
    pub base_url: String,
    pub model: String,
}

pub use providers::{get_provider_for_model, ModelProvider};