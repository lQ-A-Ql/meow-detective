use thiserror::Error;

/// Typed error for the import analysis pipeline.
#[derive(Debug, Error)]
pub enum ImportAnalysisError {
    /// The post-import pipeline only supports explicitly classified sources.
    #[error("unsupported data source platform `{0}` for post-import analysis")]
    UnsupportedPlatform(String),

    /// Direct SQLite error from query/prepare/execute operations.
    #[error("database error: {0}")]
    Db(#[from] rusqlite::Error),

    /// Persistence-layer error from staging or main DB open operations.
    #[error("persistence error: {0}")]
    Persistence(#[from] persistence_sqlite::DbError),

    /// Filesystem or thread-spawn I/O error.
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    /// Staging subsystem context error (merge failures, contextual wraps).
    #[error("staging error: {0}")]
    Staging(String),

    /// Catch-all for channel send failures, thread join panics,
    /// cancellation, and other contextual errors.
    #[error("{0}")]
    Other(String),
}

impl transport::ServiceErrorCategory for ImportAnalysisError {
    fn category(&self) -> transport::ErrorCategory {
        match self {
            Self::UnsupportedPlatform(_) => transport::ErrorCategory::Unsupported,
            Self::Db(_) | Self::Persistence(_) | Self::Io(_) => transport::ErrorCategory::Io,
            Self::Staging(_) => transport::ErrorCategory::Validation,
            Self::Other(_) => transport::ErrorCategory::Internal,
        }
    }
}
