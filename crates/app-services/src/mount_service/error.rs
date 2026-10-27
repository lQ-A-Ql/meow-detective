use persistence_sqlite::DbError;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum MountServiceError {
    #[error("OS identity could not be read: {0}")]
    OsRelease(#[from] artifacts_linux::LinuxArtifactError),
    #[error("emulation parent view failed: {0}")]
    DerivedVolume(#[from] evidence_emulation::EmulationError),
    #[error("emulation block reader failed: {0}")]
    Block(#[from] evidence_block::BlockDeviceError),
    #[error("a direct NTFS volume requires recovery media for its generated boot disk")]
    DirectVolumeRecoveryRequired,
    #[error("database error: {0}")]
    Database(#[from] DbError),
    #[error("original-disk fingerprint lookup failed: {0}")]
    Digest(#[from] crate::digest_service::DigestServiceError),
    #[error("source metadata could not be read: {0}")]
    SourceMetadata(#[from] std::io::Error),
    #[error("source identity changed: expected {expected} bytes, found {actual} bytes")]
    SourceIdentityMismatch { expected: u64, actual: u64 },
    #[error("evidence emulation requires a valid persisted SHA-256 source fingerprint")]
    InvalidSourceFingerprint,
    #[error(
        "source content does not match the persisted import SHA-256 (expected {expected}, recomputed {actual})"
    )]
    SourceHashMismatch { expected: String, actual: String },
    #[error("source SHA-256 could not be recomputed for integrity verification: {0}")]
    SourceHashVerify(String),
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
            Self::OsRelease(artifacts_linux::LinuxArtifactError::Io(_)) => {
                transport::ErrorCategory::Io
            }
            Self::OsRelease(artifacts_linux::LinuxArtifactError::Unsupported { .. }) => {
                transport::ErrorCategory::Unsupported
            }
            Self::OsRelease(_) => transport::ErrorCategory::Parser,
            Self::DerivedVolume(evidence_emulation::EmulationError::InvalidDerivedVolume(_)) => {
                transport::ErrorCategory::Unsupported
            }
            Self::DerivedVolume(_) | Self::Block(_) => transport::ErrorCategory::Io,
            Self::Database(_) | Self::SourceMetadata(_) | Self::Reader(_) | Self::Catalog(_) => {
                transport::ErrorCategory::Io
            }
            Self::Digest(error) => error.category(),
            Self::SourceIdentityMismatch { .. }
            | Self::InvalidSourceFingerprint
            | Self::SourceHashMismatch { .. } => transport::ErrorCategory::Security,
            Self::SourceHashVerify(_) => transport::ErrorCategory::Io,
            Self::SourceNotReady(_) | Self::NotFound(_) => transport::ErrorCategory::Validation,
            Self::Unsupported(_) | Self::DirectVolumeRecoveryRequired => {
                transport::ErrorCategory::Unsupported
            }
        }
    }
}
