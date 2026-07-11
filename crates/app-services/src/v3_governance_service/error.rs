#[derive(Debug, thiserror::Error)]
pub enum V3GovernanceError {
    #[error("database error: {0}")]
    Db(#[from] rusqlite::Error),
    #[error("I/O error: {0}")]
    Io(String),
    #[error("parser error: {0}")]
    Parser(String),
    #[error("invalid input: {0}")]
    Validation(String),
    #[error("unsupported: {0}")]
    Unsupported(String),
    #[error("{0}")]
    Other(String),
}

impl transport::ServiceErrorCategory for V3GovernanceError {
    fn category(&self) -> transport::ErrorCategory {
        match self {
            Self::Db(_) | Self::Io(_) => transport::ErrorCategory::Io,
            Self::Parser(_) => transport::ErrorCategory::Parser,
            Self::Validation(_) => transport::ErrorCategory::Validation,
            Self::Unsupported(_) => transport::ErrorCategory::Unsupported,
            Self::Other(_) => transport::ErrorCategory::Internal,
        }
    }
}

impl From<crate::governance::GovernanceError> for V3GovernanceError {
    fn from(error: crate::governance::GovernanceError) -> Self {
        match error {
            crate::governance::GovernanceError::Database(error) => Self::Db(error),
            crate::governance::GovernanceError::Parse(error) => Self::Parser(error.to_string()),
            crate::governance::GovernanceError::Validation(message) => Self::Validation(message),
            crate::governance::GovernanceError::Unsupported(message) => Self::Unsupported(message),
            crate::governance::GovernanceError::Internal(message) => Self::Other(message),
        }
    }
}

impl From<crate::graph_service::GraphServiceError> for V3GovernanceError {
    fn from(error: crate::graph_service::GraphServiceError) -> Self {
        match error {
            crate::graph_service::GraphServiceError::Db(error) => Self::Io(error.to_string()),
            crate::graph_service::GraphServiceError::Json(error) => Self::Parser(error.to_string()),
            crate::graph_service::GraphServiceError::NotFound(message)
            | crate::graph_service::GraphServiceError::InvalidInput(message) => {
                Self::Validation(message)
            }
            crate::graph_service::GraphServiceError::Unsupported(message) => {
                Self::Unsupported(message)
            }
            crate::graph_service::GraphServiceError::Other(message) => Self::Other(message),
        }
    }
}

impl From<crate::artifact_service::ArtifactServiceError> for V3GovernanceError {
    fn from(error: crate::artifact_service::ArtifactServiceError) -> Self {
        match error {
            crate::artifact_service::ArtifactServiceError::Db(error) => Self::Io(error.to_string()),
            crate::artifact_service::ArtifactServiceError::Io(error) => Self::Io(error.to_string()),
            crate::artifact_service::ArtifactServiceError::Extractor(message) => {
                Self::Parser(message)
            }
            crate::artifact_service::ArtifactServiceError::NotFound(message)
            | crate::artifact_service::ArtifactServiceError::InvalidInput(message) => {
                Self::Validation(message)
            }
            crate::artifact_service::ArtifactServiceError::Unsupported(message) => {
                Self::Unsupported(message)
            }
            crate::artifact_service::ArtifactServiceError::Other(message) => Self::Other(message),
        }
    }
}
