//! Config Module
//!
//! Configuration loading and validation.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub model: String,
    pub base_url: Option<String>,
    pub api_key: Option<String>,
    pub permission_mode: String,
    pub read_only: bool,
    pub allow_dangerous: bool,
    pub workspace: Option<String>,
    pub max_tokens: Option<u32>,
    pub temperature: Option<f32>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            model: "claude-sonnet-4-20250514".to_string(),
            base_url: None,
            api_key: None,
            permission_mode: "normal".to_string(),
            read_only: false,
            allow_dangerous: false,
            workspace: None,
            max_tokens: None,
            temperature: None,
        }
    }
}

impl Config {
    pub fn load() -> Result<Self, ConfigError> {
        let config_paths = [
            PathBuf::from(".claude.json"),
            PathBuf::from("~/.claude.json"),
            PathBuf::from(".config/claude/config.json"),
        ];

        for path in &config_paths {
            if path.exists() {
                let content = std::fs::read_to_string(path)
                    .map_err(|e| ConfigError::IoError(e.to_string()))?;
                let config: Config = serde_json::from_str(&content)
                    .map_err(|e| ConfigError::ParseError(e.to_string()))?;
                return Ok(config);
            }
        }

        Ok(Config::default())
    }

    pub fn validate(&self) -> Result<(), ConfigError> {
        if self.model.is_empty() {
            return Err(ConfigError::ValidationError("model cannot be empty".to_string()));
        }
        Ok(())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("Failed to read config: {0}")]
    IoError(String),
    #[error("Failed to parse config: {0}")]
    ParseError(String),
    #[error("Invalid config: {0}")]
    ValidationError(String),
}