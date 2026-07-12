use thiserror::Error;

/// Errors raised while indexing or projecting extracted entities.
#[derive(Debug, Error)]
pub enum EntityExtractionError {
    #[error("database error: {0}")]
    Db(#[from] persistence_sqlite::DbError),
    #[error("serialization error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("invalid input: {0}")]
    InvalidInput(String),
    #[error("{0}")]
    Other(String),
}

impl From<rusqlite::Error> for EntityExtractionError {
    fn from(error: rusqlite::Error) -> Self {
        Self::Db(persistence_sqlite::DbError::from(error))
    }
}

impl transport::ServiceErrorCategory for EntityExtractionError {
    fn category(&self) -> transport::ErrorCategory {
        match self {
            Self::Db(_) => transport::ErrorCategory::Io,
            Self::Json(_) => transport::ErrorCategory::Parser,
            Self::InvalidInput(_) => transport::ErrorCategory::Validation,
            Self::Other(_) => transport::ErrorCategory::Internal,
        }
    }
}
