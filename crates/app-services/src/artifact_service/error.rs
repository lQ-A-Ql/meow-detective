use crate::{analysis_service::AnalysisServiceError, file_service::FileServiceError};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ArtifactServiceError {
    #[error("database error: {0}")]
    Db(#[from] persistence_sqlite::DbError),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("extractor error: {0}")]
    Extractor(String),
    #[error("not found: {0}")]
    NotFound(String),
    #[error("invalid input: {0}")]
    InvalidInput(String),
    #[error("unsupported operation: {0}")]
    Unsupported(String),
    #[error("other error: {0}")]
    Other(String),
}

impl transport::ServiceErrorCategory for ArtifactServiceError {
    fn category(&self) -> transport::ErrorCategory {
        match self {
            Self::Db(_) | Self::Io(_) => transport::ErrorCategory::Io,
            Self::Extractor(_) => transport::ErrorCategory::Parser,
            Self::NotFound(_) | Self::InvalidInput(_) => transport::ErrorCategory::Validation,
            Self::Unsupported(_) => transport::ErrorCategory::Unsupported,
            Self::Other(_) => transport::ErrorCategory::Internal,
        }
    }
}

impl ArtifactServiceError {
    pub fn extractor(message: impl Into<String>) -> Self {
        Self::Extractor(message.into())
    }

    pub fn not_found(message: impl Into<String>) -> Self {
        Self::NotFound(message.into())
    }

    pub fn invalid_input(message: impl Into<String>) -> Self {
        Self::InvalidInput(message.into())
    }

    pub fn other(message: impl Into<String>) -> Self {
        Self::Other(message.into())
    }
}

impl From<FileServiceError> for ArtifactServiceError {
    fn from(error: FileServiceError) -> Self {
        match error {
            FileServiceError::Db(error) => Self::Db(error),
            FileServiceError::Io(error) => Self::Io(error),
            FileServiceError::NotFound(message) => Self::NotFound(message),
            FileServiceError::InvalidInput(message) => Self::InvalidInput(message),
            FileServiceError::PathTraversal(message)
            | FileServiceError::Security(message)
            | FileServiceError::Other(message) => Self::Other(message),
        }
    }
}

impl From<AnalysisServiceError> for ArtifactServiceError {
    fn from(error: AnalysisServiceError) -> Self {
        match error {
            AnalysisServiceError::Db(error) => Self::Db(error),
            AnalysisServiceError::Io(error) => Self::Io(error),
            AnalysisServiceError::Read(message)
            | AnalysisServiceError::Extraction(message)
            | AnalysisServiceError::NotFound(_, message)
            | AnalysisServiceError::Other(message) => Self::Other(message),
            AnalysisServiceError::InvalidInput(message) => Self::InvalidInput(message),
            AnalysisServiceError::Unsupported(message) => Self::Unsupported(message),
        }
    }
}
