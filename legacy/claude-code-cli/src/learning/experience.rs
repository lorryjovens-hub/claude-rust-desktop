//! Experience Collector - Records and stores agent interaction experiences
//!
//! Captures task execution data including context, skill used, outcome,
//! duration, and any errors encountered.

use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::sync::Arc;
use tokio::sync::RwLock;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Experience {
    pub id: String,
    pub timestamp: u64,
    pub context: String,
    pub intent: String,
    pub skill_used: Option<String>,
    pub tools_used: Vec<String>,
    pub outcome: ExperienceOutcome,
    pub duration_ms: u64,
    pub error: Option<String>,
    pub user_feedback: Option<f32>,
    pub session_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ExperienceOutcome {
    Success,
    PartialSuccess,
    Failure,
    Timeout,
    Cancelled,
}

impl Experience {
    pub fn new(
        context: String,
        intent: String,
        skill_used: Option<String>,
        tools_used: Vec<String>,
        outcome: ExperienceOutcome,
        duration_ms: u64,
        session_id: String,
    ) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            timestamp: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            context,
            intent,
            skill_used,
            tools_used,
            outcome,
            duration_ms,
            error: None,
            user_feedback: None,
            session_id,
        }
    }

    pub fn with_error(mut self, error: String) -> Self {
        self.error = Some(error);
        self
    }

    pub fn with_feedback(mut self, feedback: f32) -> Self {
        self.user_feedback = Some(feedback);
        self
    }

    pub fn is_successful(&self) -> bool {
        matches!(self.outcome, ExperienceOutcome::Success | ExperienceOutcome::PartialSuccess)
    }

    pub fn normalized_score(&self) -> f32 {
        let base_score = match self.outcome {
            ExperienceOutcome::Success => 1.0,
            ExperienceOutcome::PartialSuccess => 0.6,
            ExperienceOutcome::Failure => 0.0,
            ExperienceOutcome::Timeout => 0.3,
            ExperienceOutcome::Cancelled => 0.0,
        };

        let feedback_factor = self.user_feedback.unwrap_or(base_score);
        (base_score + feedback_factor) / 2.0
    }
}

pub struct ExperienceCollector {
    experiences: Arc<RwLock<VecDeque<Experience>>>,
    max_size: usize,
    session_experiences: Arc<RwLock<Vec<String>>>,
}

impl ExperienceCollector {
    pub fn new(max_size: usize) -> Self {
        Self {
            experiences: Arc::new(RwLock::new(VecDeque::new())),
            max_size,
            session_experiences: Arc::new(RwLock::new(Vec::new())),
        }
    }

    pub async fn record(&self, experience: Experience) {
        let exp_id = experience.id.clone();

        {
            let mut experiences = self.experiences.write().await;
            if experiences.len() >= self.max_size {
                if let Some(oldest) = experiences.pop_front() {
                    drop(oldest);
                }
            }
            experiences.push_back(experience);
        }

        {
            let mut session_ids = self.session_experiences.write().await;
            session_ids.push(exp_id);
        }
    }

    pub async fn get_recent(&self, count: usize) -> Vec<Experience> {
        let experiences = self.experiences.read().await;
        experiences.iter().rev().take(count).cloned().collect()
    }

    pub async fn get_by_skill(&self, skill_name: &str) -> Vec<Experience> {
        let experiences = self.experiences.read().await;
        experiences
            .iter()
            .filter(|e| e.skill_used.as_deref() == Some(skill_name))
            .cloned()
            .collect()
    }

    pub async fn get_by_intent(&self, intent: &str) -> Vec<Experience> {
        let experiences = self.experiences.read().await;
        let intent_lower = intent.to_lowercase();
        experiences
            .iter()
            .filter(|e| e.intent.to_lowercase().contains(&intent_lower))
            .cloned()
            .collect()
    }

    pub async fn get_successful_patterns(&self) -> Vec<Experience> {
        let experiences = self.experiences.read().await;
        experiences
            .iter()
            .filter(|e| e.is_successful())
            .cloned()
            .collect()
    }

    pub async fn get_session_experiences(&self, session_id: &str) -> Vec<Experience> {
        let experiences = self.experiences.read().await;
        experiences
            .iter()
            .filter(|e| e.session_id == session_id)
            .cloned()
            .collect()
    }

    pub async fn get_skill_statistics(&self, skill_name: &str) -> SkillStatistics {
        let experiences = self.get_by_skill(skill_name).await;
        let total = experiences.len();
        if total == 0 {
            return SkillStatistics::default();
        }

        let successful = experiences.iter().filter(|e| e.is_successful()).count();
        let total_duration: u64 = experiences.iter().map(|e| e.duration_ms).sum();
        let avg_duration = total_duration / total as u64;

        let avg_score = experiences.iter().map(|e| e.normalized_score()).sum::<f32>() / total as f32;

        SkillStatistics {
            total_executions: total,
            successful_executions: successful,
            success_rate: successful as f32 / total as f32,
            average_duration_ms: avg_duration,
            average_score: avg_score,
        }
    }

    pub async fn count(&self) -> usize {
        let experiences = self.experiences.read().await;
        experiences.len()
    }

    pub async fn clear_session(&self, session_id: &str) {
        let mut experiences = self.experiences.write().await;
        experiences.retain(|e| e.session_id != session_id);
    }
}

impl Default for ExperienceCollector {
    fn default() -> Self {
        Self::new(super::MAX_EXPERIENCE_BUFFER)
    }
}

impl Clone for ExperienceCollector {
    fn clone(&self) -> Self {
        Self {
            experiences: self.experiences.clone(),
            max_size: self.max_size,
            session_experiences: self.session_experiences.clone(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SkillStatistics {
    pub total_executions: usize,
    pub successful_executions: usize,
    pub success_rate: f32,
    pub average_duration_ms: u64,
    pub average_score: f32,
}
