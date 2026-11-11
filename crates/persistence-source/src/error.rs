use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum SourceError {
    #[error("invalid source identifier: {0}")]
    InvalidSourceId(String),
    #[error("storage root does not exist: {0}")]
    MissingRoot(PathBuf),
    #[error("storage root is not a directory: {0}")]
    InvalidRoot(PathBuf),
    #[error("source identity is missing")]
    IdentityMissing,
    #[error("source detail storage is missing")]
    DetailStorageMissing,
    #[error("source generation directory is missing")]
    GenerationDirectoryMissing,
    #[error("source generation mismatch: expected {expected}, found {actual}")]
    GenerationMismatch { expected: String, actual: String },
    #[error("source publication conflict at {0}")]
    PublicationConflict(PathBuf),
    #[error("source store is still in use: {0}")]
    StillInUse(PathBuf),
    #[error("source storage is being removed: {0}")]
    SourceClosing(PathBuf),
    #[error("source storage registry lock is poisoned")]
    RegistryPoisoned,
    #[error("source details are not ready: {0}")]
    SourceNotReady(String),
    #[error("source storage operation failed: {0}")]
    System(String),
    #[error(transparent)]
    Db(#[from] persistence_sqlite::DbError),
    #[error(transparent)]
    Fjall(#[from] persistence_fjall::FjallStorageError),
    #[error(transparent)]
    Artifacts(#[from] artifact_storage::ArtifactStorageError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, SourceError>;
