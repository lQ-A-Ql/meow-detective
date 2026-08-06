use persistence_sqlite::DbError;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum MountServiceError {
    #[error("database error: {0}")]
    Database(#[from] DbError),
    #[error("source metadata could not be read: {0}")]
    SourceMetadata(#[from] std::io::Error),
    #[error("source identity changed: expected {expected} bytes, found {actual} bytes")]
    SourceIdentityMismatch { expected: u64, actual: u64 },
    #[error("evidence emulation requires a valid persisted SHA-256 source fingerprint")]
    InvalidSourceFingerprint,
    #[error("source is not ready: {0}")]
    SourceNotReady(String),
    #[error("mount target was not found: {0}")]
    NotFound(String),
    #[error("partition cannot be mounted: {0}")]
    Unsupported(String),
    #[error("filesystem reader could not be opened: {0}")]
    Reader(String),
    #[error("mount catalog error: {0}")]
    Catalog(String),
}

impl transport::ServiceErrorCategory for MountServiceError {
    fn category(&self) -> transport::ErrorCategory {
        match self {
            Self::Database(_) | Self::SourceMetadata(_) | Self::Reader(_) | Self::Catalog(_) => {
                transport::ErrorCategory::Io
            }
            Self::SourceIdentityMismatch { .. } | Self::InvalidSourceFingerprint => {
                transport::ErrorCategory::Security
            }
            Self::SourceNotReady(_) | Self::NotFound(_) => transport::ErrorCategory::Validation,
            Self::Unsupported(_) => transport::ErrorCategory::Unsupported,
        }
    }
}
