//! Learning Module - Self-growing skill evolution system
//!
//! Enables the agent to learn from interactions, extract patterns from successful
//! task completions, and automatically generate new skills over time.
//!
//! ## Core Components
//!
//! - **ExperienceCollector**: Records task interactions with outcomes
//! - **PatternExtractor**: Identifies recurring patterns in successful completions
//! - **SkillGenerator**: Creates new skills from extracted patterns
//! - **FeedbackLoop**: Adjusts strategy based on execution results
//! - **LearningEngine**: Orchestrates all learning components
//!
//! ## How It Works
//!
//! 1. Every skill execution is recorded with its context and outcome
//! 2. Patterns are extracted from successful executions
//! 3. When a pattern is observed N times, a new skill may be generated
//! 4. Generated skills are suggested to the user for activation
//! 5. Feedback from skill usage refines future generations
//!
//! ## Usage
//!
//! ```rust,ignore
//! let engine = LearningEngine::new();
//! engine.record_experience(context, skill_name, success, duration).await;
//! if let Some(skill) = engine.check_for_new_skills().await {
//!     engine.suggest_skill(skill).await;
//! }
//! ```

pub mod experience;
pub mod pattern;
pub mod generator;
pub mod feedback;
pub mod engine;

pub use experience::{ExperienceCollector, Experience, ExperienceOutcome};
pub use pattern::{PatternExtractor, Pattern, PatternConfidence};
pub use generator::{SkillGenerator, GeneratedSkill, SkillTemplate};
pub use feedback::{FeedbackLoop, Feedback, PerformanceMetric};
pub use engine::{LearningEngine, LearningConfig, LearningStats};

pub const MIN_PATTERN_OCCURRENCES: usize = 3;
pub const SKILL_GENERATION_THRESHOLD: f64 = 0.85;
pub const MAX_EXPERIENCE_BUFFER: usize = 1000;
