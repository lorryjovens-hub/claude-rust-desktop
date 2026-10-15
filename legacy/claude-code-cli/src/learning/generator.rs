//! Skill Generator - Creates new skills from extracted patterns
//!
//! Analyzes patterns and generates new skill definitions that can be
//! registered and used for future tasks.

use crate::learning::experience::SkillStatistics;
use crate::learning::pattern::{Pattern, PatternType};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillTemplate {
    pub name: String,
    pub description: String,
    pub category: String,
    pub trigger_patterns: Vec<String>,
    pub recommended_tools: Vec<String>,
    pub steps: Vec<SkillStep>,
    pub success_rate: f32,
    pub avg_duration_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillStep {
    pub order: usize,
    pub action: String,
    pub tool_hint: Option<String>,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneratedSkill {
    pub id: String,
    pub template: SkillTemplate,
    pub source_patterns: Vec<String>,
    pub confidence: f32,
    pub created_at: u64,
    pub times_suggested: usize,
    pub times_accepted: usize,
    pub times_executed: usize,
    pub success_count: usize,
}

impl GeneratedSkill {
    pub fn adoption_rate(&self) -> f32 {
        if self.times_suggested == 0 {
            return 0.0;
        }
        self.times_accepted as f32 / self.times_suggested as f32
    }

    pub fn execution_success_rate(&self) -> f32 {
        if self.times_executed == 0 {
            return 0.0;
        }
        self.success_count as f32 / self.times_executed as f32
    }

    pub fn usefulness_score(&self) -> f32 {
        (self.adoption_rate() + self.execution_success_rate()) / 2.0
    }
}

pub struct SkillGenerator {
    generated_skills: Arc<RwLock<Vec<GeneratedSkill>>>,
    skill_templates: Arc<RwLock<Vec<SkillTemplate>>>,
    confidence_threshold: f32,
}

impl SkillGenerator {
    pub fn new(confidence_threshold: f32) -> Self {
        Self {
            generated_skills: Arc::new(RwLock::new(Vec::new())),
            skill_templates: Arc::new(RwLock::new(Vec::new())),
            confidence_threshold,
        }
    }

    pub async fn generate_from_patterns(&self, patterns: &[Pattern]) -> Vec<GeneratedSkill> {
        let mut generated = Vec::new();

        let tool_sequences: Vec<_> = patterns
            .iter()
            .filter(|p| p.pattern_type == PatternType::ToolSequence && p.confidence.score >= self.confidence_threshold)
            .collect();

        if tool_sequences.len() >= 2 {
            if let Some(skill) = self.create_workflow_skill(&tool_sequences).await {
                generated.push(skill);
            }
        }

        let command_patterns: Vec<_> = patterns
            .iter()
            .filter(|p| p.pattern_type == PatternType::CommandPattern && p.confidence.score >= self.confidence_threshold)
            .collect();

        for pattern in command_patterns {
            if let Some(skill) = self.create_command_skill(pattern).await {
                generated.push(skill);
            }
        }

        let mut skills = self.generated_skills.write().await;
        for new_skill in &generated {
            if !skills.iter().any(|s| s.template.name == new_skill.template.name) {
                skills.push(new_skill.clone());
            }
        }

        generated
    }

    async fn create_workflow_skill(&self, patterns: &[&Pattern]) -> Option<GeneratedSkill> {
        if patterns.is_empty() {
            return None;
        }

        let mut all_tools: Vec<String> = Vec::new();
        let mut total_duration = 0u64;
        let mut total_success = 0f32;

        for p in patterns {
            for tool in &p.key_elements {
                if !all_tools.contains(tool) {
                    all_tools.push(tool.clone());
                }
            }
            total_duration += p.average_duration_ms;
            total_success += p.success_rate;
        }

        let avg_duration = total_duration / patterns.len() as u64;
        let avg_success = total_success / patterns.len() as f32;

        let first_pattern = patterns.first()?;
        let workflow_name = self.generate_workflow_name(&first_pattern.key_elements);

        let template = SkillTemplate {
            name: workflow_name.clone(),
            description: format!("Workflow for {} tasks (generated from {} patterns)", first_pattern.key_elements.join(" + "), patterns.len()),
            category: "generated".to_string(),
            trigger_patterns: vec![
                format!("{} task", first_pattern.key_elements.first().unwrap()),
                format!("work with {}", first_pattern.key_elements.join(" and ")),
            ],
            recommended_tools: all_tools,
            steps: patterns.iter().enumerate().map(|(i, p)| {
                SkillStep {
                    order: i + 1,
                    action: format!("Execute: {}", p.key_elements.join(" then ")),
                    tool_hint: p.key_elements.first().cloned(),
                    description: p.description.clone(),
                }
            }).collect(),
            success_rate: avg_success,
            avg_duration_ms: avg_duration,
        };

        Some(GeneratedSkill {
            id: format!("gen_{}", workflow_name.replace(' ', "_").to_lowercase()),
            template,
            source_patterns: patterns.iter().map(|p| p.id.clone()).collect(),
            confidence: patterns.iter().map(|p| p.confidence.score).sum::<f32>() / patterns.len() as f32,
            created_at: chrono::Utc::now().timestamp() as u64,
            times_suggested: 0,
            times_accepted: 0,
            times_executed: 0,
            success_count: 0,
        })
    }

    async fn create_command_skill(&self, pattern: &Pattern) -> Option<GeneratedSkill> {
        let command = pattern.key_elements.first()?;
        let command_name = command.split_whitespace().last().unwrap_or(command);

        let template = SkillTemplate {
            name: format!("run_{}", command_name),
            description: pattern.description.clone(),
            category: "generated".to_string(),
            trigger_patterns: vec![
                format!("run {}", command_name),
                format!("execute {}", command_name),
                format!("{}", command_name),
            ],
            recommended_tools: vec![command.clone()],
            steps: vec![
                SkillStep {
                    order: 1,
                    action: format!("Execute: {}", command),
                    tool_hint: Some("terminal".to_string()),
                    description: format!("Run the {} command", command_name),
                }
            ],
            success_rate: pattern.success_rate,
            avg_duration_ms: pattern.average_duration_ms,
        };

        Some(GeneratedSkill {
            id: format!("gen_cmd_{}", command_name.to_lowercase()),
            template,
            source_patterns: vec![pattern.id.clone()],
            confidence: pattern.confidence.score,
            created_at: chrono::Utc::now().timestamp() as u64,
            times_suggested: 0,
            times_accepted: 0,
            times_executed: 0,
            success_count: 0,
        })
    }

    fn generate_workflow_name(&self, tools: &[String]) -> String {
        if tools.is_empty() {
            return "unnamed_workflow".to_string();
        }

        let main_tool = tools.first().unwrap();
        let main_name = main_tool.replace(['_', '-'], " ");

        if tools.len() == 1 {
            format!("workflow_{}", main_name.replace(' ', "_").to_lowercase())
        } else {
            format!("{}_workflow", main_name.replace(' ', "_").to_lowercase())
        }
    }

    pub async fn get_suggestions(&self, context: &str) -> Vec<GeneratedSkill> {
        let skills = self.generated_skills.read().await;
        let context_lower = context.to_lowercase();

        skills
            .iter()
            .filter(|s| {
                s.times_accepted == 0 &&
                (s.template.trigger_patterns.iter().any(|p| context_lower.contains(&p.to_lowercase())) ||
                 s.template.description.to_lowercase().contains(&context_lower))
            })
            .filter(|s| s.usefulness_score() >= 0.5 || s.confidence >= 0.8)
            .cloned()
            .collect()
    }

    pub async fn record_suggestion(&self, skill_id: &str) {
        let mut skills = self.generated_skills.write().await;
        if let Some(skill) = skills.iter_mut().find(|s| s.id == skill_id) {
            skill.times_suggested += 1;
        }
    }

    pub async fn record_acceptance(&self, skill_id: &str) {
        let mut skills = self.generated_skills.write().await;
        if let Some(skill) = skills.iter_mut().find(|s| s.id == skill_id) {
            skill.times_accepted += 1;
        }
    }

    pub async fn record_execution(&self, skill_id: &str, success: bool) {
        let mut skills = self.generated_skills.write().await;
        if let Some(skill) = skills.iter_mut().find(|s| s.id == skill_id) {
            skill.times_executed += 1;
            if success {
                skill.success_count += 1;
            }
        }
    }

    pub async fn get_all_generated_skills(&self) -> Vec<GeneratedSkill> {
        let skills = self.generated_skills.read().await;
        skills.clone()
    }

    pub async fn get_top_skills(&self, limit: usize) -> Vec<GeneratedSkill> {
        let skills = self.generated_skills.read().await;
        let mut sorted = skills.clone();
        sorted.sort_by(|a, b| {
            b.usefulness_score()
                .partial_cmp(&a.usefulness_score())
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        sorted.into_iter().take(limit).collect()
    }
}

impl Default for SkillGenerator {
    fn default() -> Self {
        Self::new(0.7)
    }
}

impl Clone for SkillGenerator {
    fn clone(&self) -> Self {
        Self {
            generated_skills: self.generated_skills.clone(),
            skill_templates: self.skill_templates.clone(),
            confidence_threshold: self.confidence_threshold,
        }
    }
}
