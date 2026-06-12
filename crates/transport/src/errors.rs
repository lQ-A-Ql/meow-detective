use serde::Serialize;
use serde_json::Value;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApiErrorDto {
    pub code: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<Value>,
    pub recoverable: bool,
}

impl ApiErrorDto {
    pub fn new(code: impl Into<String>, message: impl Into<String>, recoverable: bool) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            category: None,
            details: None,
            recoverable,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ErrorCategory {
    Validation,
    Unsupported,
    Io,
    Parser,
    Security,
    External,
    Timeout,
    Internal,
}

impl ErrorCategory {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Validation => "validation",
            Self::Unsupported => "unsupported",
            Self::Io => "io",
            Self::Parser => "parser",
            Self::Security => "security",
            Self::External => "external",
            Self::Timeout => "timeout",
            Self::Internal => "internal",
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandError {
    pub code: String,
    pub message: String,
    pub category: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recoverable: Option<bool>,
}

impl std::fmt::Display for CommandError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{}:{}] {}", self.category, self.code, self.message)
    }
}

impl CommandError {
    fn new(
        code: impl Into<String>,
        message: impl Into<String>,
        category: ErrorCategory,
        recoverable: bool,
    ) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            category: category.as_str().to_string(),
            recoverable: Some(recoverable),
        }
    }

    pub fn not_found(entity: &str) -> Self {
        Self::new(
            "NOT_FOUND",
            format!("{} not found", entity),
            ErrorCategory::Validation,
            true,
        )
    }

    pub fn no_active_case() -> Self {
        Self::new(
            "NO_ACTIVE_CASE",
            "No active case. Open or create a case first.",
            ErrorCategory::Validation,
            true,
        )
    }

    pub fn internal(message: impl Into<String>) -> Self {
        Self::new("INTERNAL", message, ErrorCategory::Internal, false)
    }

    pub fn invalid_input(message: impl Into<String>) -> Self {
        Self::new("INVALID_INPUT", message, ErrorCategory::Validation, true)
    }

    pub fn conflict(message: impl Into<String>) -> Self {
        Self::new("CONFLICT", message, ErrorCategory::Validation, true)
    }

    pub fn unsupported(message: impl Into<String>) -> Self {
        Self::new("UNSUPPORTED", message, ErrorCategory::Unsupported, true)
    }

    pub fn io(message: impl Into<String>) -> Self {
        Self::new("IO_ERROR", message, ErrorCategory::Io, false)
    }

    pub fn parser(message: impl Into<String>) -> Self {
        Self::new("PARSER_ERROR", message, ErrorCategory::Parser, true)
    }

    pub fn security(message: impl Into<String>) -> Self {
        Self::new("SECURITY_ERROR", message, ErrorCategory::Security, false)
    }

    pub fn external(message: impl Into<String>) -> Self {
        Self::new("EXTERNAL_ERROR", message, ErrorCategory::External, true)
    }

    pub fn timeout(message: impl Into<String>) -> Self {
        Self::new("TIMEOUT", message, ErrorCategory::Timeout, true)
    }

    pub fn from_service_error(e: impl std::fmt::Display) -> Self {
        let msg = e.to_string();
        tracing::error!("Service error: {}", msg);
        let normalized = msg.to_ascii_lowercase();

        if normalized.contains("timeout") {
            return Self::timeout("The operation timed out");
        }
        if normalized.contains("not supported") || normalized.contains("unsupported") {
            return Self::unsupported("The requested operation is not supported");
        }
        if normalized.contains("permission")
            || normalized.contains("forbidden")
            || normalized.contains("not allowed")
            || normalized.contains("disabled for this server")
        {
            return Self::security("The operation was blocked by the current security policy");
        }
        if normalized.contains("parse")
            || normalized.contains("invalid hive")
            || normalized.contains("truncated")
            || normalized.contains("corrupt")
        {
            return Self::parser("The input could not be parsed reliably");
        }
        if normalized.contains("network")
            || normalized.contains("connection")
            || normalized.contains("http")
            || normalized.contains("stdio")
            || normalized.contains("mcp")
        {
            return Self::external("The external dependency returned an error");
        }
        if normalized.contains("file system")
            || normalized.contains("i/o")
            || normalized.contains("os error")
            || normalized.contains("path")
        {
            return Self::io("A file system operation failed");
        }

        Self::internal("An operation failed. Check logs for details.")
    }

    pub fn from_lock_error(label: &str, e: impl std::fmt::Debug) -> Self {
        tracing::error!("{} lock poisoned: {:?}", label, e);
        Self::internal(format!("{} is temporarily unavailable", label))
    }

    pub fn from_join_error(e: impl std::fmt::Debug) -> Self {
        tracing::error!("Task join error: {:?}", e);
        Self::internal("An internal task failed")
    }
}

impl From<std::io::Error> for CommandError {
    fn from(e: std::io::Error) -> Self {
        tracing::error!("IO error: {:?}", e);
        CommandError::io("A file system operation failed")
    }
}
