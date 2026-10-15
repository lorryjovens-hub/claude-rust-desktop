//! SkillTool - Skill/Capability management system
//!
//! Provides skill registration, discovery, and execution for agent capabilities.

use crate::tools::{Tool, ToolOutput, ToolError};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Skill {
    pub id: String,
    pub name: String,
    pub description: String,
    pub category: SkillCategory,
    pub parameters: Vec<SkillParameter>,
    pub enabled: bool,
    pub created_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum SkillCategory {
    Coding,
    Debugging,
    Refactoring,
    Testing,
    Documentation,
    Analysis,
    Research,
    Communication,
    Custom,
}

impl std::fmt::Display for SkillCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SkillCategory::Coding => write!(f, "coding"),
            SkillCategory::Debugging => write!(f, "debugging"),
            SkillCategory::Refactoring => write!(f, "refactoring"),
            SkillCategory::Testing => write!(f, "testing"),
            SkillCategory::Documentation => write!(f, "documentation"),
            SkillCategory::Analysis => write!(f, "analysis"),
            SkillCategory::Research => write!(f, "research"),
            SkillCategory::Communication => write!(f, "communication"),
            SkillCategory::Custom => write!(f, "custom"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillParameter {
    pub name: String,
    pub param_type: String,
    pub description: String,
    pub required: bool,
    pub default: Option<String>,
}

impl Skill {
    pub fn new(name: String, description: String, category: SkillCategory) -> Self {
        Self {
            id: format!("skill_{}", SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_millis()),
            name,
            description,
            category,
            parameters: Vec::new(),
            enabled: true,
            created_at: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs(),
        }
    }

    pub fn add_parameter(&mut self, name: String, param_type: String, description: String, required: bool, default: Option<String>) {
        self.parameters.push(SkillParameter {
            name,
            param_type,
            description,
            required,
            default,
        });
    }
}

pub struct SkillStore {
    skills: HashMap<String, Skill>,
}

impl Default for SkillStore {
    fn default() -> Self {
        Self::new()
    }
}

impl SkillStore {
    pub fn new() -> Self {
        Self {
            skills: HashMap::new(),
        }
    }

    pub fn register_skill(&mut self, skill: Skill) {
        self.skills.insert(skill.id.clone(), skill);
    }

    pub fn get_skill(&self, id: &str) -> Option<&Skill> {
        self.skills.get(id)
    }

    pub fn list_skills(&self, category: Option<SkillCategory>, enabled_only: bool) -> Vec<&Skill> {
        self.skills.values()
            .filter(|s| {
                let category_match = category.as_ref().map_or(true, |c| &s.category == c);
                let enabled_match = !enabled_only || s.enabled;
                category_match && enabled_match
            })
            .collect()
    }

    pub fn enable_skill(&mut self, id: &str, enabled: bool) -> bool {
        if let Some(skill) = self.skills.get_mut(id) {
            skill.enabled = enabled;
            return true;
        }
        false
    }

    pub fn delete_skill(&mut self, id: &str) -> bool {
        self.skills.remove(id).is_some()
    }
}

pub type SharedSkillStore = Arc<RwLock<SkillStore>>;

pub fn create_shared_skill_store() -> SharedSkillStore {
    Arc::new(RwLock::new(SkillStore::new()))
}

static SKILL_STORE: std::sync::OnceLock<SharedSkillStore> = std::sync::OnceLock::new();

pub fn get_skill_store() -> SharedSkillStore {
    SKILL_STORE.get_or_init(create_shared_skill_store).clone()
}

fn init_default_skills(store: &mut SkillStore) {
    let mut coding_skill = Skill::new(
        "code_generation".to_string(),
        "Generate code based on specifications".to_string(),
        SkillCategory::Coding,
    );
    coding_skill.add_parameter("language".to_string(), "string".to_string(), "Programming language".to_string(), true, None);
    coding_skill.add_parameter("spec".to_string(), "string".to_string(), "Code specification".to_string(), true, None);
    store.register_skill(coding_skill);

    let mut debug_skill = Skill::new(
        "debug_analysis".to_string(),
        "Analyze and debug code issues".to_string(),
        SkillCategory::Debugging,
    );
    debug_skill.add_parameter("error".to_string(), "string".to_string(), "Error message or description".to_string(), true, None);
    debug_skill.add_parameter("context".to_string(), "string".to_string(), "Additional context".to_string(), false, None);
    store.register_skill(debug_skill);

    let mut test_skill = Skill::new(
        "test_generation".to_string(),
        "Generate unit tests for code".to_string(),
        SkillCategory::Testing,
    );
    test_skill.add_parameter("language".to_string(), "string".to_string(), "Programming language".to_string(), true, None);
    test_skill.add_parameter("framework".to_string(), "string".to_string(), "Testing framework".to_string(), false, Some("pytest".to_string()));
    store.register_skill(test_skill);

    let mut refactor_skill = Skill::new(
        "code_refactor".to_string(),
        "Refactor code for better quality".to_string(),
        SkillCategory::Refactoring,
    );
    refactor_skill.add_parameter("goal".to_string(), "string".to_string(), "Refactoring goal".to_string(), true, None);
    store.register_skill(refactor_skill);

    let mut docs_skill = Skill::new(
        "documentation".to_string(),
        "Generate documentation for code".to_string(),
        SkillCategory::Documentation,
    );
    docs_skill.add_parameter("format".to_string(), "string".to_string(), "Documentation format".to_string(), false, Some("markdown".to_string()));
    store.register_skill(docs_skill);
}

#[derive(Debug, Clone)]
pub struct SkillTool;

impl SkillTool {
    pub fn new() -> Self {
        Self
    }

    pub fn init_default_skills() {
        let store = get_skill_store();
        let mut store = store.blocking_write();
        init_default_skills(&mut store);
    }
}

impl Default for SkillTool {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Deserialize)]
struct SkillInput {
    operation: String,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    category: Option<String>,
    #[serde(default)]
    skill_id: Option<String>,
    #[serde(default)]
    enabled: Option<bool>,
    #[serde(default)]
    parameters: Option<Vec<SkillParameter>>,
}

fn parse_category(cat: &str) -> Option<SkillCategory> {
    match cat.to_lowercase().as_str() {
        "coding" => Some(SkillCategory::Coding),
        "debugging" => Some(SkillCategory::Debugging),
        "refactoring" => Some(SkillCategory::Refactoring),
        "testing" => Some(SkillCategory::Testing),
        "documentation" => Some(SkillCategory::Documentation),
        "analysis" => Some(SkillCategory::Analysis),
        "research" => Some(SkillCategory::Research),
        "communication" => Some(SkillCategory::Communication),
        "custom" => Some(SkillCategory::Custom),
        _ => None,
    }
}

#[async_trait]
impl Tool for SkillTool {
    fn name(&self) -> &str {
        "Skill"
    }

    fn description(&self) -> &str {
        "Manage agent skills/capabilities. Operations: register, list, get, enable, disable, delete"
    }

    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "operation": {
                    "type": "string",
                    "enum": ["register", "list", "get", "enable", "disable", "delete", "init_defaults"],
                    "description": "Operation to perform"
                },
                "name": {
                    "type": "string",
                    "description": "Skill name"
                },
                "description": {
                    "type": "string",
                    "description": "Skill description"
                },
                "category": {
                    "type": "string",
                    "enum": ["coding", "debugging", "refactoring", "testing", "documentation", "analysis", "research", "communication", "custom"],
                    "description": "Skill category"
                },
                "skill_id": {
                    "type": "string",
                    "description": "Skill ID"
                },
                "enabled": {
                    "type": "boolean",
                    "description": "Enable/disable skill"
                },
                "parameters": {
                    "type": "array",
                    "description": "Skill parameters"
                }
            },
            "required": ["operation"]
        })
    }

    async fn execute(&self, input: serde_json::Value) -> Result<ToolOutput, ToolError> {
        let input: SkillInput = serde_json::from_value(input)
            .map_err(|e| ToolError {
                message: format!("Invalid input: {}", e),
                code: Some("invalid_input".to_string()),
            })?;

        match input.operation.as_str() {
            "register" => {
                let name = input.name.ok_or_else(|| ToolError {
                    message: "Skill 'name' is required for register operation".to_string(),
                    code: Some("missing_field".to_string()),
                })?;
                let description = input.description.unwrap_or_default();
                let category = parse_category(&input.category.unwrap_or_else(|| "custom".to_string()))
                    .unwrap_or(SkillCategory::Custom);

                let mut skill = Skill::new(name, description, category);
                if let Some(params) = input.parameters {
                    for p in params {
                        skill.add_parameter(p.name, p.param_type, p.description, p.required, p.default);
                    }
                }

                let store = get_skill_store();
                let mut store = store.write().await;
                store.register_skill(skill.clone());

                Ok(ToolOutput {
                    output_type: "json".to_string(),
                    content: serde_json::to_string_pretty(&skill).unwrap(),
                    metadata: HashMap::new(),
                })
            }
            "list" => {
                let category = input.category.as_ref().and_then(|c| parse_category(c));
                let enabled_only = input.enabled.unwrap_or(false);

                let store = get_skill_store();
                let store = store.read().await;
                let skills = store.list_skills(category, enabled_only);

                let skill_summaries: Vec<serde_json::Value> = skills.iter().map(|s| {
                    serde_json::json!({
                        "id": s.id,
                        "name": s.name,
                        "description": s.description,
                        "category": s.category.to_string(),
                        "enabled": s.enabled,
                        "parameters": s.parameters,
                    })
                }).collect();

                Ok(ToolOutput {
                    output_type: "json".to_string(),
                    content: serde_json::to_string_pretty(&skill_summaries).unwrap(),
                    metadata: HashMap::new(),
                })
            }
            "get" => {
                let skill_id = input.skill_id.ok_or_else(|| ToolError {
                    message: "Skill 'skill_id' is required for get operation".to_string(),
                    code: Some("missing_field".to_string()),
                })?;

                let store = get_skill_store();
                let store = store.read().await;
                let skill = store.get_skill(&skill_id)
                    .ok_or_else(|| ToolError {
                        message: format!("Skill not found: {}", skill_id),
                        code: Some("not_found".to_string()),
                    })?;

                Ok(ToolOutput {
                    output_type: "json".to_string(),
                    content: serde_json::to_string_pretty(&skill).unwrap(),
                    metadata: HashMap::new(),
                })
            }
            "enable" => {
                let skill_id = input.skill_id.ok_or_else(|| ToolError {
                    message: "Skill 'skill_id' is required for enable operation".to_string(),
                    code: Some("missing_field".to_string()),
                })?;

                let store = get_skill_store();
                let mut store = store.write().await;
                let success = store.enable_skill(&skill_id, true);

                Ok(ToolOutput {
                    output_type: "json".to_string(),
                    content: serde_json::to_string_pretty(&serde_json::json!({
                        "success": success,
                        "skill_id": skill_id
                    })).unwrap(),
                    metadata: HashMap::new(),
                })
            }
            "disable" => {
                let skill_id = input.skill_id.ok_or_else(|| ToolError {
                    message: "Skill 'skill_id' is required for disable operation".to_string(),
                    code: Some("missing_field".to_string()),
                })?;

                let store = get_skill_store();
                let mut store = store.write().await;
                let success = store.enable_skill(&skill_id, false);

                Ok(ToolOutput {
                    output_type: "json".to_string(),
                    content: serde_json::to_string_pretty(&serde_json::json!({
                        "success": success,
                        "skill_id": skill_id
                    })).unwrap(),
                    metadata: HashMap::new(),
                })
            }
            "delete" => {
                let skill_id = input.skill_id.ok_or_else(|| ToolError {
                    message: "Skill 'skill_id' is required for delete operation".to_string(),
                    code: Some("missing_field".to_string()),
                })?;

                let store = get_skill_store();
                let mut store = store.write().await;
                let success = store.delete_skill(&skill_id);

                Ok(ToolOutput {
                    output_type: "json".to_string(),
                    content: serde_json::to_string_pretty(&serde_json::json!({
                        "success": success,
                        "skill_id": skill_id
                    })).unwrap(),
                    metadata: HashMap::new(),
                })
            }
            "init_defaults" => {
                let store = get_skill_store();
                let mut store = store.write().await;
                init_default_skills(&mut store);

                Ok(ToolOutput {
                    output_type: "json".to_string(),
                    content: serde_json::to_string_pretty(&serde_json::json!({
                        "message": "Default skills initialized"
                    })).unwrap(),
                    metadata: HashMap::new(),
                })
            }
            _ => Err(ToolError {
                message: format!("Unknown operation: {}", input.operation),
                code: Some("unknown_operation".to_string()),
            }),
        }
    }
}