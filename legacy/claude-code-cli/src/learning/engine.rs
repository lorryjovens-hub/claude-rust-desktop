//! Learning Engine - Orchestrates all learning components
//!
//! The central coordinator that manages:
//! - Experience collection from all skill executions
//! - Pattern extraction from accumulated experiences
//! - Skill generation from discovered patterns
//! - Feedback processing and strategy adjustment
//! - Learning state persistence

use crate::learning::experience::{ExperienceCollector, Experience, ExperienceOutcome, SkillStatistics};
use crate::learning::pattern::{PatternExtractor, Pattern, PatternType};
use crate::learning::generator::{SkillGenerator, GeneratedSkill};
use crate::learning::feedback::{FeedbackLoop, Feedback, FeedbackType, PerformanceMetric};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::RwLock;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LearningConfig {
    pub enabled: bool,
    pub min_pattern_occurrences: usize,
    pub skill_generation_threshold: f32,
    pub auto_suggest_threshold: f32,
    pub learning_session_timeout_secs: u64,
    pub max_experiences_stored: usize,
    pub auto_extract_interval_secs: u64,
}

impl Default for LearningConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            min_pattern_occurrences: 3,
            skill_generation_threshold: 0.75,
            auto_suggest_threshold: 0.7,
            learning_session_timeout_secs: 3600,
            max_experiences_stored: 1000,
            auto_extract_interval_secs: 300,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LearningStats {
    pub total_experiences: usize,
    pub total_patterns_discovered: usize,
    pub total_skills_generated: usize,
    pub total_skill_suggestions: usize,
    pub total_feedback_received: usize,
    pub active_sessions: usize,
    pub last_extraction_time: u64,
    pub learning_uptime_secs: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LearningSuggestion {
    pub skill: GeneratedSkill,
    pub confidence: f32,
    pub reason: String,
    pub context_preview: String,
}

pub struct LearningEngine {
    config: LearningConfig,
    experience_collector: ExperienceCollector,
    pattern_extractor: PatternExtractor,
    skill_generator: SkillGenerator,
    feedback_loop: FeedbackLoop,
    sessions: Arc<RwLock<HashMap<String, LearningSession>>>,
    started_at: u64,
}

struct LearningSession {
    session_id: String,
    started_at: u64,
    last_activity: u64,
    experiences_gathered: usize,
}

impl LearningEngine {
    pub fn new(config: LearningConfig) -> Self {
        let started_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        Self {
            config: config.clone(),
            experience_collector: ExperienceCollector::new(config.max_experiences_stored),
            pattern_extractor: PatternExtractor::new(config.min_pattern_occurrences),
            skill_generator: SkillGenerator::new(config.skill_generation_threshold as f32),
            feedback_loop: FeedbackLoop::new(),
            sessions: Arc::new(RwLock::new(HashMap::new())),
            started_at,
        }
    }

    pub fn with_defaults() -> Self {
        Self::new(LearningConfig::default())
    }

    pub async fn start_session(&self, session_id: String) {
        let mut sessions = self.sessions.write().await;
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        sessions.insert(session_id.clone(), LearningSession {
            session_id,
            started_at: now,
            last_activity: now,
            experiences_gathered: 0,
        });
    }

    pub async fn end_session(&self, session_id: &str) {
        let mut sessions = self.sessions.write().await;
        sessions.remove(session_id);
    }

    pub async fn record_experience(
        &self,
        context: String,
        intent: String,
        skill_used: Option<String>,
        tools_used: Vec<String>,
        outcome: ExperienceOutcome,
        duration_ms: u64,
        session_id: String,
    ) {
        let experience = Experience::new(
            context.clone(),
            intent,
            skill_used.clone(),
            tools_used.clone(),
            outcome.clone(),
            duration_ms,
            session_id.clone(),
        );

        self.experience_collector.record(experience).await;

        if let Some(ref skill_name) = skill_used {
            self.feedback_loop.update_from_experience(skill_name, outcome, duration_ms).await;
        }

        if let Some(session) = self.sessions.write().await.get_mut(&session_id) {
            session.last_activity = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs();
            session.experiences_gathered += 1;
        }
    }

    pub async fn record_user_feedback(&self, skill_id: String, experience_id: String, rating: f32) {
        let feedback = Feedback {
            skill_id,
            experience_id,
            feedback_type: FeedbackType::Explicit,
            rating,
            timestamp: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            context_hash: 0,
        };

        self.feedback_loop.record_feedback(feedback).await;
    }

    pub async fn extract_patterns(&self) -> Vec<Pattern> {
        let experiences = self.experience_collector.get_successful_patterns().await;
        self.pattern_extractor.extract_from_experiences(&experiences).await
    }

    pub async fn generate_skills(&self) -> Vec<GeneratedSkill> {
        let patterns = self.extract_patterns().await;
        self.skill_generator.generate_from_patterns(&patterns).await
    }

    pub async fn get_suggestions_for_context(&self, context: &str) -> Vec<LearningSuggestion> {
        let all_skills = self.skill_generator.get_all_generated_skills().await;
        let ranked = self.feedback_loop.rank_skills_for_context(&all_skills, context).await;

        let mut suggestions = Vec::new();
        for (skill_id, score) in ranked {
            if score >= self.config.auto_suggest_threshold as f32 {
                if let Some(skill) = all_skills.iter().find(|s| s.id == skill_id) {
                    suggestions.push(LearningSuggestion {
                        skill: skill.clone(),
                        confidence: score,
                        reason: format!("Matched {} trigger patterns", skill.template.trigger_patterns.len()),
                        context_preview: context.chars().take(100).collect(),
                    });
                }
            }
        }

        suggestions
    }

    pub async fn check_for_new_skills(&self) -> Option<GeneratedSkill> {
        let patterns = self.extract_patterns().await;
        let new_skills = self.skill_generator.generate_from_patterns(&patterns).await;
        new_skills.into_iter().max_by_key(|s| s.confidence as u32)
    }

    pub async fn suggest_skill(&self, skill: &GeneratedSkill) {
        self.skill_generator.record_suggestion(&skill.id).await;
    }

    pub async fn accept_skill(&self, skill_id: &str) -> bool {
        let skills = self.skill_generator.get_all_generated_skills().await;
        if skills.iter().any(|s| s.id == skill_id) {
            self.skill_generator.record_acceptance(skill_id).await;
            true
        } else {
            false
        }
    }

    pub async fn record_skill_usage(&self, skill_id: &str, success: bool) {
        self.skill_generator.record_execution(skill_id, success).await;
    }

    pub async fn get_learning_stats(&self) -> LearningStats {
        let active_sessions = self.sessions.read().await.len() as usize;
        let patterns = self.pattern_extractor.get_all_patterns().await;
        let skills = self.skill_generator.get_all_generated_skills().await;

        LearningStats {
            total_experiences: self.experience_collector.count().await,
            total_patterns_discovered: patterns.len(),
            total_skills_generated: skills.len(),
            total_skill_suggestions: skills.iter().map(|s| s.times_suggested).sum(),
            total_feedback_received: 0,
            active_sessions,
            last_extraction_time: chrono::Utc::now().timestamp() as u64,
            learning_uptime_secs: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs()
                - self.started_at,
        }
    }

    pub async fn get_skill_recommendations(&self, context: &str, limit: usize) -> Vec<(GeneratedSkill, f32)> {
        let suggestions = self.get_suggestions_for_context(context).await;
        suggestions
            .into_iter()
            .take(limit)
            .map(|s| (s.skill, s.confidence))
            .collect()
    }

    pub async fn get_patterns_by_type(&self, pattern_type: PatternType) -> Vec<Pattern> {
        self.pattern_extractor.get_patterns_by_type(pattern_type).await
    }

    pub async fn get_skill_metrics(&self, skill_id: &str) -> Option<PerformanceMetric> {
        self.feedback_loop.get_metrics(skill_id).await
    }

    pub async fn get_top_performing_skills(&self, limit: usize) -> Vec<(GeneratedSkill, PerformanceMetric)> {
        let skills = self.skill_generator.get_top_skills(limit).await;
        let mut results = Vec::new();

        for skill in skills {
            if let Some(metric) = self.feedback_loop.get_metrics(&skill.id).await {
                results.push((skill, metric));
            }
        }

        results
    }

    pub async fn get_experience_summary(&self, skill_name: &str) -> SkillStatistics {
        self.experience_collector.get_skill_statistics(skill_name).await
    }

    pub fn is_enabled(&self) -> bool {
        self.config.enabled
    }

    pub fn set_enabled(&mut self, enabled: bool) {
        self.config.enabled = enabled;
    }
}

impl Default for LearningEngine {
    fn default() -> Self {
        Self::with_defaults()
    }
}

impl Clone for LearningEngine {
    fn clone(&self) -> Self {
        Self {
            config: self.config.clone(),
            experience_collector: self.experience_collector.clone(),
            pattern_extractor: self.pattern_extractor.clone(),
            skill_generator: self.skill_generator.clone(),
            feedback_loop: self.feedback_loop.clone(),
            sessions: self.sessions.clone(),
            started_at: self.started_at,
        }
    }
}
