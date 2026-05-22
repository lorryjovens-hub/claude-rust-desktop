use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub total_tokens: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelCost {
    pub model: String,
    pub input_cost_per_1k: f64,
    pub output_cost_per_1k: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionCost {
    pub session_id: String,
    pub conversation_id: String,
    pub model: String,
    pub total_input_tokens: u64,
    pub total_output_tokens: u64,
    pub estimated_cost: f64,
    pub started_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CostSummary {
    pub total_input_tokens: u64,
    pub total_output_tokens: u64,
    pub total_tokens: u64,
    pub estimated_total_cost: f64,
    pub sessions: Vec<SessionCost>,
    pub model_breakdown: HashMap<String, TokenUsage>,
}

pub struct CostTracker {
    store_dir: PathBuf,
    sessions: Arc<Mutex<HashMap<String, SessionCost>>>,
    model_costs: HashMap<String, ModelCost>,
}

impl CostTracker {
    pub fn new(store_dir: PathBuf) -> Self {
        fs::create_dir_all(&store_dir).ok();
        let mut tracker = Self {
            store_dir,
            sessions: Arc::new(Mutex::new(HashMap::new())),
            model_costs: HashMap::new(),
        };
        tracker.init_model_costs();
        tracker
    }

    fn init_model_costs(&mut self) {
        let costs = vec![
            ModelCost {
                model: "claude-opus-4-5".to_string(),
                input_cost_per_1k: 0.015,
                output_cost_per_1k: 0.075,
            },
            ModelCost {
                model: "claude-sonnet-4-6".to_string(),
                input_cost_per_1k: 0.003,
                output_cost_per_1k: 0.015,
            },
            ModelCost {
                model: "claude-haiku-3-5".to_string(),
                input_cost_per_1k: 0.0008,
                output_cost_per_1k: 0.004,
            },
            ModelCost {
                model: "claude-sonnet-4-20250514".to_string(),
                input_cost_per_1k: 0.003,
                output_cost_per_1k: 0.015,
            },
            ModelCost {
                model: "claude-haiku-4-20250514".to_string(),
                input_cost_per_1k: 0.0008,
                output_cost_per_1k: 0.004,
            },
            ModelCost {
                model: "claude-opus-4-20250514".to_string(),
                input_cost_per_1k: 0.015,
                output_cost_per_1k: 0.075,
            },
            ModelCost {
                model: "gpt-4o".to_string(),
                input_cost_per_1k: 0.005,
                output_cost_per_1k: 0.015,
            },
            ModelCost {
                model: "gpt-4o-mini".to_string(),
                input_cost_per_1k: 0.00015,
                output_cost_per_1k: 0.0006,
            },
        ];

        for cost in costs {
            self.model_costs.insert(cost.model.clone(), cost);
        }
    }

    pub fn calculate_cost(&self, model: &str, input_tokens: u64, output_tokens: u64) -> f64 {
        let model_cost = self.model_costs.get(model);
        if let Some(mc) = model_cost {
            let input_cost = (input_tokens as f64 / 1000.0) * mc.input_cost_per_1k;
            let output_cost = (output_tokens as f64 / 1000.0) * mc.output_cost_per_1k;
            input_cost + output_cost
        } else {
            0.0
        }
    }

    pub async fn create_session(&self, conversation_id: &str, model: &str) -> String {
        let session_id = uuid::Uuid::new_v4().to_string()[..12].to_string();
        let now = chrono::Utc::now().to_rfc3339();

        let session = SessionCost {
            session_id: session_id.clone(),
            conversation_id: conversation_id.to_string(),
            model: model.to_string(),
            total_input_tokens: 0,
            total_output_tokens: 0,
            estimated_cost: 0.0,
            started_at: now.clone(),
            updated_at: now,
        };

        let mut sessions = self.sessions.lock().await;
        sessions.insert(session_id.clone(), session);
        session_id
    }

    pub async fn add_usage(&self, session_id: &str, input_tokens: u64, output_tokens: u64) -> Result<f64> {
        let mut sessions = self.sessions.lock().await;
        if let Some(session) = sessions.get_mut(session_id) {
            session.total_input_tokens += input_tokens;
            session.total_output_tokens += output_tokens;
            session.estimated_cost = self.calculate_cost(
                &session.model,
                session.total_input_tokens,
                session.total_output_tokens,
            );
            session.updated_at = chrono::Utc::now().to_rfc3339();

            self.save_session(session)?;
            Ok(session.estimated_cost)
        } else {
            anyhow::bail!("Session not found: {}", session_id)
        }
    }

    pub async fn get_session_cost(&self, session_id: &str) -> Option<SessionCost> {
        let sessions = self.sessions.lock().await;
        sessions.get(session_id).cloned()
    }

    pub async fn get_conversation_cost(&self, conversation_id: &str) -> CostSummary {
        let sessions = self.sessions.lock().await;
        let mut total_input = 0u64;
        let mut total_output = 0u64;
        let mut total_cost = 0.0;
        let mut model_breakdown: HashMap<String, TokenUsage> = HashMap::new();
        let mut session_list = Vec::new();

        for session in sessions.values() {
            if session.conversation_id == conversation_id {
                total_input += session.total_input_tokens;
                total_output += session.total_output_tokens;
                total_cost += session.estimated_cost;
                session_list.push(session.clone());

                let entry = model_breakdown.entry(session.model.clone()).or_insert(TokenUsage {
                    input_tokens: 0,
                    output_tokens: 0,
                    total_tokens: 0,
                });
                entry.input_tokens += session.total_input_tokens;
                entry.output_tokens += session.total_output_tokens;
                entry.total_tokens = entry.input_tokens + entry.output_tokens;
            }
        }

        CostSummary {
            total_input_tokens: total_input,
            total_output_tokens: total_output,
            total_tokens: total_input + total_output,
            estimated_total_cost: total_cost,
            sessions: session_list,
            model_breakdown,
        }
    }

    pub async fn get_all_sessions(&self) -> Vec<SessionCost> {
        let sessions = self.sessions.lock().await;
        sessions.values().cloned().collect()
    }

    fn save_session(&self, session: &SessionCost) -> Result<()> {
        let path = self.store_dir.join(format!("{}.json", session.session_id));
        let data = serde_json::to_string_pretty(session)?;
        fs::write(path, data)?;
        Ok(())
    }

    pub fn load_saved_sessions(&mut self) {
        if !self.store_dir.exists() {
            return;
        }

        if let Ok(entries) = fs::read_dir(&self.store_dir) {
            for entry in entries.flatten() {
                if entry.path().extension().and_then(|e| e.to_str()) == Some("json") {
                    if let Ok(data) = fs::read_to_string(entry.path()) {
                        if let Ok(session) = serde_json::from_str::<SessionCost>(&data) {
                            let mut sessions = self.sessions.blocking_lock();
                            sessions.insert(session.session_id.clone(), session);
                        }
                    }
                }
            }
        }
    }
}
