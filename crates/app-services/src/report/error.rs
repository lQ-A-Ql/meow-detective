#[derive(Debug, thiserror::Error)]
pub enum ReportError {
    #[error("database error: {0}")]
    Db(#[from] persistence_sqlite::DbError),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("not found: {0}")]
    NotFound(String),
    #[error("unsupported: {0}")]
    Unsupported(String),
    #[error("{0}")]
    Other(String),
    #[error("report export cancelled")]
    Cancelled,
    #[error(transparent)]
    File(#[from] evidence_access::file_service::FileServiceError),
}

impl transport::ServiceErrorCategory for ReportError {
    fn category(&self) -> transport::ErrorCategory {
        match self {
            Self::Db(error) => match error.category() {
                persistence_sqlite::connection::DbErrorCategory::Io => transport::ErrorCategory::Io,
                persistence_sqlite::connection::DbErrorCategory::Timeout => {
                    transport::ErrorCategory::Timeout
                }
                persistence_sqlite::connection::DbErrorCategory::Cancelled => {
                    transport::ErrorCategory::Cancelled
                }
                persistence_sqlite::connection::DbErrorCategory::Internal => {
                    transport::ErrorCategory::Internal
                }
            },
            Self::Io(_) => transport::ErrorCategory::Io,
            Self::NotFound(_) => transport::ErrorCategory::Validation,
            Self::Unsupported(_) => transport::ErrorCategory::Unsupported,
            Self::Other(_) => transport::ErrorCategory::Internal,
            Self::Cancelled => transport::ErrorCategory::Cancelled,
            Self::File(error) => transport::ServiceErrorCategory::category(error),
        }
    }
}

impl From<crate::source_db::ReadySourceError> for ReportError {
    fn from(error: crate::source_db::ReadySourceError) -> Self {
        match error {
            crate::source_db::ReadySourceError::Db(error) => Self::Db(error),
            crate::source_db::ReadySourceError::NotFound { .. }
            | crate::source_db::ReadySourceError::NotReady { .. } => {
                Self::NotFound(error.to_string())
            }
            crate::source_db::ReadySourceError::UnsupportedPlatform { .. } => {
                Self::Unsupported(error.to_string())
            }
        }
    }
}
