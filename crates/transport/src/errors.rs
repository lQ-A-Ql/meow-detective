use serde::Serialize;
use serde_json::Value;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApiErrorDto {
    pub code: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<Value>,
    pub recoverable: bool,
}

impl ApiErrorDto {
    pub fn new(code: impl Into<String>, message: impl Into<String>, recoverable: bool) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            details: None,
            recoverable,
        }
    }
}

/// Structured error type for Tauri commands.
/// Replaces raw `String` errors with categorized variants that
/// avoid leaking internal details (paths, SQL, stack traces) to the frontend.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandError {
    pub code: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recoverable: Option<bool>,
}

impl std::fmt::Display for CommandError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{}] {}", self.code, self.message)
    }
}

impl CommandError {
    /// The requested entity was not found.
    pub fn not_found(entity: &str) -> Self {
        Self {
            code: "NOT_FOUND".into(),
            message: format!("{} not found", entity),
            recoverable: Some(true),
        }
    }

    /// No case is currently open.
    pub fn no_active_case() -> Self {
        Self {
            code: "NO_ACTIVE_CASE".into(),
            message: "No active case. Open or create a case first.".into(),
            recoverable: Some(true),
        }
    }

    /// An unexpected internal error occurred. The message should be safe
    /// to display to users (no SQL, paths, or stack traces).
    pub fn internal(message: impl Into<String>) -> Self {
        Self {
            code: "INTERNAL".into(),
            message: message.into(),
            recoverable: Some(false),
        }
    }

    /// The input provided by the user is invalid.
    pub fn invalid_input(message: impl Into<String>) -> Self {
        Self {
            code: "INVALID_INPUT".into(),
            message: message.into(),
            recoverable: Some(true),
        }
    }

    pub fn conflict(message: impl Into<String>) -> Self {
        Self {
            code: "CONFLICT".into(),
            message: message.into(),
            recoverable: Some(true),
        }
    }
}

impl From<std::io::Error> for CommandError {
    fn from(e: std::io::Error) -> Self {
        tracing::error!("IO error: {:?}", e);
        CommandError::internal("A file system operation failed")
    }
}

// NOTE: From<String> intentionally NOT implemented.
// Use CommandError::from_service_error() or specific variants to avoid
// leaking internal error details to the frontend.

/// Helper for Tauri command layers to convert service errors.
/// Usage in commands: `.map_err(|e| CommandError::from_service_error(e))`
impl CommandError {
    pub fn from_service_error(e: impl std::fmt::Display) -> Self {
        tracing::error!("Service error: {}", e);
        CommandError::internal("An operation failed. Check logs for details.")
    }

    /// Create from a lock poisoning error. Logs the real error, returns sanitized message.
    pub fn from_lock_error(label: &str, e: impl std::fmt::Debug) -> Self {
        tracing::error!("{} lock poisoned: {:?}", label, e);
        CommandError::internal(format!("{} is temporarily unavailable", label))
    }

    /// Create from a thread join error. Logs the real error, returns sanitized message.
    pub fn from_join_error(e: impl std::fmt::Debug) -> Self {
        tracing::error!("Task join error: {:?}", e);
        CommandError::internal("An internal task failed")
    }
}
