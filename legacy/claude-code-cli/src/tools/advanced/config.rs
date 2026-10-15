//! ConfigTool - Configuration management
//!
//! Provides configuration loading, merging, and management for Claude Code settings.

use crate::tools::{Tool, ToolOutput, ToolError};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::fs;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClaudeConfig {
    pub version: Option<String>,
    pub permissions: Option<PermissionsConfig>,
    pub tools: Option<ToolsConfig>,
    pub agent: Option<AgentConfig>,
    pub ui: Option<UiConfig>,
    pub mcp: Option<McpConfig>,
    pub plugins: Option<PluginsConfig>,
    pub hooks: Option<HooksConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PermissionsConfig {
    pub mode: Option<String>,
    pub allow: Option<Vec<String>>,
    pub deny: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolsConfig {
    pub enabled: Option<Vec<String>>,
    pub disabled: Option<Vec<String>>,
    pub timeout_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentConfig {
    pub model: Option<String>,
    pub temperature: Option<f32>,
    pub max_tokens: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiConfig {
    pub theme: Option<String>,
    pub font_size: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpConfig {
    pub servers: Option<HashMap<String, serde_json::Value>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginsConfig {
    pub enabled: Option<Vec<String>>,
    pub disabled: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HooksConfig {
    pub pre_tool_call: Option<Vec<String>>,
    pub post_tool_call: Option<Vec<String>>,
}

impl Default for ClaudeConfig {
    fn default() -> Self {
        Self {
            version: Some("1.0".to_string()),
            permissions: None,
            tools: None,
            agent: None,
            ui: None,
            mcp: None,
            plugins: None,
            hooks: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ConfigSource {
    Local,
    Project,
    User,
    Default,
}

impl std::fmt::Display for ConfigSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConfigSource::Local => write!(f, "local"),
            ConfigSource::Project => write!(f, "project"),
            ConfigSource::User => write!(f, "user"),
            ConfigSource::Default => write!(f, "default"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ConfigEntry {
    pub source: ConfigSource,
    pub path: PathBuf,
    pub config: ClaudeConfig,
}

pub struct ConfigTool {
    workspace_path: Option<String>,
    config_cache: HashMap<ConfigSource, Option<ClaudeConfig>>,
}

impl Default for ConfigTool {
    fn default() -> Self {
        Self::new()
    }
}

impl ConfigTool {
    pub fn new() -> Self {
        Self {
            workspace_path: None,
            config_cache: HashMap::new(),
        }
    }

    pub fn with_workspace(mut self, path: &str) -> Self {
        self.workspace_path = Some(path.to_string());
        self
    }

    fn get_config_paths() -> Vec<(ConfigSource, PathBuf)> {
        let mut paths = Vec::new();

        if let Some(home) = dirs::home_dir() {
            paths.push((
                ConfigSource::User,
                home.join(".claude").join("config.json"),
            ));
        }

        if let Ok(cwd) = std::env::current_dir() {
            paths.push((ConfigSource::Project, cwd.join(".claude.json")));
            paths.push((ConfigSource::Local, cwd.join(".clauderc.json")));
        }

        paths
    }

    fn load_config_file(path: &PathBuf) -> Result<ClaudeConfig, ToolError> {
        if !path.exists() {
            return Err(ToolError {
                message: format!("Config file not found: {}", path.display()),
                code: Some("file_not_found".to_string()),
            });
        }

        let content = fs::read_to_string(path)
            .map_err(|e| ToolError {
                message: format!("Failed to read config: {}", e),
                code: Some("read_error".to_string()),
            })?;

        let config: ClaudeConfig = serde_json::from_str(&content)
            .map_err(|e| ToolError {
                message: format!("Failed to parse config: {}", e),
                code: Some("parse_error".to_string()),
            })?;

        Ok(config)
    }

    fn merge_configs(base: &ClaudeConfig, override_config: &ClaudeConfig) -> ClaudeConfig {
        ClaudeConfig {
            version: override_config.version.clone().or_else(|| base.version.clone()),
            permissions: override_config.permissions.clone().or_else(|| base.permissions.clone()),
            tools: override_config.tools.clone().or_else(|| base.tools.clone()),
            agent: override_config.agent.clone().or_else(|| base.agent.clone()),
            ui: override_config.ui.clone().or_else(|| base.ui.clone()),
            mcp: override_config.mcp.clone().or_else(|| base.mcp.clone()),
            plugins: override_config.plugins.clone().or_else(|| base.plugins.clone()),
            hooks: override_config.hooks.clone().or_else(|| base.hooks.clone()),
        }
    }

    fn discover_configs(&self) -> Vec<ConfigEntry> {
        let mut entries = Vec::new();
        let paths = Self::get_config_paths();

        for (source, path) in paths {
            match Self::load_config_file(&path) {
                Ok(config) => {
                    entries.push(ConfigEntry {
                        source: source.clone(),
                        path,
                        config,
                    });
                }
                Err(_) => {}
            }
        }

        entries
    }

    fn load_and_merge(&self) -> Result<ClaudeConfig, ToolError> {
        let entries = self.discover_configs();

        if entries.is_empty() {
            return Ok(ClaudeConfig::default());
        }

        let mut merged = entries[0].config.clone();

        for entry in entries.iter().skip(1) {
            merged = Self::merge_configs(&merged, &entry.config);
        }

        Ok(merged)
    }

    fn get_config_entry(&mut self, source: ConfigSource) -> Result<Option<ConfigEntry>, ToolError> {
        let paths = Self::get_config_paths();

        for (src, path) in paths {
            if src == source {
                match Self::load_config_file(&path) {
                    Ok(config) => {
                        return Ok(Some(ConfigEntry {
                            source,
                            path,
                            config,
                        }));
                    }
                    Err(_) => return Ok(None),
                }
            }
        }

        Ok(None)
    }

    fn save_config(&self, path: &PathBuf, config: &ClaudeConfig) -> Result<(), ToolError> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| ToolError {
                    message: format!("Failed to create config directory: {}", e),
                    code: Some("mkdir_error".to_string()),
                })?;
        }

        let content = serde_json::to_string_pretty(config)
            .map_err(|e| ToolError {
                message: format!("Failed to serialize config: {}", e),
                code: Some("serialize_error".to_string()),
            })?;

        fs::write(path, content)
            .map_err(|e| ToolError {
                message: format!("Failed to write config: {}", e),
                code: Some("write_error".to_string()),
            })?;

        Ok(())
    }
}

#[derive(Debug, Deserialize)]
struct GetInput {
    operation: String,
    #[serde(default)]
    source: Option<String>,
    #[serde(default)]
    key: Option<String>,
    #[serde(default)]
    value: Option<serde_json::Value>,
}

#[async_trait]
impl Tool for ConfigTool {
    fn name(&self) -> &str {
        "Config"
    }

    fn description(&self) -> &str {
        "Manage Claude Code configuration. Operations: get, set, list, merge, sources"
    }

    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "operation": {
                    "type": "string",
                    "enum": ["get", "set", "list", "merge", "sources", "create"],
                    "description": "Operation to perform"
                },
                "source": {
                    "type": "string",
                    "enum": ["local", "project", "user", "default"],
                    "description": "Config source to target"
                },
                "key": {
                    "type": "string",
                    "description": "Config key to get/set"
                },
                "value": {
                    "type": "any",
                    "description": "Value to set"
                }
            },
            "required": ["operation"]
        })
    }

    async fn execute(&self, input: serde_json::Value) -> Result<ToolOutput, ToolError> {
        let input: GetInput = serde_json::from_value(input)
            .map_err(|e| ToolError {
                message: format!("Invalid input: {}", e),
                code: Some("invalid_input".to_string()),
            })?;

        match input.operation.as_str() {
            "get" => self.get_config(&input).await,
            "set" => self.set_config(&input).await,
            "list" => self.list_configs().await,
            "merge" => self.merge_configs_op().await,
            "sources" => self.list_sources().await,
            "create" => self.create_config(&input).await,
            _ => Err(ToolError {
                message: format!("Unknown operation: {}", input.operation),
                code: Some("unknown_operation".to_string()),
            }),
        }
    }
}

impl ConfigTool {
    async fn get_config(&self, input: &GetInput) -> Result<ToolOutput, ToolError> {
        let mut config_tool = self.clone();
        let config = config_tool.load_and_merge()?;

        if let Some(key) = &input.key {
            let value = Self::get_nested_value(&config, key);
            Ok(ToolOutput {
                output_type: "json".to_string(),
                content: serde_json::to_string_pretty(&value).unwrap_or_default(),
                metadata: HashMap::new(),
            })
        } else {
            Ok(ToolOutput {
                output_type: "json".to_string(),
                content: serde_json::to_string_pretty(&config).unwrap(),
                metadata: HashMap::new(),
            })
        }
    }

    fn get_nested_value(config: &ClaudeConfig, key: &str) -> serde_json::Value {
        match key {
            "version" => serde_json::to_value(&config.version).unwrap(),
            "permissions" => serde_json::to_value(&config.permissions).unwrap(),
            "tools" => serde_json::to_value(&config.tools).unwrap(),
            "agent" => serde_json::to_value(&config.agent).unwrap(),
            "ui" => serde_json::to_value(&config.ui).unwrap(),
            "mcp" => serde_json::to_value(&config.mcp).unwrap(),
            "plugins" => serde_json::to_value(&config.plugins).unwrap(),
            "hooks" => serde_json::to_value(&config.hooks).unwrap(),
            _ => serde_json::Value::Null,
        }
    }

    async fn set_config(&self, input: &GetInput) -> Result<ToolOutput, ToolError> {
        let source = input.source.as_deref().unwrap_or("local");
        let paths = Self::get_config_paths();

        let target_path = paths.iter()
            .find(|(s, _)| s.to_string().to_lowercase() == source.to_lowercase())
            .map(|(_, p)| p.clone())
            .ok_or_else(|| ToolError {
                message: format!("Unknown config source: {}", source),
                code: Some("unknown_source".to_string()),
            })?;

        let mut config = Self::load_config_file(&target_path)
            .unwrap_or_else(|_| ClaudeConfig::default());

        if let Some(key) = &input.key {
            Self::set_nested_value(&mut config, key, input.value.clone().unwrap_or(serde_json::Value::Null));
        }

        self.save_config(&target_path, &config)?;

        Ok(ToolOutput {
            output_type: "json".to_string(),
            content: serde_json::to_string_pretty(&serde_json::json!({
                "message": "Config updated successfully",
                "path": target_path.to_string_lossy()
            })).unwrap(),
            metadata: HashMap::new(),
        })
    }

    fn set_nested_value(config: &mut ClaudeConfig, key: &str, value: serde_json::Value) {
        match key {
            "version" => config.version = value.as_str().map(String::from),
            "permissions" => config.permissions = serde_json::from_value(value).ok(),
            "tools" => config.tools = serde_json::from_value(value).ok(),
            "agent" => config.agent = serde_json::from_value(value).ok(),
            "ui" => config.ui = serde_json::from_value(value).ok(),
            "mcp" => config.mcp = serde_json::from_value(value).ok(),
            "plugins" => config.plugins = serde_json::from_value(value).ok(),
            "hooks" => config.hooks = serde_json::from_value(value).ok(),
            _ => {}
        }
    }

    async fn list_configs(&self) -> Result<ToolOutput, ToolError> {
        let paths = Self::get_config_paths();
        let mut configs = Vec::new();

        for (source, path) in paths {
            let exists = path.exists();
            let content = if exists {
                fs::read_to_string(&path).ok()
            } else {
                None
            };

            configs.push(serde_json::json!({
                "source": source,
                "path": path.to_string_lossy(),
                "exists": exists,
                "content": content
            }));
        }

        Ok(ToolOutput {
            output_type: "json".to_string(),
            content: serde_json::to_string_pretty(&configs).unwrap(),
            metadata: HashMap::new(),
        })
    }

    async fn merge_configs_op(&self) -> Result<ToolOutput, ToolError> {
        let merged = self.load_and_merge()?;

        Ok(ToolOutput {
            output_type: "json".to_string(),
            content: serde_json::to_string_pretty(&merged).unwrap(),
            metadata: HashMap::new(),
        })
    }

    async fn list_sources(&self) -> Result<ToolOutput, ToolError> {
        let paths = Self::get_config_paths();
        let mut sources = Vec::new();

        for (source, path) in paths {
            sources.push(serde_json::json!({
                "name": format!("{:?}", source),
                "path": path.to_string_lossy(),
                "priority": match source {
                    ConfigSource::Local => 1,
                    ConfigSource::Project => 2,
                    ConfigSource::User => 3,
                    ConfigSource::Default => 0,
                },
                "exists": path.exists()
            }));
        }

        sources.sort_by(|a, b| {
            let p1 = a["priority"].as_u64().unwrap_or(0);
            let p2 = b["priority"].as_u64().unwrap_or(0);
            p2.cmp(&p1)
        });

        Ok(ToolOutput {
            output_type: "json".to_string(),
            content: serde_json::to_string_pretty(&sources).unwrap(),
            metadata: HashMap::new(),
        })
    }

    async fn create_config(&self, input: &GetInput) -> Result<ToolOutput, ToolError> {
        let source = input.source.as_deref().unwrap_or("local");
        let paths = Self::get_config_paths();

        let target_path = paths.iter()
            .find(|(s, _)| s.to_string().to_lowercase() == source.to_lowercase())
            .map(|(_, p)| p.clone())
            .ok_or_else(|| ToolError {
                message: format!("Unknown config source: {}", source),
                code: Some("unknown_source".to_string()),
            })?;

        if target_path.exists() {
            return Err(ToolError {
                message: "Config file already exists".to_string(),
                code: Some("already_exists".to_string()),
            });
        }

        let config = if let Some(key) = &input.key {
            let mut cfg = ClaudeConfig::default();
            Self::set_nested_value(&mut cfg, key, input.value.clone().unwrap_or(serde_json::Value::Null));
            cfg
        } else {
            ClaudeConfig::default()
        };

        self.save_config(&target_path, &config)?;

        Ok(ToolOutput {
            output_type: "json".to_string(),
            content: serde_json::to_string_pretty(&serde_json::json!({
                "message": "Config created successfully",
                "path": target_path.to_string_lossy()
            })).unwrap(),
            metadata: HashMap::new(),
        })
    }
}

impl Clone for ConfigTool {
    fn clone(&self) -> Self {
        Self {
            workspace_path: self.workspace_path.clone(),
            config_cache: self.config_cache.clone(),
        }
    }
}