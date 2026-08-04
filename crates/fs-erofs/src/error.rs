use std::io;

use thiserror::Error;

pub type Result<T> = std::result::Result<T, ErofsError>;

#[derive(Debug, Error)]
pub enum ErofsError {
    #[error("EROFS I/O error: {0}")]
    Io(#[from] io::Error),
    #[error("invalid EROFS metadata: {0}")]
    Invalid(String),
    #[error("unsupported EROFS capability: {0}")]
    Unsupported(String),
    #[error("EROFS path was not found: {0}")]
    NotFound(String),
}

impl ErofsError {
    pub(crate) fn into_io(self) -> io::Error {
        match self {
            Self::Io(error) => error,
            Self::Invalid(message) => io::Error::new(io::ErrorKind::InvalidData, message),
            Self::Unsupported(message) => io::Error::new(io::ErrorKind::Unsupported, message),
            Self::NotFound(path) => io::Error::new(
                io::ErrorKind::NotFound,
                format!("EROFS path was not found: {path}"),
            ),
        }
    }
}
