//! Typed error type for `artifacts-macos` parsers.

use thiserror::Error;

/// Errors that can occur when parsing macOS artifacts.
#[derive(Debug, Error)]
pub enum MacArtifactError {
    /// The input data is invalid, malformed, or otherwise cannot be parsed.
    #[error("invalid input: {0}")]
    InvalidInput(String),

    /// The format or version is not supported by this parser.
    #[error("unsupported format/version: {0}")]
    Unsupported(String),

    /// A decoding error occurred while interpreting bytes or strings.
    #[error("decode error: {0}")]
    Decode(String),

    /// A plist parsing error occurred.
    #[error("plist error: {0}")]
    Plist(String),

    /// A SQLite database operation failed.
    #[error("database error: {0}")]
    Database(#[from] rusqlite::Error),

    /// An I/O operation failed.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}

/// Convenient alias for results returned by macOS artifact parsers.
pub type Result<T> = std::result::Result<T, MacArtifactError>;
