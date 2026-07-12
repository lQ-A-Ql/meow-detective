//! Unified error type for the Forensics application.
//!
//! Provides a single error enum that can represent all error conditions
//! across the application, with proper error conversion traits.

use thiserror::Error;

/// Unified error type for the Forensics application.
///
/// This enum consolidates all error types into a single hierarchy,
/// making error handling consistent across crates.
#[derive(Debug, Error)]
pub enum ForensicsError {
    /// IO errors (file operations, network, etc.)
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// Database errors (SQLite operations)
    #[error("Database error: {0}")]
    Database(String),

    /// Data parsing errors (file system structures, artifacts, etc.)
    #[error("Parse error: {0}")]
    Parse(String),

    /// Input validation errors
    #[error("Validation error: {0}")]
    Validation(String),

    /// Resource not found
    #[error("Not found: {0}")]
    NotFound(String),

    /// Security-related errors (path traversal, unauthorized access, etc.)
    #[error("Security error: {0}")]
    Security(String),

    /// Internal errors (logic errors, unexpected state, etc.)
    #[error("Internal error: {0}")]
    Internal(String),

    /// Operation cancelled by user
    #[error("Operation cancelled")]
    Cancelled,

    /// Operation not supported
    #[error("Not supported: {0}")]
    NotSupported(String),
}

// Implement From conversions for common error types

impl From<serde_json::Error> for ForensicsError {
    fn from(err: serde_json::Error) -> Self {
        ForensicsError::Parse(format!("JSON error: {}", err))
    }
}

impl From<String> for ForensicsError {
    fn from(err: String) -> Self {
        ForensicsError::Internal(err)
    }
}

impl From<Box<dyn std::error::Error>> for ForensicsError {
    fn from(err: Box<dyn std::error::Error>) -> Self {
        ForensicsError::Internal(err.to_string())
    }
}

impl From<&str> for ForensicsError {
    fn from(err: &str) -> Self {
        ForensicsError::Internal(err.to_string())
    }
}

/// Type alias for Result with ForensicsError
pub type ForensicsResult<T> = Result<T, ForensicsError>;

#[cfg(test)]
#[path = "../tests/unit/error.rs"]
mod tests;
