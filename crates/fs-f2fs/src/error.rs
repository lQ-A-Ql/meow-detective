use std::io;

use thiserror::Error;

pub type Result<T> = std::result::Result<T, F2fsError>;

#[derive(Debug, Error)]
pub enum F2fsError {
    #[error("F2FS I/O error: {0}")]
    Io(#[from] io::Error),
    #[error("invalid F2FS metadata: {0}")]
    Invalid(String),
    #[error("unsupported F2FS capability: {0}")]
    Unsupported(String),
    #[error("F2FS path was not found: {0}")]
    NotFound(String),
}

impl F2fsError {
    pub(crate) fn from_failed_copies(
        structure: &str,
        primary: F2fsError,
        backup: F2fsError,
    ) -> Self {
        let message = format!(
            "both F2FS {structure} copies are unavailable: primary={primary}; backup={backup}"
        );
        if matches!(primary, Self::Unsupported(_)) || matches!(backup, Self::Unsupported(_)) {
            Self::Unsupported(message)
        } else {
            Self::Invalid(message)
        }
    }

    pub(crate) fn into_io(self) -> io::Error {
        match self {
            Self::Io(error) => error,
            Self::Invalid(message) => io::Error::new(io::ErrorKind::InvalidData, message),
            Self::Unsupported(message) => io::Error::new(io::ErrorKind::Unsupported, message),
            Self::NotFound(path) => io::Error::new(
                io::ErrorKind::NotFound,
                format!("F2FS path was not found: {path}"),
            ),
        }
    }
}
