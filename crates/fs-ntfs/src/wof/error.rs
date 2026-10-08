use std::io;

#[derive(Debug, thiserror::Error)]
pub(super) enum WofError {
    #[error("invalid WOF data: {0}")]
    Invalid(&'static str),
    #[error("WOF provider or format is unsupported: {0}")]
    Unsupported(&'static str),
    #[error("WOF source read failed: {0}")]
    Io(#[from] io::Error),
}

impl From<WofError> for io::Error {
    fn from(error: WofError) -> Self {
        let kind = match &error {
            WofError::Invalid(_) => io::ErrorKind::InvalidData,
            WofError::Unsupported(_) => io::ErrorKind::Unsupported,
            WofError::Io(error) => error.kind(),
        };
        Self::new(kind, error)
    }
}

pub(super) type Result<T> = std::result::Result<T, WofError>;
