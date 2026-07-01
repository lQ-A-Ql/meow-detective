use persistence_sqlite::DbError;
use std::io;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum FileServiceError {
    #[error("database error: {0}")]
    Db(#[from] DbError),
    #[error("I/O error: {0}")]
    Io(#[from] io::Error),
    #[error("not found: {0}")]
    NotFound(String),
    #[error("invalid input: {0}")]
    InvalidInput(String),
    #[error("path traversal: {0}")]
    PathTraversal(String),
    #[error("other error: {0}")]
    Other(String),
}

impl FileServiceError {
    pub fn not_found(message: impl Into<String>) -> Self {
        Self::NotFound(message.into())
    }

    pub fn invalid_input(message: impl Into<String>) -> Self {
        Self::InvalidInput(message.into())
    }

    pub fn path_traversal(message: impl Into<String>) -> Self {
        Self::PathTraversal(message.into())
    }

    pub fn other(message: impl Into<String>) -> Self {
        Self::Other(message.into())
    }

    /// Returns true when the error indicates a read started past the recorded
    /// end-of-file.  Callers that are only interested in a bounded prefix (such
    /// as header extraction) can treat this as EOF rather than a fatal error.
    pub fn is_read_offset_beyond_size(&self) -> bool {
        matches!(self, Self::Other(message) if message == "Read offset exceeds file size")
    }
}

impl From<String> for FileServiceError {
    fn from(message: String) -> Self {
        Self::InvalidInput(message)
    }
}

impl transport::ServiceErrorCategory for FileServiceError {
    fn category(&self) -> transport::ErrorCategory {
        match self {
            Self::Db(_) | Self::Io(_) => transport::ErrorCategory::Io,
            Self::NotFound(_) => transport::ErrorCategory::Validation,
            Self::InvalidInput(_) | Self::PathTraversal(_) => transport::ErrorCategory::Validation,
            Self::Other(_) => transport::ErrorCategory::Internal,
        }
    }
}
