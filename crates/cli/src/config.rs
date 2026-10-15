use crate::{
    args::{ApiFormatArg, Cli, ConfigCommands},
    legacy_api_config::ApiConfig,
};
use anyhow::{bail, ensure, Context, Result};
use claude_core::{
    config::{ConfigManager, ModelConfig, ProviderConfig},
    native_engine::provider_manager::{ApiFormat, Provider},
    runtime::{provider_from_config, RuntimePaths},
};
use serde_json::{json, Value};
use std::{collections::HashMap, path::Path};

pub struct Selection {
    pub providers: Vec<Provider>,
    pub model: String,
    pub provider_id: String,
}

fn preferences(paths: &RuntimePaths) -> Result<Value> {
    let path = paths.data_dir.join("cli.json");
    if !path.exists() {
        return Ok(json!({}));
    }
    Ok(serde_json::from_str(&std::fs::read_to_string(path)?)?)
}

fn save_preferences(paths: &RuntimePaths, value: &Value) -> Result<()> {
    std::fs::create_dir_all(&paths.data_dir)?;
    std::fs::write(
        paths.data_dir.join("cli.json"),
        serde_json::to_vec_pretty(value)?,
    )?;
    Ok(())
}

pub(crate) fn provider_template(model: &str) -> ProviderConfig {
    let model = ApiConfig::resolve_model_alias(model);
    let metadata = ApiConfig::get_provider_metadata(&model);
    let anthropic = model.starts_with("claude");
    ProviderConfig {
        id: "cli".into(),
        name: "CLI".into(),
        provider_type: if anthropic { "anthropic" } else { "openai" }.into(),
        api_key: None,
        base_url: metadata
            .map(|m| m.default_base_url)
            .unwrap_or(if model.starts_with("deepseek") {
                "https://api.deepseek.com"
            } else {
                "https://api.openai.com/v1"
            })
            .into(),
        models: vec![ModelConfig {
            id: model.clone(),
            name: model,
            enabled: true,
            max_tokens: Some(8192),
            supports_vision: false,
            supports_tools: true,
            supports_streaming: true,
            context_window: None,
            supports_text: true,
            api: None,
            cost_per_1k_input: None,
            cost_per_1k_output: None,
        }],
        enabled: true,
        is_default: false,
        settings: HashMap::new(),
        supports_web_search: false,
        web_search_strategy: None,
        web_search_tested_at: None,
        web_search_test_reason: None,
    }
}

pub fn select(
    cli: &Cli,
    paths: &RuntimePaths,
    resumed_model: Option<&str>,
    resumed_provider: Option<&str>,
) -> Result<Selection> {
    let prefs = preferences(paths)?;
    let config = paths.read_config()?;
    let mut providers = paths.providers()?;
    let default_provider = config.default_provider_id.as_deref().or_else(|| {
        config
            .providers
            .iter()
            .find(|p| p.is_default)
            .map(|p| p.id.as_str())
    });
    let selected_provider = cli
        .provider
        .as_deref()
        .or(resumed_provider)
        .or(default_provider);
    let first_model = providers
        .iter()
        .filter(|p| p.enabled && selected_provider.map_or(true, |id| p.id == id))
        .flat_map(|p| &p.models)
        .find(|m| m.enabled)
        .map(|m| m.id.as_str());
    let model = cli
        .model
        .as_deref()
        .or(resumed_model)
        .or(prefs["model"].as_str())
        .or(first_model)
        .unwrap_or(if std::env::var_os("DEEPSEEK_API_KEY").is_some() {
            "deepseek-chat"
        } else {
            "sonnet"
        });
    let (qualified_provider, raw_model) = model
        .split_once("::")
        .map(|(p, m)| (Some(p), m))
        .unwrap_or((None, model));
    if let (Some(explicit), Some(qualified)) = (cli.provider.as_deref(), qualified_provider) {
        ensure!(
            explicit == qualified,
            "--provider conflicts with qualified model ID"
        );
    }
    let selected_provider = qualified_provider.or(selected_provider);
    let model = if providers
        .iter()
        .any(|p| p.models.iter().any(|m| m.id == raw_model))
    {
        raw_model.to_owned()
    } else {
        ApiConfig::resolve_model_alias(raw_model)
    };
    let matching: Vec<usize> = providers
        .iter()
        .enumerate()
        .filter(|(_, p)| {
            p.enabled
                && selected_provider.map_or(true, |id| p.id == id)
                && p.models.iter().any(|m| m.enabled && m.id == model)
        })
        .map(|(i, _)| i)
        .collect();
    ensure!(
        matching.len() <= 1,
        "Model exists in several providers; select --provider or provider::model"
    );
    let metadata = ApiConfig::get_provider_metadata(&model);
    let builtin_key_env = matching
        .first()
        .and_then(|index| providers[*index].builtin_id.as_deref())
        .map(claude_core::native_engine::provider_runtime::credential_env);
    let configured_key_env = matching
        .first()
        .and_then(|index| {
            config
                .providers
                .iter()
                .find(|p| p.id == providers[*index].id)
        })
        .and_then(|p| p.settings.get("apiKeyEnv"))
        .and_then(Value::as_str);
    let key_env = cli
        .api_key_env
        .as_deref()
        .or(configured_key_env)
        .or(builtin_key_env.as_deref())
        .unwrap_or_else(|| {
            if model.starts_with("deepseek") {
                "DEEPSEEK_API_KEY"
            } else {
                metadata.map(|m| m.auth_env).unwrap_or("OPENAI_API_KEY")
            }
        });
    let key = std::env::var(key_env).ok().filter(|s| !s.is_empty());
    if cli.api_key_env.is_some() || configured_key_env.is_some() {
        ensure!(
            key.is_some(),
            "API key environment variable {key_env} is empty or unset"
        );
    }
    let env_url = if model.starts_with("deepseek") {
        std::env::var("DEEPSEEK_BASE_URL").ok()
    } else {
        metadata.and_then(|m| std::env::var(m.base_url_env).ok())
    };
    let base_url = cli.base_url.clone().or(env_url);
    let index = if let Some(index) = matching.first() {
        *index
    } else {
        ensure!(
            selected_provider.is_none() || selected_provider == Some("cli-runtime"),
            "Selected provider does not contain enabled model {model}"
        );
        let mut p = provider_from_config(&provider_template(&model));
        p.id = "cli-runtime".into();
        providers.push(p);
        providers.len() - 1
    };
    let p = &mut providers[index];
    if let Some(key) = key {
        p.api_key = key;
    }
    if let Some(url) = base_url {
        p.base_url = url;
        p.builtin_id = None;
    }
    if let Some(format) = cli.api_format {
        p.api_format = match format {
            ApiFormatArg::Anthropic => ApiFormat::Anthropic,
            ApiFormatArg::Openai => ApiFormat::OpenAI,
        };
        p.builtin_id = None;
    }
    ensure!(!p.api_key.trim().is_empty() || matches!(p.builtin_id.as_deref(),Some("amazon-bedrock"|"google-vertex")), "No API key configured; set {key_env}, use --api-key-env, or configure a provider in Desktop");
    if let Some(limit) = cli.max_tokens {
        p.models
            .iter_mut()
            .filter(|m| m.id == model)
            .for_each(|m| m.max_tokens = Some(limit));
    }
    let provider_id = p.id.clone();
    Ok(Selection {
        providers,
        model,
        provider_id,
    })
}

pub fn redact(value: &mut Value) {
    match value {
        Value::Object(fields) => {
            for (key, value) in fields {
                let key = key.to_ascii_lowercase();
                if key.contains("key")
                    || key.ends_with("_token")
                    || key == "token"
                    || key.contains("secret")
                    || key.contains("password")
                    || key == "env"
                {
                    if !value.is_null() {
                        *value = json!("[redacted]");
                    }
                } else {
                    redact(value);
                }
            }
        }
        Value::Array(values) => values.iter_mut().for_each(redact),
        _ => {}
    }
}

pub fn run(paths: &RuntimePaths, action: &ConfigCommands) -> Result<()> {
    paths.read_config()?; // Never overwrite a malformed Desktop config with defaults.
    match action {
        ConfigCommands::Show => {
            let mut value = json!({"data_dir":paths.data_dir,"preferences":preferences(paths)?,"config":paths.read_config()?});
            redact(&mut value);
            println!("{}", serde_json::to_string_pretty(&value)?);
        }
        ConfigCommands::Set { key, value } => {
            ensure!(
                ["model", "base_url", "api_format", "api_key", "max_tokens"]
                    .contains(&key.as_str()),
                "Unsupported setting: {key}"
            );
            let mut prefs = preferences(paths)?;
            let mut manager = ConfigManager::new(paths.data_dir.clone());
            let model = if key == "model" {
                value.as_str()
            } else {
                prefs["model"].as_str().unwrap_or("sonnet")
            };
            let mut p = manager
                .get_provider("cli")
                .cloned()
                .unwrap_or_else(|| provider_template(model));
            match key.as_str() {
                "model" => {
                    let model = ApiConfig::resolve_model_alias(value);
                    if !p.models.iter().any(|m| m.id == model) {
                        p.models.push(provider_template(&model).models.remove(0));
                    }
                    prefs["model"] = json!(format!("cli::{model}"));
                }
                "base_url" => {
                    ensure!(
                        value.starts_with("https://") || value.starts_with("http://"),
                        "API URL must start with http:// or https://"
                    );
                    p.base_url = value.clone();
                }
                "api_format" => {
                    ensure!(
                        ["openai", "anthropic"].contains(&value.as_str()),
                        "Expected openai or anthropic"
                    );
                    p.provider_type = value.clone();
                }
                "api_key" => p.api_key = Some(value.clone()),
                "max_tokens" => {
                    let n: u32 = value
                        .parse()
                        .context("max_tokens must be a positive integer")?;
                    ensure!(n > 0, "max_tokens must be positive");
                    p.models.iter_mut().for_each(|m| m.max_tokens = Some(n));
                }
                _ => unreachable!(),
            }
            manager.update_config(|c| {
                c.providers.retain(|p| p.id != "cli");
                c.providers.push(p);
            })?;
            save_preferences(paths, &prefs)?;
            eprintln!("Updated {key}");
        }
        ConfigCommands::Reset => {
            ConfigManager::new(paths.data_dir.clone()).remove_provider("cli")?;
            save_preferences(paths, &json!({}))?;
            eprintln!("CLI settings reset; other Desktop providers preserved");
        }
        ConfigCommands::ImportLegacy { source } => import_legacy(paths, source)?,
    }
    Ok(())
}

fn import_legacy(paths: &RuntimePaths, source: &Path) -> Result<()> {
    let text = std::fs::read_to_string(source).context("Cannot read legacy settings")?;
    let old: Value = serde_json::from_str(text.trim_start_matches('\u{feff}'))
        .context("Invalid legacy settings")?;
    let model = old["model"]
        .as_str()
        .context("Legacy settings must include model")?;
    let mut provider = provider_template(model);
    provider.id = "legacy-cli".into();
    provider.name = "Imported CLI".into();
    if let Some(url) = old["api"]["base_url"].as_str() {
        provider.base_url = url.into();
    }
    provider.api_key = old["api"]["api_key"].as_str().map(String::from);
    if let Some(limit) = old["api"]["max_tokens"].as_u64() {
        let limit = u32::try_from(limit).context("Invalid legacy max_tokens")?;
        ensure!(limit > 0, "Invalid legacy max_tokens");
        provider.models[0].max_tokens = Some(limit);
    }
    // The old REPL used an OpenAI-compatible endpoint for non-Anthropic hosts.
    if !provider.base_url.contains("api.anthropic.com") {
        provider.provider_type = "openai".into();
    }
    let mut manager = ConfigManager::new(paths.data_dir.clone());
    if manager.get_provider("legacy-cli").is_some() {
        bail!("legacy-cli provider already exists; refusing to overwrite it");
    }
    manager.add_provider(provider)?;
    let mut prefs = preferences(paths)?;
    prefs["model"] = json!(format!(
        "legacy-cli::{}",
        ApiConfig::resolve_model_alias(model)
    ));
    save_preferences(paths, &prefs)?;
    eprintln!("Imported legacy provider. Source settings were not modified.");
    Ok(())
}
