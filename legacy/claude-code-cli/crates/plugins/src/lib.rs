//! Plugins Module
//!
//! Provides plugin lifecycle management and hook execution.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Plugin {
    pub name: String,
    pub version: String,
    pub description: Option<String>,
    pub hooks: Vec<String>,
    pub tools: Vec<String>,
    pub config: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginMetadata {
    pub name: String,
    pub version: String,
    pub description: Option<String>,
    pub author: Option<String>,
    pub repository: Option<String>,
}

pub struct PluginManager {
    plugins: HashMap<String, Plugin>,
    plugin_dirs: Vec<PathBuf>,
}

impl Default for PluginManager {
    fn default() -> Self {
        Self::new()
    }
}

impl PluginManager {
    pub fn new() -> Self {
        Self {
            plugins: HashMap::new(),
            plugin_dirs: Vec::new(),
        }
    }

    pub fn add_plugin_dir(&mut self, dir: PathBuf) {
        self.plugin_dirs.push(dir);
    }

    pub fn load_plugins(&mut self) -> Result<Vec<Plugin>, PluginError> {
        let mut loaded = Vec::new();

        for dir in &self.plugin_dirs {
            if let Ok(entries) = std::fs::read_dir(dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.is_dir() && path.join(".claude-plugin").exists() {
                        if let Ok(plugin) = self.load_plugin_from_dir(&path) {
                            loaded.push(plugin.clone());
                            self.plugins.insert(plugin.name.clone(), plugin);
                        }
                    }
                }
            }
        }

        Ok(loaded)
    }

    fn load_plugin_from_dir(&self, dir: &PathBuf) -> Result<Plugin, PluginError> {
        let metadata_path = dir.join(".claude-plugin/plugin.json");
        if !metadata_path.exists() {
            return Err(PluginError::MetadataNotFound(dir.to_string_lossy().to_string()));
        }

        let content = std::fs::read_to_string(&metadata_path)
            .map_err(|e| PluginError::IoError(e.to_string()))?;

        let metadata: PluginMetadata = serde_json::from_str(&content)
            .map_err(|e| PluginError::ParseError(e.to_string()))?;

        Ok(Plugin {
            name: metadata.name,
            version: metadata.version,
            description: metadata.description,
            hooks: Vec::new(),
            tools: Vec::new(),
            config: HashMap::new(),
        })
    }

    pub fn get_plugin(&self, name: &str) -> Option<&Plugin> {
        self.plugins.get(name)
    }

    pub fn list_plugins(&self) -> Vec<&Plugin> {
        self.plugins.values().collect()
    }
}

#[derive(Debug, thiserror::Error)]
pub enum PluginError {
    #[error("Plugin metadata not found in: {0}")]
    MetadataNotFound(String),
    #[error("Failed to read plugin: {0}")]
    IoError(String),
    #[error("Failed to parse plugin metadata: {0}")]
    ParseError(String),
}