use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Provider {
    pub id: String,
    pub name: String,
    pub base_url: String,
    pub api_key: String,
    pub api_format: ApiFormat,
    pub models: Vec<ModelConfig>,
    pub enabled: bool,
    pub web_search_strategy: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ApiFormat {
    #[serde(rename = "anthropic")]
    Anthropic,
    #[serde(rename = "openai")]
    OpenAI,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelConfig {
    pub id: String,
    pub name: String,
    pub enabled: bool,
    pub max_tokens: Option<u32>,
    pub supports_vision: bool,
    pub supports_web_search: bool,
}

#[derive(Debug, Clone)]
pub struct ResolvedProvider {
    pub provider: Provider,
    pub model: ModelConfig,
}

pub struct ProviderManager {
    providers: Vec<Provider>,
    config_path: PathBuf,
}

impl ProviderManager {
    pub fn new(config_path: PathBuf) -> Self {
        let mut manager = Self {
            providers: Vec::new(),
            config_path,
        };
        manager.load();
        manager
    }

    pub fn from_providers(providers: Vec<Provider>, config_path: PathBuf) -> Self {
        let mut manager = Self {
            providers,
            config_path,
        };
        if let Err(e) = manager.save() {
            eprintln!("[ProviderManager] Failed to save synced providers: {}", e);
        }
        manager
    }

    pub fn config_path(&self) -> &PathBuf {
        &self.config_path
    }

    pub fn load(&mut self) {
        if self.config_path.exists() {
            if let Ok(content) = std::fs::read_to_string(&self.config_path) {
                if let Ok(providers) = serde_json::from_str::<Vec<Provider>>(&content) {
                    self.providers = providers;
                    eprintln!("[ProviderManager] Loaded {} providers from {}", self.providers.len(), self.config_path.display());
                }
            }
        }
    }

    pub fn save(&self) -> Result<(), anyhow::Error> {
        if let Some(parent) = self.config_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let content = serde_json::to_string_pretty(&self.providers)?;
        std::fs::write(&self.config_path, content)?;
        Ok(())
    }

    pub fn resolve_provider(&self, model_id: &str) -> Option<ResolvedProvider> {
        let mut first_match: Option<ResolvedProvider> = None;
        
        // Model alias mapping for common mismatches
        let aliases: std::collections::HashMap<&str, Vec<&str>> = [
            ("deepseek-v4-pro", vec!["deepseek-chat"]),
            ("deepseek-v4-flash", vec!["deepseek-reasoner"]),
        ].iter().cloned().collect();
        
        // Build list of IDs to try: original + aliases
        let mut ids_to_try = vec![model_id];
        if let Some(alias_list) = aliases.get(model_id) {
            ids_to_try.extend(alias_list.iter().copied());
        }
        // Also check if model_id is an alias of another model
        for (canonical, alias_list) in &aliases {
            if alias_list.contains(&model_id) {
                ids_to_try.push(canonical);
            }
        }
        
        for try_id in &ids_to_try {
            for provider in &self.providers {
                if !provider.enabled {
                    continue;
                }
                
                for model in &provider.models {
                    if model.id == *try_id && model.enabled {
                        if first_match.is_none() {
                            first_match = Some(ResolvedProvider {
                                provider: provider.clone(),
                                model: model.clone(),
                            });
                        } else {
                            eprintln!(
                                "[ProviderManager] WARNING: model \"{}\" exists in multiple providers. Using first match.",
                                model_id
                            );
                        }
                        break;
                    }
                }
                
                if first_match.is_some() {
                    break;
                }
            }
            if first_match.is_some() {
                break;
            }
        }

        if let Some(resolved) = &first_match {
            eprintln!(
                "[ProviderManager] Resolved \"{}\" → \"{}\" ({})",
                model_id, resolved.provider.name, resolved.provider.base_url
            );
        } else {
            eprintln!("[ProviderManager] No provider found for \"{}\"", model_id);
        }

        first_match
    }

    pub fn normalize_base_url(url: &str) -> String {
        let clean = url.trim_end_matches('/');
        let clean = clean
            .strip_suffix("/chat/completions")
            .or_else(|| clean.strip_suffix("/messages"))
            .or_else(|| clean.strip_suffix("/anthropic"))
            .or_else(|| clean.strip_suffix("/v1"))
            .unwrap_or(clean);
        clean.trim_end_matches('/').to_string()
    }

    pub fn list_providers(&self) -> &[Provider] {
        &self.providers
    }

    pub fn update_provider(&mut self, id: &str, provider: Provider) {
        if let Some(idx) = self.providers.iter().position(|p| p.id == id) {
            self.providers[idx] = provider;
            eprintln!("[ProviderManager] Updated provider: {}", id);
        } else {
            self.providers.push(provider);
            eprintln!("[ProviderManager] Added new provider: {}", id);
        }
        if let Err(e) = self.save() {
            eprintln!("[ProviderManager] Failed to save providers: {}", e);
        } else {
            eprintln!("[ProviderManager] Providers saved successfully to {}", self.config_path.display());
        }
    }

    pub fn delete_provider(&mut self, id: &str) {
        self.providers.retain(|p| p.id != id);
        eprintln!("[ProviderManager] Deleted provider: {}", id);
        if let Err(e) = self.save() {
            eprintln!("[ProviderManager] Failed to save providers after deletion: {}", e);
        } else {
            eprintln!("[ProviderManager] Providers saved successfully after deletion");
        }
    }
}
