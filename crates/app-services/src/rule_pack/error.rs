//! Typed errors for the rule-pack engine.

use thiserror::Error;

/// Errors returned by rule-pack execution functions.
#[derive(Debug, Error)]
pub enum RulePackError {
    /// SQLite / rusqlite database error.
    #[error("database error: {0}")]
    Db(#[from] rusqlite::Error),

    /// Catch-all for contextual or miscellaneous errors.
    #[error("{0}")]
    Other(String),
}

impl transport::ServiceErrorCategory for RulePackError {
    fn category(&self) -> transport::ErrorCategory {
        match self {
            Self::Db(_) => transport::ErrorCategory::Io,
            Self::Other(_) => transport::ErrorCategory::Internal,
        }
    }
}
