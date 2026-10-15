//! Feedback Loop - Closes the learning cycle with performance adjustment
//!
//! Analyzes skill performance and adjusts recommendations based on:
//! - Historical success rates
//! - User feedback
//! - Context similarity
//! - Execution patterns

use crate::learning::experience::{ExperienceCollector, ExperienceOutcome};
use crate::learning::generator::GeneratedSkill;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Feedback {
    pub skill_id: String,
    pub experience_id: String,
    pub feedback_type: FeedbackType,
    pub rating: f32,
    pub timestamp: u64,
    pub context_hash: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum FeedbackType {
    Implicit,
    Explicit,
    OutcomeBased,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceMetric {
    pub skill_id: String,
    pub total_uses: usize,
    pub successful_uses: usize,
    pub average_rating: f32,
    pub average_duration_ms: u64,
    pub context_matches: usize,
    pub last_used: u64,
}

impl PerformanceMetric {
    pub fn success_rate(&self) -> f32 {
        if self.total_uses == 0 {
            return 0.0;
        }
        self.successful_uses as f32 / self.total_uses as f32
    }

    pub fn effectiveness_score(&self) -> f32 {
        let success_factor = self.success_rate();
        let rating_factor = self.average_rating;
        let recency_factor = self.compute_recency_factor();
        (success_factor * 0.5 + rating_factor * 0.3 + recency_factor * 0.2).clamp(0.0, 1.0)
    }

    fn compute_recency_factor(&self) -> f32 {
        let now = chrono::Utc::now().timestamp() as u64;
        let hours_since_use = (now - self.last_used) / 3600;
        match hours_since_use {
            0..=24 => 1.0,
            25..=72 => 0.8,
            73..=168 => 0.6,
            _ => 0.4,
        }
    }
}

pub struct FeedbackLoop {
    feedback_records: Arc<RwLock<Vec<Feedback>>>,
    skill_metrics: Arc<RwLock<HashMap<String, PerformanceMetric>>>,
    context_adjustments: Arc<RwLock<HashMap<String, f32>>>,
}

impl FeedbackLoop {
    pub fn new() -> Self {
        Self {
            feedback_records: Arc::new(RwLock::new(Vec::new())),
            skill_metrics: Arc::new(RwLock::new(HashMap::new())),
            context_adjustments: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn record_feedback(&self, feedback: Feedback) {
        let fb = feedback.clone();
        {
            let mut records = self.feedback_records.write().await;
            records.push(fb);
        }
        self.update_metrics_from_feedback(&feedback).await;
    }

    async fn update_metrics_from_feedback(&self, feedback: &Feedback) {
        let mut metrics = self.skill_metrics.write().await;
        let metric = metrics.entry(feedback.skill_id.clone()).or_insert_with(|| PerformanceMetric {
            skill_id: feedback.skill_id.clone(),
            total_uses: 0,
            successful_uses: 0,
            average_rating: 0.0,
            average_duration_ms: 0,
            context_matches: 0,
            last_used: 0,
        });

        metric.total_uses += 1;
        metric.average_rating = (metric.average_rating * (metric.total_uses - 1) as f32 + feedback.rating) / metric.total_uses as f32;

        if feedback.rating >= 0.7 {
            metric.successful_uses += 1;
        }

        metric.last_used = feedback.timestamp;
    }

    pub async fn get_skill_adjustment(&self, skill_id: &str, context_similarity: f32) -> f32 {
        let metrics = self.skill_metrics.read().await;
        if let Some(metric) = metrics.get(skill_id) {
            let effectiveness = metric.effectiveness_score();
            let context_bonus = if context_similarity > 0.8 {
                0.1
            } else if context_similarity > 0.5 {
                0.0
            } else {
                -0.1
            };
            effectiveness * 0.7 + context_similarity * 0.3 + context_bonus
        } else {
            0.5
        }
    }

    pub async fn update_from_experience(&self, skill_id: &str, outcome: ExperienceOutcome, duration_ms: u64) {
        let feedback = Feedback {
            skill_id: skill_id.to_string(),
            experience_id: uuid::Uuid::new_v4().to_string(),
            feedback_type: FeedbackType::OutcomeBased,
            rating: match outcome {
                ExperienceOutcome::Success => 1.0,
                ExperienceOutcome::PartialSuccess => 0.6,
                ExperienceOutcome::Failure => 0.0,
                ExperienceOutcome::Timeout => 0.3,
                ExperienceOutcome::Cancelled => 0.0,
            },
            timestamp: chrono::Utc::now().timestamp() as u64,
            context_hash: 0,
        };
        self.record_feedback(feedback).await;

        let mut metrics = self.skill_metrics.write().await;
        let metric = metrics.entry(skill_id.to_string()).or_insert_with(|| PerformanceMetric {
            skill_id: skill_id.to_string(),
            total_uses: 0,
            successful_uses: 0,
            average_rating: 0.0,
            average_duration_ms: 0,
            context_matches: 0,
            last_used: 0,
        });

        metric.total_uses += 1;
        metric.average_duration_ms = (metric.average_duration_ms * (metric.total_uses - 1) as u64 + duration_ms) / metric.total_uses as u64;
        metric.last_used = chrono::Utc::now().timestamp() as u64;
    }

    pub async fn rank_skills_for_context(&self, skills: &[GeneratedSkill], context: &str) -> Vec<(String, f32)> {
        let mut rankings: Vec<(String, f32)> = Vec::new();
        let context_lower = context.to_lowercase();

        for skill in skills {
            let mut score = skill.confidence;

            if let Some(adjustment) = self.context_adjustments.read().await.get(&skill.id) {
                score *= adjustment;
            }

            for trigger in &skill.template.trigger_patterns {
                if context_lower.contains(&trigger.to_lowercase()) {
                    score += 0.15;
                    break;
                }
            }

            let metrics = self.skill_metrics.read().await;
            if let Some(metric) = metrics.get(&skill.id) {
                score = score * 0.6 + metric.effectiveness_score() * 0.4;
            }

            rankings.push((skill.id.clone(), score.min(1.0)));
        }

        rankings.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        rankings
    }

    pub async fn adjust_for_context(&self, skill_id: &str, context_similarity: f32) {
        let adjustment = match context_similarity {
            s if s >= 0.9 => 1.2,
            s if s >= 0.7 => 1.1,
            s if s >= 0.5 => 1.0,
            s if s >= 0.3 => 0.9,
            _ => 0.8,
        };

        let mut adjustments = self.context_adjustments.write().await;
        adjustments.insert(skill_id.to_string(), adjustment);
    }

    pub async fn get_metrics(&self, skill_id: &str) -> Option<PerformanceMetric> {
        let metrics = self.skill_metrics.read().await;
        metrics.get(skill_id).cloned()
    }

    pub async fn get_all_metrics(&self) -> Vec<PerformanceMetric> {
        let metrics = self.skill_metrics.read().await;
        metrics.values().cloned().collect()
    }
}

impl Default for FeedbackLoop {
    fn default() -> Self {
        Self::new()
    }
}

impl Clone for FeedbackLoop {
    fn clone(&self) -> Self {
        Self {
            feedback_records: self.feedback_records.clone(),
            skill_metrics: self.skill_metrics.clone(),
            context_adjustments: self.context_adjustments.clone(),
        }
    }
}
