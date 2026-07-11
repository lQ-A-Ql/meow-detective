/// Typed error for report generation operations.
#[derive(Debug, thiserror::Error)]
pub enum ReportError {
    #[error("database error: {0}")]
    Db(#[from] persistence_sqlite::DbError),
    #[error("SQLite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("not found: {0}")]
    NotFound(String),
    #[error("unsupported: {0}")]
    Unsupported(String),
    #[error("{0}")]
    Other(String),
}

impl transport::ServiceErrorCategory for ReportError {
    fn category(&self) -> transport::ErrorCategory {
        match self {
            Self::Db(_) | Self::Sqlite(_) | Self::Io(_) => transport::ErrorCategory::Io,
            Self::NotFound(_) => transport::ErrorCategory::Validation,
            Self::Unsupported(_) => transport::ErrorCategory::Unsupported,
            Self::Other(_) => transport::ErrorCategory::Internal,
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

impl From<crate::correlation::CorrelationError> for ReportError {
    fn from(error: crate::correlation::CorrelationError) -> Self {
        match error {
            crate::correlation::CorrelationError::Db(error) => Self::Db(error),
            crate::correlation::CorrelationError::Json(error) => Self::Other(error.to_string()),
            crate::correlation::CorrelationError::InvalidInput(message) => Self::NotFound(message),
            crate::correlation::CorrelationError::Unsupported(message) => {
                Self::Unsupported(message)
            }
            crate::correlation::CorrelationError::Other(message) => Self::Other(message),
        }
    }
}

impl From<crate::governance::GovernanceError> for ReportError {
    fn from(error: crate::governance::GovernanceError) -> Self {
        match error {
            crate::governance::GovernanceError::Database(error) => Self::Sqlite(error),
            crate::governance::GovernanceError::Parse(error) => Self::Other(error.to_string()),
            crate::governance::GovernanceError::Validation(message) => Self::NotFound(message),
            crate::governance::GovernanceError::Unsupported(message) => Self::Unsupported(message),
            crate::governance::GovernanceError::Internal(message) => Self::Other(message),
        }
    }
}

impl From<crate::artifact_service::ArtifactServiceError> for ReportError {
    fn from(error: crate::artifact_service::ArtifactServiceError) -> Self {
        match error {
            crate::artifact_service::ArtifactServiceError::Db(error) => Self::Db(error),
            crate::artifact_service::ArtifactServiceError::Io(error) => Self::Io(error),
            crate::artifact_service::ArtifactServiceError::NotFound(message)
            | crate::artifact_service::ArtifactServiceError::InvalidInput(message) => {
                Self::NotFound(message)
            }
            crate::artifact_service::ArtifactServiceError::Unsupported(message) => {
                Self::Unsupported(message)
            }
            other => Self::Other(other.to_string()),
        }
    }
}

impl From<crate::timeline_service::TimelineServiceError> for ReportError {
    fn from(error: crate::timeline_service::TimelineServiceError) -> Self {
        match error {
            crate::timeline_service::TimelineServiceError::Db(error) => Self::Db(error),
            crate::timeline_service::TimelineServiceError::NotFound(message)
            | crate::timeline_service::TimelineServiceError::InvalidInput(message) => {
                Self::NotFound(message)
            }
            crate::timeline_service::TimelineServiceError::Unsupported(message) => {
                Self::Unsupported(message)
            }
            crate::timeline_service::TimelineServiceError::Other(message) => Self::Other(message),
        }
    }
}
