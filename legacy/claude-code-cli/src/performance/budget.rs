//! Budget Manager - Token and result budget management
//!
//! Manages per-tool output caps, turn budgets, and context window budgets.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BudgetConfig {
    pub default_tool_cap: usize,
    pub turn_budget_chars: usize,
    pub preview_size_chars: usize,
    pub tool_thresholds: HashMap<String, usize>,
}

impl Default for BudgetConfig {
    fn default() -> Self {
        let mut tool_thresholds = HashMap::new();
        tool_thresholds.insert("search_files".to_string(), 50_000);
        tool_thresholds.insert("grep".to_string(), 30_000);
        tool_thresholds.insert("read_file".to_string(), 100_000);
        tool_thresholds.insert("terminal".to_string(), 20_000);
        tool_thresholds.insert("bash".to_string(), 20_000);
        tool_thresholds.insert("powershell".to_string(), 20_000);

        Self {
            default_tool_cap: 40_000,
            turn_budget_chars: 200_000,
            preview_size_chars: 500,
            tool_thresholds,
        }
    }
}

pub struct BudgetManager {
    config: BudgetConfig,
}

impl BudgetManager {
    pub fn new(config: BudgetConfig) -> Self {
        Self { config }
    }

    pub fn resolve_threshold(&self, tool_name: &str) -> usize {
        self.config
            .tool_thresholds
            .get(tool_name)
            .copied()
            .unwrap_or(self.config.default_tool_cap)
    }

    pub fn should_enforce_budget(&self, tool_name: &str) -> bool {
        let threshold = self.resolve_threshold(tool_name);
        threshold != usize::MAX
    }

    pub fn truncate_output(&self, content: &str, tool_name: &str) -> (String, bool) {
        let threshold = self.resolve_threshold(tool_name);

        if content.len() <= threshold {
            return (content.to_string(), false);
        }

        let truncated = &content[..threshold];
        let last_nl = truncated.rfind('\n').unwrap_or(threshold / 2);
        let preview = format!("{}...\n\n[Output truncated: {} chars total]", &truncated[..last_nl], content.len());

        (preview, true)
    }

    pub fn check_turn_budget(&self, messages: &[String]) -> usize {
        messages.iter().map(|m| m.len()).sum()
    }

    pub fn is_over_budget(&self, messages: &[String]) -> bool {
        self.check_turn_budget(messages) > self.config.turn_budget_chars
    }
}

impl Default for BudgetManager {
    fn default() -> Self {
        Self::new(BudgetConfig::default())
    }
}
