use thiserror::Error;

#[derive(Debug, Error)]
pub enum TimelineServiceError {
    #[error("database error: {0}")]
    Db(#[from] persistence_sqlite::DbError),
    #[error("not found: {0}")]
    NotFound(String),
    #[error("invalid input: {0}")]
    InvalidInput(String),
    #[error("unsupported: {0}")]
    Unsupported(String),
    #[error("{0}")]
    Other(String),
}

impl transport::ServiceErrorCategory for TimelineServiceError {
    fn category(&self) -> transport::ErrorCategory {
        match self {
            Self::Db(_) => transport::ErrorCategory::Io,
            Self::NotFound(_) | Self::InvalidInput(_) => transport::ErrorCategory::Validation,
            Self::Unsupported(_) => transport::ErrorCategory::Unsupported,
            Self::Other(_) => transport::ErrorCategory::Internal,
        }
    }
}

impl From<rusqlite::Error> for TimelineServiceError {
    fn from(error: rusqlite::Error) -> Self {
        Self::Db(persistence_sqlite::DbError::from(error))
    }
}

impl From<crate::source_db::ReadySourceError> for TimelineServiceError {
    fn from(error: crate::source_db::ReadySourceError) -> Self {
        match error {
            crate::source_db::ReadySourceError::Db(error) => Self::Db(error),
            crate::source_db::ReadySourceError::NotFound { .. } => {
                Self::NotFound(error.to_string())
            }
            crate::source_db::ReadySourceError::NotReady { .. } => {
                Self::InvalidInput(error.to_string())
            }
            crate::source_db::ReadySourceError::UnsupportedPlatform { .. } => {
                Self::Unsupported(error.to_string())
            }
        }
    }
}
