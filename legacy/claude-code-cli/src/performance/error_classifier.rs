//! Error Classifier - API error classification for smart failover and recovery
//!
//! Provides structured taxonomy of API errors and determines recovery actions:
//! - Retry with backoff
//! - Rotate credential
//! - Fallback to another provider
//! - Compress context
//! - Abort

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FailoverReason {
    Auth,
    AuthPermanent,
    Billing,
    RateLimit,
    Overloaded,
    ServerError,
    Timeout,
    ContextOverflow,
    PayloadTooLarge,
    ModelNotFound,
    FormatError,
    ThinkingSignature,
    LongContextTier,
    Unknown,
}

impl FailoverReason {
    pub fn is_retryable(&self) -> bool {
        matches!(
            self,
            FailoverReason::Auth
                | FailoverReason::RateLimit
                | FailoverReason::Overloaded
                | FailoverReason::ServerError
                | FailoverReason::Timeout
                | FailoverReason::Unknown
        )
    }

    pub fn should_compress(&self) -> bool {
        matches!(
            self,
            FailoverReason::ContextOverflow | FailoverReason::PayloadTooLarge
        )
    }

    pub fn should_rotate_credential(&self) -> bool {
        matches!(
            self,
            FailoverReason::Auth
                | FailoverReason::Billing
                | FailoverReason::RateLimit
        )
    }

    pub fn should_fallback(&self) -> bool {
        matches!(
            self,
            FailoverReason::ModelNotFound
                | FailoverReason::FormatError
                | FailoverReason::AuthPermanent
        )
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClassifiedError {
    pub reason: FailoverReason,
    pub status_code: Option<u16>,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub message: String,
    pub retryable: bool,
    pub should_compress: bool,
    pub should_rotate_credential: bool,
    pub should_fallback: bool,
}

impl ClassifiedError {
    pub fn new(reason: FailoverReason, message: &str) -> Self {
        Self {
            reason,
            status_code: None,
            provider: None,
            model: None,
            message: message.to_string(),
            retryable: reason.is_retryable(),
            should_compress: reason.should_compress(),
            should_rotate_credential: reason.should_rotate_credential(),
            should_fallback: reason.should_fallback(),
        }
    }

    pub fn with_status(mut self, status: u16) -> Self {
        self.status_code = Some(status);
        self
    }

    pub fn with_provider(mut self, provider: &str) -> Self {
        self.provider = Some(provider.to_string());
        self
    }

    pub fn with_model(mut self, model: &str) -> Self {
        self.model = Some(model.to_string());
        self
    }

    pub fn is_auth(&self) -> bool {
        matches!(self.reason, FailoverReason::Auth | FailoverReason::AuthPermanent)
    }
}

const BILLING_PATTERNS: &[&str] = &[
    "insufficient credits",
    "insufficient_quota",
    "credit balance",
    "credits have been exhausted",
    "top up your credits",
    "payment required",
    "billing hard limit",
    "exceeded your current quota",
    "account is deactivated",
    "plan does not include",
];

const RATE_LIMIT_PATTERNS: &[&str] = &[
    "rate limit",
    "rate_limit",
    "too many requests",
    "throttled",
    "requests per minute",
    "tokens per minute",
    "requests per day",
    "try again in",
    "please retry after",
    "resource_exhausted",
    "rate increased too quickly",
];

const CONTEXT_OVERFLOW_PATTERNS: &[&str] = &[
    "context length",
    "context size",
    "maximum context",
    "token limit",
    "too many tokens",
    "reduce the length",
    "exceeds the limit",
    "context window",
    "prompt is too long",
    "prompt exceeds max length",
    "max_tokens",
    "maximum number of tokens",
    "超过最大长度",
    "上下文长度",
];

const MODEL_NOT_FOUND_PATTERNS: &[&str] = &[
    "is not a valid model",
    "invalid model",
    "model not found",
    "model_not_found",
    "does not exist",
    "no such model",
    "unknown model",
    "unsupported model",
];

const AUTH_PATTERNS: &[&str] = &[
    "invalid api key",
    "invalid_api_key",
    "authentication",
    "unauthorized",
    "forbidden",
    "invalid token",
    "token expired",
    "token revoked",
    "access denied",
];

const TRANSPORT_ERROR_TYPES: &[&str] = &[
    "ReadTimeout",
    "ConnectTimeout",
    "PoolTimeout",
    "ConnectError",
    "RemoteProtocolError",
    "ConnectionError",
    "ConnectionResetError",
    "ConnectionAbortedError",
    "BrokenPipeError",
];

pub struct ErrorClassifier {
    context_window_size: usize,
}

impl ErrorClassifier {
    pub fn new(context_window_size: usize) -> Self {
        Self { context_window_size }
    }

    pub fn classify(&self, error: &str, status_code: Option<u16>) -> ClassifiedError {
        let message_lower = error.to_lowercase();

        if let Some(status) = status_code {
            return self.classify_by_status(error, status);
        }

        if AUTH_PATTERNS.iter().any(|p| message_lower.contains(p)) {
            return ClassifiedError::new(FailoverReason::Auth, error);
        }

        if BILLING_PATTERNS.iter().any(|p| message_lower.contains(p)) {
            return ClassifiedError::new(FailoverReason::Billing, error);
        }

        if RATE_LIMIT_PATTERNS.iter().any(|p| message_lower.contains(p)) {
            return ClassifiedError::new(FailoverReason::RateLimit, error);
        }

        if CONTEXT_OVERFLOW_PATTERNS.iter().any(|p| message_lower.contains(p)) {
            return ClassifiedError::new(FailoverReason::ContextOverflow, error);
        }

        if MODEL_NOT_FOUND_PATTERNS.iter().any(|p| message_lower.contains(p)) {
            return ClassifiedError::new(FailoverReason::ModelNotFound, error);
        }

        if TRANSPORT_ERROR_TYPES.iter().any(|t| message_lower.contains(&t.to_lowercase())) {
            return ClassifiedError::new(FailoverReason::Timeout, error);
        }

        ClassifiedError::new(FailoverReason::Unknown, error)
    }

    fn classify_by_status(&self, error: &str, status: u16) -> ClassifiedError {
        match status {
            400 => ClassifiedError::new(FailoverReason::FormatError, error)
                .with_status(status),
            401 | 403 => {
                if error.to_lowercase().contains("invalid") || error.to_lowercase().contains("expired") {
                    ClassifiedError::new(FailoverReason::Auth, error)
                        .with_status(status)
                } else {
                    ClassifiedError::new(FailoverReason::AuthPermanent, error)
                        .with_status(status)
                }
            }
            402 => ClassifiedError::new(FailoverReason::Billing, error)
                .with_status(status),
            404 => ClassifiedError::new(FailoverReason::ModelNotFound, error)
                .with_status(status),
            413 => ClassifiedError::new(FailoverReason::PayloadTooLarge, error)
                .with_status(status),
            429 => ClassifiedError::new(FailoverReason::RateLimit, error)
                .with_status(status),
            500..=599 => {
                if status == 503 || status == 529 {
                    ClassifiedError::new(FailoverReason::Overloaded, error)
                        .with_status(status)
                } else {
                    ClassifiedError::new(FailoverReason::ServerError, error)
                        .with_status(status)
                }
            }
            _ => ClassifiedError::new(FailoverReason::Unknown, error)
                .with_status(status),
        }
    }

    pub fn should_retry(&self, error: &ClassifiedError) -> bool {
        error.retryable
    }

    pub fn should_compress_context(&self, error: &ClassifiedError) -> bool {
        error.should_compress
    }

    pub fn should_rotate(&self, error: &ClassifiedError) -> bool {
        error.should_rotate_credential
    }
}

impl Default for ErrorClassifier {
    fn default() -> Self {
        Self::new(200_000)
    }
}
