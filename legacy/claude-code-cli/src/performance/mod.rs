//! Performance Module - Performance optimization components
//!
//! This module provides:
//! - Context compression for long conversations
//! - Prompt caching for cost optimization
//! - Smart model routing
//! - Rate limit tracking
//! - Error classification and recovery
//! - Tool result persistence

pub mod context_compressor;
pub mod prompt_cache;
pub mod smart_router;
pub mod rate_limiter;
pub mod error_classifier;
pub mod tool_persistence;
pub mod retry_policy;
pub mod budget;

pub use context_compressor::{ContextCompressor, CompressorConfig, CompressionResult};
pub use prompt_cache::{PromptCache, CacheConfig, apply_anthropic_cache_control};
pub use smart_router::{SmartRouter, RouterConfig, RouteDecision};
pub use rate_limiter::{RateLimiter, RateLimitState, RateLimitBucket};
pub use error_classifier::{ErrorClassifier, ClassifiedError, FailoverReason};
pub use tool_persistence::{ToolPersistence, PersistedOutput};
pub use retry_policy::{RetryPolicy, RetryConfig};
pub use budget::{BudgetManager, BudgetConfig};
