use serde::{Deserialize, Serialize};

const SELF_HOSTED_IDENTITY_BLOCK: &str = r#"<identity>
You are an AI assistant running in a desktop application called Claude Desktop. The app is built on top of the Claude Code engine, so you have the same powerful tool capabilities that Claude Code has — file operations (Read/Write/Edit), shell execution (Bash), code search (Glob/Grep), web tools (WebSearch/WebFetch), sub-agents (Task), Skills, and more — but the product you are powering is a general-purpose desktop assistant for everyday users, not a coding CLI.

Treat the user the way claude.ai would: with warmth, curiosity, presence, and depth. At the same time, when the user actually wants something built, read, run, or changed on disk — including handing you an entire code project to read or modify — do that work decisively and competently. Conversation-first does NOT mean code-shy.

You are NOT Kiro. You are NOT made by Amazon, AWS, or any Amazon product. You have no connection to Kiro. Do not create .kiro directories, do not produce Kiro-style configs, do not refer to yourself as Kiro under any circumstances.

Default to 简体中文 when the user writes in Chinese. Match the user's language otherwise.

Do not claim to be Claude or created by Anthropic unless the active model actually is a Claude-family model. If the active model is something else, describe yourself generically as the assistant in Claude Desktop.
</identity>"#;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemPromptConfig {
    pub custom_prompt: Option<String>,
    pub identity_block: Option<String>,
    pub remove_kiro_sections: bool,
    pub default_language: String,
}

impl Default for SystemPromptConfig {
    fn default() -> Self {
        Self {
            custom_prompt: None,
            identity_block: Some(SELF_HOSTED_IDENTITY_BLOCK.to_string()),
            remove_kiro_sections: true,
            default_language: "zh-CN".to_string(),
        }
    }
}

pub fn build_self_hosted_system_prompt(full_prompt: &str) -> String {
    let without_overrides = remove_override_sections(full_prompt);

    if contains_identity_block(&without_overrides) {
        return without_overrides
            .replace_batch(concat!(
                r"<identity>[\s\S]*?</identity>\s*",
            ), &format!("{}\n\n", SELF_HOSTED_IDENTITY_BLOCK))
            .trim()
            .to_string();
    }

    format!("{}\n\n{}", SELF_HOSTED_IDENTITY_BLOCK, without_overrides).trim().to_string()
}

fn remove_override_sections(prompt: &str) -> String {
    let re = regex::Regex::new(r"<override_instructions>[\s\S]*?</override_instructions>\s*").unwrap();
    re.replace_all(prompt, "").to_string()
}

fn contains_identity_block(prompt: &str) -> bool {
    let re = regex::Regex::new(r"<identity>[\s\S]*?</identity>").unwrap();
    re.is_match(prompt)
}

pub fn is_claude_family_model(model_id: &str) -> bool {
    let re = regex::Regex::new(r"^claude-").unwrap();
    re.is_match(model_id.trim())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResolvedModel {
    pub model_id: String,
    pub fallback_applied: bool,
    pub error: Option<String>,
}

pub fn resolve_requested_model_for_mode(
    model_id: Option<&str>,
    user_mode: &str,
    has_provider: bool,
) -> ResolvedModel {
    let normalized_model_id = model_id.unwrap_or("claude-sonnet-4-6").trim().to_string();
    let effective_user_mode = if user_mode == "selfhosted" {
        "selfhosted"
    } else {
        "clawparrot"
    };

    if effective_user_mode == "clawparrot" && !is_claude_family_model(&normalized_model_id) {
        return ResolvedModel {
            model_id: "claude-sonnet-4-6".to_string(),
            fallback_applied: true,
            error: None,
        };
    }

    if effective_user_mode == "selfhosted" && !has_provider && !is_claude_family_model(&normalized_model_id) {
        return ResolvedModel {
            model_id: normalized_model_id.clone(),
            fallback_applied: false,
            error: Some(format!(
                "No enabled self-hosted provider found for model \"{}\".",
                normalized_model_id
            )),
        };
    }

    ResolvedModel {
        model_id: normalized_model_id,
        fallback_applied: false,
        error: None,
    }
}

trait ReplaceAllExt {
    fn replace_batch(&self, pattern: &str, replacement: &str) -> String;
}

impl ReplaceAllExt for str {
    fn replace_batch(&self, pattern: &str, replacement: &str) -> String {
        let re = regex::Regex::new(pattern).unwrap();
        re.replace_all(self, replacement).to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_claude_family_model() {
        assert!(is_claude_family_model("claude-sonnet-4-6"));
        assert!(is_claude_family_model("claude-opus-4"));
        assert!(!is_claude_family_model("gpt-4"));
        assert!(!is_claude_family_model("gemini-pro"));
    }

    #[test]
    fn test_resolve_model_for_clawparrot() {
        let result = resolve_requested_model_for_mode(Some("gpt-4"), "clawparrot", false);
        assert!(result.fallback_applied);
        assert_eq!(result.model_id, "claude-sonnet-4-6");
    }

    #[test]
    fn test_resolve_model_for_selfhosted() {
        let result = resolve_requested_model_for_mode(Some("claude-sonnet-4-6"), "selfhosted", true);
        assert!(!result.fallback_applied);
        assert_eq!(result.model_id, "claude-sonnet-4-6");
    }
}
