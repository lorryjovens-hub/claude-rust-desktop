//! Hooks Module
//!
//! Provides pre/post execution hooks for extensibility.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookConfig {
    pub enabled: bool,
    pub pre_hooks: Vec<Hook>,
    pub post_hooks: Vec<Hook>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Hook {
    pub name: String,
    pub command: String,
    pub working_dir: Option<String>,
    pub env: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookResult {
    pub hook_name: String,
    pub success: bool,
    pub stdout: String,
    pub stderr: String,
    pub duration_ms: u64,
}

pub struct HookManager {
    config: HookConfig,
}

impl Default for HookManager {
    fn default() -> Self {
        Self::new()
    }
}

impl HookManager {
    pub fn new() -> Self {
        Self {
            config: HookConfig {
                enabled: true,
                pre_hooks: Vec::new(),
                post_hooks: Vec::new(),
            },
        }
    }

    pub fn with_hooks(mut self, pre: Vec<Hook>, post: Vec<Hook>) -> Self {
        self.config.pre_hooks = pre;
        self.config.post_hooks = post;
        self
    }

    pub fn add_pre_hook(&mut self, hook: Hook) {
        self.config.pre_hooks.push(hook);
    }

    pub fn add_post_hook(&mut self, hook: Hook) {
        self.config.post_hooks.push(hook);
    }

    pub async fn run_pre_hooks(&self) -> Vec<HookResult> {
        self.run_hooks(&self.config.pre_hooks).await
    }

    pub async fn run_post_hooks(&self) -> Vec<HookResult> {
        self.run_hooks(&self.config.post_hooks).await
    }

    async fn run_hooks(&self, hooks: &[Hook]) -> Vec<HookResult> {
        let mut results = Vec::new();

        for hook in hooks {
            let start = std::time::Instant::now();
            let result = self.execute_hook(hook).await;
            let duration = start.elapsed().as_millis() as u64;

            results.push(HookResult {
                hook_name: hook.name.clone(),
                success: result.is_ok(),
                stdout: result.as_ref().map(|r| r.stdout.clone()).unwrap_or_default(),
                stderr: result.as_ref().map(|r| r.stderr.clone()).unwrap_or_default(),
                duration_ms: duration,
            });
        }

        results
    }

    async fn execute_hook(&self, hook: &Hook) -> Result<HookOutput, String> {
        let output = tokio::process::Command::new("sh")
            .args(["-c", &hook.command])
            .current_dir(hook.working_dir.as_deref().unwrap_or("."))
            .envs(&hook.env)
            .output()
            .await
            .map_err(|e| e.to_string())?;

        Ok(HookOutput {
            stdout: String::from_utf8_lossy(&output.stdout).to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
            exit_code: output.status.code().unwrap_or(-1),
        })
    }
}

#[derive(Debug, Clone)]
struct HookOutput {
    stdout: String,
    stderr: String,
    exit_code: i32,
}