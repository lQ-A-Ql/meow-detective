use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum FjallStorageError {
    #[error("database is locked: {path}")]
    StorageLocked { path: PathBuf },
    #[error("Fjall operation failed: {0}")]
    Engine(#[source] fjall::Error),
    #[error("storage I/O failed: {0}")]
    Io(#[source] std::io::Error),
    #[error("keyspace is not configured: {0}")]
    UnknownKeyspace(String),
    #[error("key part is too large: {length} bytes")]
    KeyPartTooLarge { length: usize },
    #[error("key encoding is truncated at byte {offset}")]
    TruncatedKey { offset: usize },
    #[error("unsupported key version: {0}")]
    UnsupportedKeyVersion(u8),
    #[error("invalid envelope: {0}")]
    InvalidEnvelope(String),
    #[error("envelope digest mismatch")]
    EnvelopeDigestMismatch,
    #[error("unsupported envelope format version: {0}")]
    UnsupportedEnvelopeVersion(u16),
    #[error("serialization failed: {0}")]
    Serialization(String),
    #[error("deserialization failed: {0}")]
    Deserialization(String),
    #[error("catalog record identity does not match its key")]
    CatalogRecordIdentityMismatch,
    #[error("catalog index entry is malformed")]
    CatalogIndexCorrupt,
    #[error("timeline record identity does not match its key")]
    TimelineRecordIdentityMismatch,
    #[error("timeline index entry is malformed")]
    TimelineIndexCorrupt,
    #[error("timeline timestamp encoding is invalid")]
    InvalidTimelineTimestampEncoding,
    #[error("source build path is invalid: {0}")]
    InvalidBuildPath(String),
    #[error("source build is missing the seal marker: {0}")]
    MissingSeal(PathBuf),
    #[error("source publication conflicts with an existing directory: {0}")]
    PublicationConflict(PathBuf),
    #[error("source manifest digest mismatch: expected {expected}, found {actual}")]
    ManifestDigestMismatch { expected: String, actual: String },
    #[error("source build is already published at {0}")]
    AlreadyPublished(PathBuf),
}

impl From<std::io::Error> for FjallStorageError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

impl From<fjall::Error> for FjallStorageError {
    fn from(error: fjall::Error) -> Self {
        Self::Engine(error)
    }
}

pub type Result<T> = std::result::Result<T, FjallStorageError>;
