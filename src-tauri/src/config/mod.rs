use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub version: String,
    pub user: UserConfig,
    pub api: ApiConfig,
    pub providers: Vec<ProviderConfig>,
    pub mcp: McpConfig,
    pub appearance: AppearanceConfig,
    pub behavior: BehaviorConfig,
    pub shortcuts: ShortcutConfig,
    pub logging: LoggingConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserConfig {
    pub id: String,
    pub name: Option<String>,
    pub email: Option<String>,
    pub created_at: Option<String>,
    pub last_active: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiConfig {
    pub default_provider: String,
    pub anthropic_api_key: Option<String>,
    pub openai_api_key: Option<String>,
    pub base_urls: HashMap<String, String>,
    pub timeout_seconds: u64,
    pub max_retries: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderConfig {
    pub id: String,
    pub name: String,
    pub provider_type: String,
    pub api_key: Option<String>,
    pub base_url: String,
    pub models: Vec<ModelConfig>,
    pub enabled: bool,
    pub is_default: bool,
    pub settings: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelConfig {
    pub id: String,
    pub name: String,
    pub enabled: bool,
    pub max_tokens: Option<u32>,
    pub supports_vision: bool,
    pub supports_tools: bool,
    pub supports_streaming: bool,
    pub context_window: Option<u32>,
    pub cost_per_1k_input: Option<f64>,
    pub cost_per_1k_output: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpConfig {
    pub servers: HashMap<String, McpServerConfig>,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerConfig {
    pub name: String,
    pub command: String,
    pub args: Vec<String>,
    pub env: HashMap<String, String>,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppearanceConfig {
    pub theme: String,
    pub font_size: u32,
    pub font_family: String,
    pub code_font_family: String,
    pub window_width: u32,
    pub window_height: u32,
    pub sidebar_width: u32,
    pub show_line_numbers: bool,
    pub word_wrap: bool,
    pub accent_color: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BehaviorConfig {
    pub auto_save: bool,
    pub auto_save_interval: u32,
    pub max_conversation_history: usize,
    pub max_context_messages: usize,
    pub confirm_before_delete: bool,
    pub show_system_prompt: bool,
    pub enable_markdown_preview: bool,
    pub streaming_speed: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShortcutConfig {
    pub send_message: String,
    pub new_conversation: String,
    pub search_conversations: String,
    pub toggle_sidebar: String,
    pub settings: String,
    pub focus_input: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoggingConfig {
    pub level: String,
    pub file_path: Option<String>,
    pub max_file_size_mb: u64,
    pub max_files: u32,
    pub enable_console: bool,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            version: "1.6.12".to_string(),
            user: UserConfig {
                id: uuid::Uuid::new_v4().to_string(),
                name: None,
                email: None,
                created_at: Some(chrono::Utc::now().to_rfc3339()),
                last_active: Some(chrono::Utc::now().to_rfc3339()),
            },
            api: ApiConfig {
                default_provider: "anthropic".to_string(),
                anthropic_api_key: None,
                openai_api_key: None,
                base_urls: HashMap::new(),
                timeout_seconds: 120,
                max_retries: 3,
            },
            providers: Self::default_providers(),
            mcp: McpConfig {
                servers: HashMap::new(),
                enabled: true,
            },
            appearance: AppearanceConfig {
                theme: "light".to_string(),
                font_size: 14,
                font_family: "system-ui".to_string(),
                code_font_family: "monospace".to_string(),
                window_width: 1150,
                window_height: 700,
                sidebar_width: 280,
                show_line_numbers: true,
                word_wrap: true,
                accent_color: "#0066cc".to_string(),
            },
            behavior: BehaviorConfig {
                auto_save: true,
                auto_save_interval: 30,
                max_conversation_history: 100,
                max_context_messages: 50,
                confirm_before_delete: true,
                show_system_prompt: false,
                enable_markdown_preview: true,
                streaming_speed: "normal".to_string(),
            },
            shortcuts: ShortcutConfig {
                send_message: "Ctrl+Enter".to_string(),
                new_conversation: "Ctrl+N".to_string(),
                search_conversations: "Ctrl+K".to_string(),
                toggle_sidebar: "Ctrl+B".to_string(),
                settings: "Ctrl+,".to_string(),
                focus_input: "Ctrl+L".to_string(),
            },
            logging: LoggingConfig {
                level: "info".to_string(),
                file_path: None,
                max_file_size_mb: 10,
                max_files: 5,
                enable_console: true,
            },
        }
    }
}

impl AppConfig {
    fn default_providers() -> Vec<ProviderConfig> {
        vec![
            ProviderConfig {
                id: "anthropic".to_string(),
                name: "Anthropic".to_string(),
                provider_type: "anthropic".to_string(),
                api_key: None,
                base_url: "https://api.anthropic.com".to_string(),
                models: vec![
                    ModelConfig {
                        id: "claude-opus-4-5".to_string(),
                        name: "Claude Opus 4".to_string(),
                        enabled: true,
                        max_tokens: Some(8192),
                        supports_vision: true,
                        supports_tools: true,
                        supports_streaming: true,
                        context_window: Some(200000),
                        cost_per_1k_input: Some(0.015),
                        cost_per_1k_output: Some(0.075),
                    },
                    ModelConfig {
                        id: "claude-sonnet-4-6".to_string(),
                        name: "Claude Sonnet 4".to_string(),
                        enabled: true,
                        max_tokens: Some(8192),
                        supports_vision: true,
                        supports_tools: true,
                        supports_streaming: true,
                        context_window: Some(200000),
                        cost_per_1k_input: Some(0.003),
                        cost_per_1k_output: Some(0.015),
                    },
                    ModelConfig {
                        id: "claude-haiku-3-5".to_string(),
                        name: "Claude Haiku 3.5".to_string(),
                        enabled: true,
                        max_tokens: Some(8192),
                        supports_vision: true,
                        supports_tools: true,
                        supports_streaming: true,
                        context_window: Some(200000),
                        cost_per_1k_input: Some(0.0008),
                        cost_per_1k_output: Some(0.004),
                    },
                ],
                enabled: true,
                is_default: true,
                settings: HashMap::new(),
            },
            ProviderConfig {
                id: "openai".to_string(),
                name: "OpenAI".to_string(),
                provider_type: "openai".to_string(),
                api_key: None,
                base_url: "https://api.openai.com".to_string(),
                models: vec![
                    ModelConfig {
                        id: "gpt-4o".to_string(),
                        name: "GPT-4o".to_string(),
                        enabled: false,
                        max_tokens: Some(4096),
                        supports_vision: true,
                        supports_tools: true,
                        supports_streaming: true,
                        context_window: Some(128000),
                        cost_per_1k_input: Some(0.005),
                        cost_per_1k_output: Some(0.015),
                    },
                    ModelConfig {
                        id: "gpt-4o-mini".to_string(),
                        name: "GPT-4o Mini".to_string(),
                        enabled: false,
                        max_tokens: Some(4096),
                        supports_vision: true,
                        supports_tools: true,
                        supports_streaming: true,
                        context_window: Some(128000),
                        cost_per_1k_input: Some(0.00015),
                        cost_per_1k_output: Some(0.0006),
                    },
                ],
                enabled: false,
                is_default: false,
                settings: HashMap::new(),
            },
        ]
    }
}

pub struct ConfigManager {
    config_path: PathBuf,
    config: AppConfig,
}

impl ConfigManager {
    pub fn new(config_dir: PathBuf) -> Self {
        let config_path = config_dir.join("config.json");
        Self {
            config_path,
            config: AppConfig::default(),
        }
    }

    pub fn load(&mut self) -> Result<()> {
        if !self.config_path.exists() {
            self.config = AppConfig::default();
            self.save()?;
            return Ok(());
        }

        let content = fs::read_to_string(&self.config_path)?;
        self.config = serde_json::from_str(&content)?;
        Ok(())
    }

    pub fn save(&self) -> Result<()> {
        if let Some(parent) = self.config_path.parent() {
            fs::create_dir_all(parent)?;
        }

        let content = serde_json::to_string_pretty(&self.config)?;
        fs::write(&self.config_path, content)?;
        Ok(())
    }

    pub fn get_config(&self) -> &AppConfig {
        &self.config
    }

    pub fn update_config<F>(&mut self, updater: F) -> Result<()>
    where
        F: FnOnce(&mut AppConfig),
    {
        updater(&mut self.config);
        self.save()
    }

    pub fn get_provider(&self, id: &str) -> Option<&ProviderConfig> {
        self.config.providers.iter().find(|p| p.id == id)
    }

    pub fn get_provider_mut(&mut self, id: &str) -> Option<&mut ProviderConfig> {
        self.config.providers.iter_mut().find(|p| p.id == id)
    }

    pub fn add_provider(&mut self, provider: ProviderConfig) -> Result<()> {
        if self.config.providers.iter().any(|p| p.id == provider.id) {
            return Err(anyhow!("Provider '{}' already exists", provider.id));
        }

        self.config.providers.push(provider);
        self.save()
    }

    pub fn remove_provider(&mut self, id: &str) -> Result<()> {
        let initial_len = self.config.providers.len();
        self.config.providers.retain(|p| p.id != id);

        if self.config.providers.len() == initial_len {
            return Err(anyhow!("Provider '{}' not found", id));
        }

        if self.config.api.default_provider == id {
            if let Some(first) = self.config.providers.first() {
                self.config.api.default_provider = first.id.clone();
            }
        }

        self.save()
    }

    pub fn set_api_key(&mut self, provider: &str, api_key: String) -> Result<()> {
        match provider {
            "anthropic" => {
                self.config.api.anthropic_api_key = Some(api_key);
            }
            "openai" => {
                self.config.api.openai_api_key = Some(api_key);
            }
            _ => {
                if let Some(p) = self.config.providers.iter_mut().find(|p| p.id == provider) {
                    p.api_key = Some(api_key);
                } else {
                    return Err(anyhow!("Provider '{}' not found", provider));
                }
            }
        }

        self.save()
    }

    pub fn get_api_key(&self, provider: &str) -> Option<String> {
        match provider {
            "anthropic" => self.config.api.anthropic_api_key.clone(),
            "openai" => self.config.api.openai_api_key.clone(),
            _ => self.config.providers.iter()
                .find(|p| p.id == provider)
                .and_then(|p| p.api_key.clone()),
        }
    }

    pub fn set_base_url(&mut self, provider: &str, base_url: String) -> Result<()> {
        self.config.api.base_urls.insert(provider.to_string(), base_url);
        self.save()
    }

    pub fn get_base_url(&self, provider: &str) -> Option<String> {
        self.config.api.base_urls.get(provider).cloned()
    }

    pub fn update_user(&mut self, name: Option<String>, email: Option<String>) -> Result<()> {
        if let Some(n) = name {
            self.config.user.name = Some(n);
        }
        if let Some(e) = email {
            self.config.user.email = Some(e);
        }
        self.config.user.last_active = Some(chrono::Utc::now().to_rfc3339());
        self.save()
    }

    pub fn get_conversation_storage_path(&self) -> PathBuf {
        self.config_path
            .parent()
            .unwrap_or(&PathBuf::from("."))
            .join("conversations")
    }

    pub fn get_cache_path(&self) -> PathBuf {
        self.config_path
            .parent()
            .unwrap_or(&PathBuf::from("."))
            .join("cache")
    }

    pub fn get_logs_path(&self) -> PathBuf {
        self.config_path
            .parent()
            .unwrap_or(&PathBuf::from("."))
            .join("logs")
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationMetadata {
    pub id: String,
    pub title: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub model: Option<String>,
    pub message_count: usize,
    pub provider: Option<String>,
    pub tags: Vec<String>,
    pub pinned: bool,
    pub archived: bool,
}

pub struct ConversationStore {
    store_path: PathBuf,
}

impl ConversationStore {
    pub fn new(store_path: PathBuf) -> Self {
        Self { store_path }
    }

    pub fn init(&self) -> Result<()> {
        fs::create_dir_all(&self.store_path)?;
        Ok(())
    }

    pub fn save_conversation(&self, id: &str, messages: &[serde_json::Value]) -> Result<()> {
        let path = self.store_path.join(format!("{}.json", id));
        let data = serde_json::json!({
            "id": id,
            "messages": messages,
            "saved_at": chrono::Utc::now().to_rfc3339(),
        });
        fs::write(path, serde_json::to_string_pretty(&data)?)?;
        Ok(())
    }

    pub fn load_conversation(&self, id: &str) -> Result<Vec<serde_json::Value>> {
        let path = self.store_path.join(format!("{}.json", id));
        if !path.exists() {
            return Ok(Vec::new());
        }
        let content = fs::read_to_string(path)?;
        let data: serde_json::Value = serde_json::from_str(&content)?;
        Ok(data.get("messages")
            .and_then(|m| m.as_array())
            .map(|a| a.clone())
            .unwrap_or_default())
    }

    pub fn list_conversations(&self) -> Result<Vec<ConversationMetadata>> {
        let mut conversations = Vec::new();

        let entries = fs::read_dir(&self.store_path)?;
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("json") {
                if let Ok(content) = fs::read_to_string(&path) {
                    if let Ok(data) = serde_json::from_str::<serde_json::Value>(&content) {
                        conversations.push(ConversationMetadata {
                            id: data.get("id")
                                .and_then(|v| v.as_str())
                                .unwrap_or("")
                                .to_string(),
                            title: data.get("title").and_then(|v| v.as_str()).map(String::from),
                            created_at: data.get("created_at")
                                .and_then(|v| v.as_str())
                                .unwrap_or("")
                                .to_string(),
                            updated_at: data.get("saved_at")
                                .and_then(|v| v.as_str())
                                .unwrap_or("")
                                .to_string(),
                            model: data.get("model").and_then(|v| v.as_str()).map(String::from),
                            message_count: data.get("messages")
                                .and_then(|m| m.as_array())
                                .map(|a| a.len())
                                .unwrap_or(0),
                            provider: None,
                            tags: Vec::new(),
                            pinned: false,
                            archived: false,
                        });
                    }
                }
            }
        }

        conversations.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
        Ok(conversations)
    }

    pub fn delete_conversation(&self, id: &str) -> Result<()> {
        let path = self.store_path.join(format!("{}.json", id));
        if path.exists() {
            fs::remove_file(path)?;
        }
        Ok(())
    }

    pub fn delete_messages_from(&self, id: &str, message_id: &str) -> Result<Vec<serde_json::Value>> {
        let mut messages = self.load_conversation(id)?;
        let idx = messages.iter().position(|m| m.get("id").and_then(|v| v.as_str()) == Some(message_id));
        if let Some(i) = idx {
            messages.truncate(i);
            self.save_conversation(id, &messages)?;
        }
        Ok(messages)
    }

    pub fn delete_messages_tail(&self, id: &str, count: usize) -> Result<Vec<serde_json::Value>> {
        let mut messages = self.load_conversation(id)?;
        if count >= messages.len() {
            messages.clear();
        } else {
            messages.truncate(messages.len() - count);
        }
        self.save_conversation(id, &messages)?;
        Ok(messages)
    }

    pub fn branch_conversation(&self, source_id: &str, from_message_id: Option<&str>) -> Result<String> {
        let mut messages = self.load_conversation(source_id)?;
        if let Some(mid) = from_message_id {
            let idx = messages.iter().position(|m| m.get("id").and_then(|v| v.as_str()) == Some(mid));
            if let Some(i) = idx {
                messages.truncate(i);
            }
        }
        let new_id = uuid::Uuid::new_v4().to_string();
        let source_conv = self.load_conversation_meta(source_id)?;
        let title = source_conv.title.unwrap_or_else(|| "Branched conversation".to_string());
        self.save_conversation_with_meta(&new_id, &messages, &format!("{} (branch)", title), source_conv.model.as_deref())?;
        Ok(new_id)
    }

    fn load_conversation_meta(&self, id: &str) -> Result<ConversationMetadata> {
        let path = self.store_path.join(format!("{}.json", id));
        if !path.exists() {
            return Ok(ConversationMetadata {
                id: id.to_string(), title: None, created_at: String::new(),
                updated_at: String::new(), model: None, message_count: 0,
                provider: None, tags: Vec::new(), pinned: false, archived: false,
            });
        }
        let content = fs::read_to_string(path)?;
        let data: serde_json::Value = serde_json::from_str(&content)?;
        Ok(ConversationMetadata {
            id: data.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            title: data.get("title").and_then(|v| v.as_str()).map(String::from),
            created_at: data.get("created_at").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            updated_at: data.get("saved_at").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            model: data.get("model").and_then(|v| v.as_str()).map(String::from),
            message_count: data.get("messages").and_then(|m| m.as_array()).map(|a| a.len()).unwrap_or(0),
            provider: None, tags: Vec::new(), pinned: false, archived: false,
        })
    }

    fn save_conversation_with_meta(&self, id: &str, messages: &[serde_json::Value], title: &str, model: Option<&str>) -> Result<()> {
        let path = self.store_path.join(format!("{}.json", id));
        let data = serde_json::json!({
            "id": id,
            "title": title,
            "model": model,
            "messages": messages,
            "created_at": chrono::Utc::now().to_rfc3339(),
            "saved_at": chrono::Utc::now().to_rfc3339(),
        });
        fs::write(path, serde_json::to_string_pretty(&data)?)?;
        Ok(())
    }
}
