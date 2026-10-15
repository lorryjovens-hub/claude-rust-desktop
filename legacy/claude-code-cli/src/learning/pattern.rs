//! Pattern Extractor - Identifies recurring patterns from successful experiences
//!
//! Analyzes completed tasks to find common patterns in:
//! - Context structures
//! - Tool sequences
//! - Solution approaches
//! - Error recovery strategies

use crate::learning::experience::{Experience, ExperienceCollector};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tokio::sync::RwLock;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Pattern {
    pub id: String,
    pub pattern_type: PatternType,
    pub description: String,
    pub key_elements: Vec<String>,
    pub occurrences: usize,
    pub confidence: PatternConfidence,
    pub success_rate: f32,
    pub average_duration_ms: u64,
    pub first_seen: u64,
    pub last_seen: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum PatternType {
    ToolSequence,
    ContextStructure,
    SolutionApproach,
    ErrorRecovery,
    CommandPattern,
    FileOperation,
    SearchPattern,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatternConfidence {
    pub score: f32,
    pub factors: Vec<String>,
}

impl PatternConfidence {
    pub fn new(score: f32) -> Self {
        Self {
            score: score.clamp(0.0, 1.0),
            factors: Vec::new(),
        }
    }

    pub fn with_factors(mut self, factors: Vec<&str>) -> Self {
        self.factors = factors.iter().map(|s| s.to_string()).collect();
        self
    }
}

pub struct PatternExtractor {
    patterns: Arc<RwLock<HashMap<String, Pattern>>>,
    min_occurrences: usize,
}

impl PatternExtractor {
    pub fn new(min_occurrences: usize) -> Self {
        Self {
            patterns: Arc::new(RwLock::new(HashMap::new())),
            min_occurrences,
        }
    }

    pub async fn extract_from_experiences(&self, experiences: &[Experience]) -> Vec<Pattern> {
        let mut discovered_patterns = Vec::new();

        let tool_patterns = self.extract_tool_sequence_patterns(experiences);
        discovered_patterns.extend(tool_patterns);

        let command_patterns = self.extract_command_patterns(experiences);
        discovered_patterns.extend(command_patterns);

        let file_patterns = self.extract_file_operation_patterns(experiences);
        discovered_patterns.extend(file_patterns);

        let mut patterns = self.patterns.write().await;
        for pattern in &discovered_patterns {
            if let Some(existing) = patterns.get_mut(&pattern.id) {
                existing.occurrences += pattern.occurrences;
                existing.last_seen = pattern.last_seen;
                existing.confidence.score = (existing.confidence.score + pattern.confidence.score) / 2.0;
            } else {
                patterns.insert(pattern.id.clone(), pattern.clone());
            }
        }

        patterns.values().filter(|p| p.occurrences >= self.min_occurrences).cloned().collect()
    }

    fn extract_tool_sequence_patterns(&self, experiences: &[Experience]) -> Vec<Pattern> {
        let mut sequence_counts: HashMap<Vec<String>, usize> = HashMap::new();
        let mut sequence_durations: HashMap<Vec<String>, Vec<u64>> = HashMap::new();
        let mut sequence_successes: HashMap<Vec<String>, usize> = HashMap::new();

        for exp in experiences.iter().filter(|e| e.is_successful() && !e.tools_used.is_empty()) {
            let sequence: Vec<String> = exp.tools_used.clone();
            *sequence_counts.entry(sequence.clone()).or_insert(0) += 1;
            sequence_durations.entry(sequence.clone()).or_insert_with(Vec::new).push(exp.duration_ms);
            if exp.is_successful() {
                *sequence_successes.entry(sequence).or_insert(0) += 1;
            }
        }

        sequence_counts
            .into_iter()
            .filter(|(_, count)| *count >= self.min_occurrences)
            .map(|(sequence, count)| {
                let durations = sequence_durations.remove(&sequence).unwrap_or_default();
                let avg_duration = if durations.is_empty() {
                    0
                } else {
                    durations.iter().sum::<u64>() / durations.len() as u64
                };
                let successes = sequence_successes.remove(&sequence).unwrap_or(0);

                Pattern {
                    id: format!("tool_seq_{}", sequence.join("_")),
                    pattern_type: PatternType::ToolSequence,
                    description: format!("Tool sequence: {} (seen {} times)", sequence.join(" -> "), count),
                    key_elements: sequence,
                    occurrences: count,
                    confidence: PatternConfidence::new(count as f32 / 10.0).with_factors(vec!["frequency", "consistency"]),
                    success_rate: successes as f32 / count as f32,
                    average_duration_ms: avg_duration,
                    first_seen: 0,
                    last_seen: chrono::Utc::now().timestamp() as u64,
                }
            })
            .collect()
    }

    fn extract_command_patterns(&self, experiences: &[Experience]) -> Vec<Pattern> {
        let mut command_counts: HashMap<String, usize> = HashMap::new();
        let mut command_successes: HashMap<String, usize> = HashMap::new();

        let command_keywords = ["bash", "shell", "exec", "run", "execute", "cmd"];

        for exp in experiences.iter().filter(|e| e.is_successful()) {
            let context_lower = exp.context.to_lowercase();
            for keyword in &command_keywords {
                if context_lower.contains(keyword) {
                    let words: Vec<&str> = context_lower.split_whitespace().collect();
                    for (i, word) in words.iter().enumerate() {
                        if word == keyword && i + 1 < words.len() {
                            let cmd = words[i + 1].to_string();
                            *command_counts.entry(cmd.clone()).or_insert(0) += 1;
                            if exp.is_successful() {
                                *command_successes.entry(cmd).or_insert(0) += 1;
                            }
                            break;
                        }
                    }
                }
            }
        }

        command_counts
            .into_iter()
            .filter(|(_, count)| *count >= self.min_occurrences)
            .map(|(command, count)| {
                let successes = command_successes.remove(&command).unwrap_or(0);

                Pattern {
                    id: format!("cmd_{}", command.replace(' ', "_")),
                    pattern_type: PatternType::CommandPattern,
                    description: format!("Command pattern: {} (seen {} times)", command, count),
                    key_elements: vec![command],
                    occurrences: count,
                    confidence: PatternConfidence::new((count as f32 / 10.0).min(1.0)).with_factors(vec!["command_frequency"]),
                    success_rate: successes as f32 / count as f32,
                    average_duration_ms: 0,
                    first_seen: 0,
                    last_seen: chrono::Utc::now().timestamp() as u64,
                }
            })
            .collect()
    }

    fn extract_file_operation_patterns(&self, experiences: &[Experience]) -> Vec<Pattern> {
        let mut file_pattern_counts: HashMap<String, usize> = HashMap::new();
        let file_keywords = ["read", "write", "edit", "create", "delete", "modify"];

        for exp in experiences.iter().filter(|e| e.is_successful()) {
            let context_lower = exp.context.to_lowercase();
            for keyword in &file_keywords {
                if context_lower.contains(keyword) {
                    let words: Vec<&str> = context_lower.split_whitespace().collect();
                    for word in words {
                        if word.ends_with(".rs") || word.ends_with(".py") || word.ends_with(".js")
                            || word.ends_with(".ts") || word.ends_with(".go") || word.ends_with(".md") {
                            let ext = std::path::Path::new(word)
                                .extension()
                                .and_then(|e| e.to_str())
                                .unwrap_or("*");
                            *file_pattern_counts.entry(ext.to_string()).or_insert(0) += 1;
                        }
                    }
                }
            }
        }

        file_pattern_counts
            .into_iter()
            .filter(|(_, count)| *count >= self.min_occurrences)
            .map(|(ext, count)| {
                Pattern {
                    id: format!("file_{}", ext),
                    pattern_type: PatternType::FileOperation,
                    description: format!("File operation pattern for .{} files (seen {} times)", ext, count),
                    key_elements: vec![ext],
                    occurrences: count,
                    confidence: PatternConfidence::new((count as f32 / 10.0).min(1.0)).with_factors(vec!["file_extension_frequency"]),
                    success_rate: 1.0,
                    average_duration_ms: 0,
                    first_seen: 0,
                    last_seen: chrono::Utc::now().timestamp() as u64,
                }
            })
            .collect()
    }

    pub async fn get_patterns_by_type(&self, pattern_type: PatternType) -> Vec<Pattern> {
        let patterns = self.patterns.read().await;
        patterns
            .values()
            .filter(|p| p.pattern_type == pattern_type)
            .cloned()
            .collect()
    }

    pub async fn get_high_confidence_patterns(&self, threshold: f32) -> Vec<Pattern> {
        let patterns = self.patterns.read().await;
        patterns
            .values()
            .filter(|p| p.confidence.score >= threshold)
            .cloned()
            .collect()
    }

    pub async fn get_all_patterns(&self) -> Vec<Pattern> {
        let patterns = self.patterns.read().await;
        patterns.values().cloned().collect()
    }

    pub async fn merge_patterns(&self, patterns: Vec<Pattern>) {
        let mut stored = self.patterns.write().await;
        for pattern in patterns {
            stored.insert(pattern.id.clone(), pattern);
        }
    }
}

impl Default for PatternExtractor {
    fn default() -> Self {
        Self::new(3)
    }
}

impl Clone for PatternExtractor {
    fn clone(&self) -> Self {
        Self {
            patterns: self.patterns.clone(),
            min_occurrences: self.min_occurrences,
        }
    }
}
