#[derive(Debug, thiserror::Error)]
pub enum SearchError {
    #[error("database error: {0}")]
    Db(#[from] persistence_sqlite::DbError),
    #[error("search index error: {0}")]
    Index(String),
    #[error("not found: {0}")]
    NotFound(String),
    #[error("invalid input: {0}")]
    InvalidInput(String),
    #[error("unsupported: {0}")]
    Unsupported(String),
    #[error("{0}")]
    Other(String),
}

impl transport::ServiceErrorCategory for SearchError {
    fn category(&self) -> transport::ErrorCategory {
        match self {
            Self::Db(_) => transport::ErrorCategory::Io,
            Self::Index(_) => transport::ErrorCategory::Parser,
            Self::NotFound(_) | Self::InvalidInput(_) => transport::ErrorCategory::Validation,
            Self::Unsupported(_) => transport::ErrorCategory::Unsupported,
            Self::Other(_) => transport::ErrorCategory::Internal,
        }
    }
}

impl From<crate::source_db::ReadySourceError> for SearchError {
    fn from(error: crate::source_db::ReadySourceError) -> Self {
        match error {
            crate::source_db::ReadySourceError::Db(error) => Self::Db(error),
            crate::source_db::ReadySourceError::UnsupportedPlatform { .. } => {
                Self::Unsupported(error.to_string())
            }
            crate::source_db::ReadySourceError::NotFound { .. } => {
                Self::NotFound(error.to_string())
            }
            crate::source_db::ReadySourceError::NotReady { .. } => {
                Self::InvalidInput(error.to_string())
            }
        }
    }
}
