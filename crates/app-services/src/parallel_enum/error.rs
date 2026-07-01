use thiserror::Error;

/// Typed error for parallel filesystem enumeration.
#[derive(Debug, Error)]
pub enum ParallelEnumError {
    #[error("Cancelled")]
    Cancelled,
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("MFT parameter error: {0}")]
    MftParams(String),
    #[error("Database error: {0}")]
    Db(#[from] rusqlite::Error),
}

impl From<String> for ParallelEnumError {
    fn from(msg: String) -> Self {
        Self::MftParams(msg)
    }
}

impl From<&str> for ParallelEnumError {
    fn from(msg: &str) -> Self {
        Self::MftParams(msg.to_owned())
    }
}

impl From<persistence_sqlite::DbError> for ParallelEnumError {
    fn from(e: persistence_sqlite::DbError) -> Self {
        Self::MftParams(e.to_string())
    }
}

impl transport::ServiceErrorCategory for ParallelEnumError {
    fn category(&self) -> transport::ErrorCategory {
        match self {
            Self::Cancelled => transport::ErrorCategory::Internal,
            Self::Io(_) | Self::Db(_) => transport::ErrorCategory::Io,
            Self::MftParams(_) => transport::ErrorCategory::Validation,
        }
    }
}
