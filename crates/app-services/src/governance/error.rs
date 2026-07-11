#[derive(Debug, thiserror::Error)]
pub enum GovernanceError {
    #[error("database operation failed: {0}")]
    Database(#[from] rusqlite::Error),
    #[error("failed to parse governance fact source: {0}")]
    Parse(#[from] serde_json::Error),
    #[error("invalid input: {0}")]
    Validation(String),
    #[error("unsupported: {0}")]
    Unsupported(String),
    #[error("internal governance error: {0}")]
    Internal(String),
}

impl From<persistence_sqlite::DbError> for GovernanceError {
    fn from(err: persistence_sqlite::DbError) -> Self {
        match err {
            persistence_sqlite::DbError::Sqlite(e) => GovernanceError::Database(e),
            _ => GovernanceError::Internal(err.to_string()),
        }
    }
}

impl From<crate::correlation::CorrelationError> for GovernanceError {
    fn from(error: crate::correlation::CorrelationError) -> Self {
        match error {
            crate::correlation::CorrelationError::Db(error) => error.into(),
            crate::correlation::CorrelationError::Json(error) => Self::Parse(error),
            crate::correlation::CorrelationError::InvalidInput(message) => {
                Self::Validation(message)
            }
            crate::correlation::CorrelationError::Unsupported(message) => {
                Self::Unsupported(message)
            }
            crate::correlation::CorrelationError::Other(message) => Self::Internal(message),
        }
    }
}

impl transport::ServiceErrorCategory for GovernanceError {
    fn category(&self) -> transport::ErrorCategory {
        match self {
            Self::Database(_) => transport::ErrorCategory::Io,
            Self::Parse(_) => transport::ErrorCategory::Parser,
            Self::Validation(_) => transport::ErrorCategory::Validation,
            Self::Unsupported(_) => transport::ErrorCategory::Unsupported,
            Self::Internal(_) => transport::ErrorCategory::Internal,
        }
    }
}
