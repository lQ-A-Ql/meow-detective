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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suggestion: Option<String>,
}

impl ApiErrorDto {
    pub fn new(code: impl Into<String>, message: impl Into<String>, recoverable: bool) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            category: None,
            details: None,
            recoverable,
            suggestion: None,
        }
    }

    pub fn with_suggestion(mut self, suggestion: impl Into<String>) -> Self {
        self.suggestion = Some(suggestion.into());
        self
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

/// Implemented by service-layer error enums (typically `thiserror::Error`) so
/// `CommandError::from_typed_service_error` can classify them without
/// substring-matching the rendered message. Each variant should map to the
/// `ErrorCategory` that best describes it; catch-all/`Other(String)` variants
/// typically map to `ErrorCategory::Internal`.
pub trait ServiceErrorCategory {
    fn category(&self) -> ErrorCategory;

    fn code(&self) -> Option<&'static str> {
        None
    }

    fn user_message(&self) -> Option<&'static str> {
        None
    }

    fn recoverable(&self) -> Option<bool> {
        None
    }

    fn safe_details(&self) -> Option<Value> {
        None
    }

    fn suggestion(&self) -> Option<&'static str> {
        None
    }
}

impl ServiceErrorCategory for std::io::Error {
    fn category(&self) -> ErrorCategory {
        ErrorCategory::Io
    }
}

impl ServiceErrorCategory for serde_json::Error {
    fn category(&self) -> ErrorCategory {
        ErrorCategory::Parser
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandError {
    pub code: String,
    pub message: String,
    pub category: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<Box<Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recoverable: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suggestion: Option<String>,
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
            details: None,
            recoverable: Some(recoverable),
            suggestion: None,
        }
    }

    fn with_suggestion(
        code: impl Into<String>,
        message: impl Into<String>,
        category: ErrorCategory,
        recoverable: bool,
        suggestion: impl Into<String>,
    ) -> Self {
        let mut s = Self::new(code, message, category, recoverable);
        s.suggestion = Some(suggestion.into());
        s
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

    /// Convert a typed service-layer error into a `CommandError` using its
    /// `ServiceErrorCategory::category()` instead of substring-matching the
    /// rendered message. Prefer this over [`Self::from_service_error`] for any
    /// error type that implements `ServiceErrorCategory`.
    pub fn from_typed_service_error<E>(e: E) -> Self
    where
        E: std::fmt::Display + ServiceErrorCategory,
    {
        let msg = e.to_string();
        tracing::error!("Service error: {}", msg);
        let normalized = msg.to_ascii_lowercase();
        let category = e.category();
        let mut command_error =
            if let Some(suggestion) = Self::forensics_suggestion(&normalized, &msg) {
                suggestion
            } else {
                match category {
                    ErrorCategory::Timeout => Self::timeout("The operation timed out"),
                    ErrorCategory::Unsupported => {
                        Self::unsupported("The requested operation is not supported")
                    }
                    ErrorCategory::Security => {
                        Self::security("The operation was blocked by the current security policy")
                    }
                    ErrorCategory::Parser => Self::parser("The input could not be parsed reliably"),
                    ErrorCategory::External => {
                        Self::external("The external dependency returned an error")
                    }
                    ErrorCategory::Io => Self::io("A file system operation failed"),
                    ErrorCategory::Validation => Self::invalid_input(msg),
                    ErrorCategory::Internal => {
                        Self::internal("An operation failed. Check logs for details.")
                    }
                }
            };
        command_error.category = category.as_str().to_string();
        if let Some(code) = e.code() {
            command_error.code = code.to_string();
        }
        if let Some(message) = e.user_message() {
            command_error.message = message.to_string();
        }
        if let Some(recoverable) = e.recoverable() {
            command_error.recoverable = Some(recoverable);
        }
        command_error.details = e.safe_details().map(Box::new);
        if let Some(suggestion) = e.suggestion() {
            command_error.suggestion = Some(suggestion.to_string());
        }
        command_error
    }

    /// Forensics-specific actionable suggestions keyed on message content.
    /// These are cross-cutting hints tied to the evidence re-import workflow,
    /// not a general error-category dimension, so they stay message-based
    /// rather than becoming `ErrorCategory` variants.
    fn forensics_suggestion(normalized: &str, msg: &str) -> Option<Self> {
        if normalized.contains("re-import") || normalized.contains("path reconstruction") {
            return Some(Self::with_suggestion(
                "IMPORT_NEEDED",
                msg,
                ErrorCategory::Internal,
                true,
                "建议重新导入 E01 镜像以重建完整的文件路径和分区元数据",
            ));
        }
        if normalized.contains("from any partition") {
            return Some(Self::with_suggestion(
                "PARTITION_NOT_FOUND",
                msg,
                ErrorCategory::Internal,
                true,
                "文件在已存储的所有分区中均未找到。可能原因：路径格式不匹配，或分区元数据缺失。建议重新导入 E01 镜像。",
            ));
        }
        if normalized.contains("no partition metadata") {
            return Some(Self::with_suggestion(
                "NO_METADATA",
                msg,
                ErrorCategory::Internal,
                true,
                "该数据源缺少分区元数据。建议重新导入 E01 镜像以生成分区信息。",
            ));
        }
        None
    }

    /// Classify a service error by matching substrings in its rendered message.
    ///
    /// This is the fallback path for error types that have no static `category()`
    /// (raw `String`, `std::io::Error` routed through here instead of `From`, or a
    /// third-party error type we don't own). Prefer [`Self::from_typed_service_error`]
    /// for any type implementing [`ServiceErrorCategory`].
    pub fn from_service_error(e: impl std::fmt::Display) -> Self {
        let msg = e.to_string();
        tracing::error!("Service error: {}", msg);
        let normalized = msg.to_ascii_lowercase();

        if let Some(suggestion) = Self::forensics_suggestion(&normalized, &msg) {
            return suggestion;
        }

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

#[cfg(test)]
#[path = "../tests/unit/errors.rs"]
mod tests;
