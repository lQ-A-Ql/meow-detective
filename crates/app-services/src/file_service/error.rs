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
}

impl From<String> for FileServiceError {
    fn from(message: String) -> Self {
        Self::InvalidInput(message)
    }
}
