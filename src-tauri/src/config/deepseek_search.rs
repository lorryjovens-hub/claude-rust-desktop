//! Independent credentials and endpoint for the DeepSeek native search service.
use crate::native_engine::provider_manager::{ApiFormat, ModelConfig, Provider, ResolvedProvider};
use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};

#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct DeepSeekSearchConfig {
    pub base_url: String,
    pub api_key: String,
    pub model: String,
}

impl std::fmt::Debug for DeepSeekSearchConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DeepSeekSearchConfig")
            .field("base_url", &self.base_url)
            .field("model", &self.model)
            .field("has_api_key", &!self.api_key.is_empty())
            .finish()
    }
}

impl Default for DeepSeekSearchConfig {
    fn default() -> Self {
        Self {
            base_url: "https://api.deepseek.com/anthropic/v1".into(),
            api_key: String::new(),
            model: "deepseek-flash".into(),
        }
    }
}

impl DeepSeekSearchConfig {
    pub fn validate(&self) -> Result<()> {
        let url = reqwest::Url::parse(self.base_url.trim())
            .map_err(|_| anyhow::anyhow!("搜索 API 地址必须是有效的 HTTP(S) URL"))?;
        if !matches!(url.scheme(), "http" | "https")
            || url.host_str().is_none()
            || !url.username().is_empty()
            || url.password().is_some()
            || url.query().is_some()
            || url.fragment().is_some()
        {
            bail!("搜索 API 地址必须使用 HTTP(S)，且不能包含用户名、密码、查询参数或片段");
        }
        if self.base_url.len() > 4096 {
            bail!("搜索 API 地址过长");
        }
        if self.model.trim().is_empty()
            || self.model.len() > 256
            || self.model.chars().any(char::is_whitespace)
            || self.model.chars().any(char::is_control)
        {
            bail!("搜索模型不能为空，且不能包含空白或控制字符");
        }
        if self.api_key.len() > 8192 || self.api_key.chars().any(char::is_control) {
            bail!("搜索 API Key 格式无效");
        }
        Ok(())
    }

    pub fn resolved_provider(&self) -> Result<ResolvedProvider> {
        if self.api_key.trim().is_empty() {
            bail!("请在系统设置 → 联网搜索中配置 DeepSeek API Key、搜索地址和模型；搜索不会复用对话的密钥或地址。");
        }
        self.validate()?;
        let model = ModelConfig {
            id: self.model.trim().into(),
            name: self.model.trim().into(),
            enabled: true,
            max_tokens: Some(2048),
            supports_vision: false,
            supports_web_search: true,
            context_window: None,
            supports_text: true,
            api: Some("anthropic-messages".into()),
            effort: None,
        };
        Ok(ResolvedProvider {
            provider: Provider {
                id: "deepseek-search".into(),
                name: "DeepSeek Search".into(),
                base_url: self.base_url.trim().trim_end_matches('/').into(),
                api_key: self.api_key.trim().into(),
                api_format: ApiFormat::Anthropic,
                // Deliberately absent: no chat or environment credential fallback.
                builtin_id: None,
                models: vec![model.clone()],
                enabled: true,
                web_search_strategy: Some("deepseek_native".into()),
            },
            model,
        })
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn old_config_gets_search_defaults_and_independent_key_is_required() {
        let config: crate::config::AppConfig = serde_json::from_str(r#"{"providers":[]}"#).unwrap();
        assert_eq!(config.deepseek_search.model, "deepseek-flash");
        assert!(config
            .deepseek_search
            .resolved_provider()
            .unwrap_err()
            .to_string()
            .contains("系统设置 → 联网搜索"));
        let mut search = config.deepseek_search;
        search.api_key = "independent-search-key".into();
        search.base_url = "https://search.example/anthropic/v1".into();
        search.model = "chosen-search-model".into();
        let resolved = search.resolved_provider().unwrap();
        assert_eq!(resolved.provider.id, "deepseek-search");
        assert_eq!(resolved.provider.api_key, "independent-search-key");
        assert_eq!(resolved.provider.base_url, search.base_url);
        assert_eq!(resolved.model.id, "chosen-search-model");
        assert!(resolved.provider.builtin_id.is_none());
        assert!(!format!("{search:?}").contains("independent-search-key"));
    }
}
