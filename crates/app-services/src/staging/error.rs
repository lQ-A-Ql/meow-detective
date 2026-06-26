use thiserror::Error;

/// Typed error for the staging subsystem (partition DB lifecycle, merge operations).
#[derive(Debug, Error)]
pub enum StagingError {
    #[error("Database error: {0}")]
    Db(#[from] rusqlite::Error),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("Merge conflict: {0}")]
    MergeConflict(String),
    #[error("Invalid staging state: {0}")]
    InvalidState(String),
    #[error("{0}")]
    Other(String),
}

impl From<String> for StagingError {
    fn from(msg: String) -> Self {
        Self::Other(msg)
    }
}

impl From<&str> for StagingError {
    fn from(msg: &str) -> Self {
        Self::Other(msg.to_owned())
    }
}

impl From<persistence_sqlite::DbError> for StagingError {
    fn from(e: persistence_sqlite::DbError) -> Self {
        Self::Other(e.to_string())
    }
}
