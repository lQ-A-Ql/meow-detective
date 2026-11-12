/// Typed error for notebook service operations.
#[derive(Debug, thiserror::Error)]
pub enum NotebookError {
    #[error("database error: {0}")]
    Db(#[from] persistence_sqlite::DbError),
    #[error("SQLite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("not found: {0}")]
    NotFound(String),
    #[error("{0}")]
    Other(String),
}

impl From<import_core::step_recorder::StepRecorderError> for NotebookError {
    fn from(error: import_core::step_recorder::StepRecorderError) -> Self {
        match error {
            import_core::step_recorder::StepRecorderError::Db(error) => Self::Db(error),
            import_core::step_recorder::StepRecorderError::Other(message) => Self::Other(message),
        }
    }
}

impl transport::ServiceErrorCategory for NotebookError {
    fn category(&self) -> transport::ErrorCategory {
        match self {
            Self::Db(_) | Self::Sqlite(_) => transport::ErrorCategory::Io,
            Self::NotFound(_) => transport::ErrorCategory::Validation,
            Self::Other(_) => transport::ErrorCategory::Internal,
        }
    }
}
