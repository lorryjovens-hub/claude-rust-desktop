use super::audit::{AuditAction, AuditEntry, AuditLoggerRef};
use super::rules::{DANGEROUS_TOOLS, PermissionAction, PermissionLevel, PermissionRule, PermissionRuleset, PermissionScope};
use super::{PermissionChecker, PermissionContext, PermissionResult, ToolPermission};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PermissionMode {
    AskPermissions,
    AcceptEdits,
    PlanMode,
    BypassPermissions,
}

impl Default for PermissionMode {
    fn default() -> Self {
        PermissionMode::AcceptEdits
    }
}

impl PermissionMode {
    pub fn from_str(s: &str) -> Self {
        let s_lower = s.to_lowercase();
        match s_lower.as_str() {
            "ask_permissions" | "ask" | "ask_permission" => PermissionMode::AskPermissions,
            "accept_edits" | "edits" => PermissionMode::AcceptEdits,
            "plan_mode" | "plan" => PermissionMode::PlanMode,
            "bypass_permissions" | "bypass" | "always_allow" | "alwaysallow" | "zh" => PermissionMode::BypassPermissions,
            _ => PermissionMode::AcceptEdits,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            PermissionMode::AskPermissions => "ask_permissions",
            PermissionMode::AcceptEdits => "accept_edits",
            PermissionMode::PlanMode => "plan_mode",
            PermissionMode::BypassPermissions => "bypass_permissions",
        }
    }
}

pub struct PermissionManager {
    ruleset: Mutex<PermissionRuleset>,
    tool_permissions: Mutex<HashMap<String, ToolPermission>>,
    audit_logger: AuditLoggerRef,
    confirmations: Mutex<HashMap<String, bool>>,
    mode: Mutex<PermissionMode>,
}

impl PermissionManager {
    pub fn new(audit_logger: AuditLoggerRef) -> Self {
        let mut manager = Self {
            ruleset: Mutex::new(PermissionRuleset::new()),
            tool_permissions: Mutex::new(HashMap::new()),
            audit_logger,
            confirmations: Mutex::new(HashMap::new()),
            mode: Mutex::new(PermissionMode::default()),
        };
        manager.load_default_rules();
        manager
    }

    pub fn set_mode(&self, mode: PermissionMode) {
        let mut m = self.mode.lock().unwrap();
        *m = mode;
        eprintln!("[Permission] Mode changed to: {}", mode.as_str());
    }

    pub fn get_mode(&self) -> PermissionMode {
        *self.mode.lock().unwrap()
    }

    fn load_default_rules(&mut self) {
        let default_rules = super::rules::get_default_rules();
        let mut ruleset = self.ruleset.lock().unwrap();
        *ruleset = default_rules;
    }

    pub fn add_rule(&self, rule: PermissionRule) {
        let mut ruleset = self.ruleset.lock().unwrap();
        ruleset.add_rule(rule);
    }

    pub fn remove_rule(&self, rule_id: &str) {
        let mut ruleset = self.ruleset.lock().unwrap();
        ruleset.rules.retain(|r| r.id != rule_id);
    }

    pub fn get_rules(&self) -> Vec<PermissionRule> {
        let ruleset = self.ruleset.lock().unwrap();
        ruleset.rules.clone()
    }

    pub fn set_tool_permission(&self, tool_name: &str, permission: ToolPermission) {
        let mut permissions = self.tool_permissions.lock().unwrap();
        permissions.insert(tool_name.to_string(), permission);
    }

    pub fn get_tool_permission(&self, tool_name: &str) -> Option<ToolPermission> {
        let permissions = self.tool_permissions.lock().unwrap();
        permissions.get(tool_name).cloned()
    }

    fn is_read_only_tool(tool_name: &str) -> bool {
        matches!(
            tool_name,
            "Read" | "ListDir" | "Grep" | "WebFetch" | "WebSearch"
                | "TodoRead" | "TodoWrite" | "SkillRead"
        )
    }

    fn is_edit_tool(tool_name: &str) -> bool {
        matches!(
            tool_name,
            "Write" | "Edit" | "SearchReplace" | "GlobReplace" | "Delete"
                | "SkillCreate" | "SkillUpdate"
        )
    }

    fn is_dangerous_tool(tool_name: &str) -> bool {
        matches!(
            tool_name,
            "Bash" | "Delete"
        )
    }

    pub fn check_permission(&self, context: &PermissionContext) -> PermissionResult {
        let mode = self.get_mode();

        // Bypass permissions: all operations auto-approved
        if mode == PermissionMode::BypassPermissions {
            eprintln!("[Permission] Bypass mode: {} auto-approved", context.tool_name);
            self.audit_logger.log(
                AuditEntry::new(AuditAction::PermissionGranted, &context.tool_name, &context.conversation_id)
                    .with_user_id(context.user_id.as_deref())
                    .with_result("Bypass mode"),
            );
            return PermissionResult::Granted;
        }

        // Plan mode: only read-only operations allowed
        if mode == PermissionMode::PlanMode {
            if Self::is_read_only_tool(&context.tool_name) {
                eprintln!("[Permission] Plan mode: {} auto-approved (read-only)", context.tool_name);
                self.audit_logger.log(
                    AuditEntry::new(AuditAction::PermissionGranted, &context.tool_name, &context.conversation_id)
                        .with_user_id(context.user_id.as_deref())
                        .with_result("Plan mode: read-only"),
                );
                return PermissionResult::Granted;
            } else {
                let reason = "Plan mode: write operations are not allowed";
                eprintln!("[Permission] Plan mode: {} denied", context.tool_name);
                self.audit_logger.log(
                    AuditEntry::new(AuditAction::PermissionDenied, &context.tool_name, &context.conversation_id)
                        .with_user_id(context.user_id.as_deref())
                        .with_result(reason),
                );
                return PermissionResult::Denied(reason.to_string());
            }
        }

        // Accept edits mode: read and edit operations auto-approved, dangerous operations need confirmation
        if mode == PermissionMode::AcceptEdits {
            if Self::is_read_only_tool(&context.tool_name) || Self::is_edit_tool(&context.tool_name) {
                eprintln!("[Permission] Accept edits mode: {} auto-approved", context.tool_name);
                self.audit_logger.log(
                    AuditEntry::new(AuditAction::PermissionGranted, &context.tool_name, &context.conversation_id)
                        .with_user_id(context.user_id.as_deref())
                        .with_result("Accept edits mode"),
                );
                return PermissionResult::Granted;
            }
            // Dangerous tools still need confirmation in accept_edits mode
            if Self::is_dangerous_tool(&context.tool_name) {
                eprintln!("[Permission] Accept edits mode: {} requires confirmation (dangerous)", context.tool_name);
                return self.request_confirmation(context);
            }
        }

        // Ask permissions mode (default): fall through to existing logic
        self.audit_logger.log(
            AuditEntry::new(AuditAction::PermissionChecked, &context.tool_name, &context.conversation_id)
                .with_user_id(context.user_id.as_deref()),
        );

        let ruleset = self.ruleset.lock().unwrap();
        let rules = ruleset.get_rules_for_tool(&context.tool_name);
        
        for rule in rules {
            let result = self.evaluate_rule(&rule, context);
            if result != PermissionResult::Granted {
                return result;
            }
        }
        drop(ruleset);

        if DANGEROUS_TOOLS.contains(&context.tool_name.as_str()) {
            return self.handle_dangerous_tool(context);
        }

        let tool_permission = self.get_tool_permission(&context.tool_name);
        if let Some(perm) = tool_permission {
            if !perm.allowed {
                let result = PermissionResult::Denied(format!("Tool {} is not allowed", context.tool_name));
                self.audit_logger.log(
                    AuditEntry::new(AuditAction::PermissionDenied, &context.tool_name, &context.conversation_id)
                        .with_user_id(context.user_id.as_deref())
                        .with_result("Tool not allowed"),
                );
                return result;
            }
            if perm.requires_confirmation {
                return self.request_confirmation(context);
            }
        }

        self.audit_logger.log(
            AuditEntry::new(AuditAction::PermissionGranted, &context.tool_name, &context.conversation_id)
                .with_user_id(context.user_id.as_deref())
                .with_result("Granted"),
        );
        PermissionResult::Granted
    }

    fn evaluate_rule(&self, rule: &PermissionRule, context: &PermissionContext) -> PermissionResult {
        if let Some(tool_name) = &rule.tool_name {
            if tool_name != &context.tool_name {
                return PermissionResult::Granted;
            }
        }

        match rule.action {
            PermissionAction::Allow => PermissionResult::Granted,
            PermissionAction::Deny => {
                let reason = format!("Rule {} denies access", rule.id);
                self.audit_logger.log(
                    AuditEntry::new(AuditAction::PermissionDenied, &context.tool_name, &context.conversation_id)
                        .with_user_id(context.user_id.as_deref())
                        .with_result(&reason),
                );
                PermissionResult::Denied(reason)
            }
            PermissionAction::Confirm => {
                self.request_confirmation(context)
            }
        }
    }

    fn handle_dangerous_tool(&self, context: &PermissionContext) -> PermissionResult {
        let confirm_key = format!("{}-{}", context.conversation_id, context.tool_name);
        let confirmations = self.confirmations.lock().unwrap();
        
        if confirmations.get(&confirm_key) == Some(&true) {
            PermissionResult::Granted
        } else {
            self.request_confirmation(context)
        }
    }

    fn request_confirmation(&self, context: &PermissionContext) -> PermissionResult {
        let message = format!(
            "Tool '{}' requires user confirmation before execution",
            context.tool_name
        );
        PermissionResult::RequiresConfirmation(message)
    }

    pub fn confirm_permission(&self, conversation_id: &str, tool_name: &str) {
        let confirm_key = format!("{}-{}", conversation_id, tool_name);
        let mut confirmations = self.confirmations.lock().unwrap();
        confirmations.insert(confirm_key, true);

        self.audit_logger.log(
            AuditEntry::new(AuditAction::PermissionConfirmed, tool_name, conversation_id)
                .with_result("User confirmed"),
        );
    }

    pub fn clear_confirmation(&self, conversation_id: &str, tool_name: &str) {
        let confirm_key = format!("{}-{}", conversation_id, tool_name);
        let mut confirmations = self.confirmations.lock().unwrap();
        confirmations.remove(&confirm_key);
    }

    pub fn clear_all_confirmations(&self) {
        let mut confirmations = self.confirmations.lock().unwrap();
        confirmations.clear();
    }

    pub fn log_tool_execution(&self, context: &PermissionContext, success: bool) {
        let action = if success { AuditAction::ToolExecuted } else { AuditAction::ToolCancelled };
        let result = if success { "Success" } else { "Cancelled" };
        
        self.audit_logger.log(
            AuditEntry::new(action, &context.tool_name, &context.conversation_id)
                .with_user_id(context.user_id.as_deref())
                .with_result(result)
                .with_details(context.tool_input.clone()),
        );
    }

    pub fn is_tool_dangerous(&self, tool_name: &str) -> bool {
        DANGEROUS_TOOLS.contains(&tool_name)
    }
}

impl PermissionChecker for PermissionManager {
    fn check_permission(&self, context: &PermissionContext) -> PermissionResult {
        self.check_permission(context)
    }
}

pub type PermissionManagerRef = Arc<PermissionManager>;
