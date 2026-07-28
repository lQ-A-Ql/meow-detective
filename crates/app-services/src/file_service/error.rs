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
    #[error("security error: {0}")]
    Security(String),
    #[error("integrity error: {0}")]
    Integrity(String),
    #[error("path traversal: {0}")]
    PathTraversal(String),
    #[error("unsupported: {0}")]
    Unsupported(String),
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

    pub fn security(message: impl Into<String>) -> Self {
        Self::Security(message.into())
    }

    pub fn integrity(message: impl Into<String>) -> Self {
        Self::Integrity(message.into())
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

impl From<crate::source_db::ReadySourceError> for FileServiceError {
    fn from(error: crate::source_db::ReadySourceError) -> Self {
        match error {
            crate::source_db::ReadySourceError::Db(error) => Self::Db(error),
            crate::source_db::ReadySourceError::NotFound { .. } => {
                Self::NotFound(error.to_string())
            }
            crate::source_db::ReadySourceError::NotReady { .. } => {
                Self::InvalidInput(error.to_string())
            }
            crate::source_db::ReadySourceError::UnsupportedPlatform { .. } => {
                Self::Unsupported(error.to_string())
            }
        }
    }
}

impl From<crate::ceph_reconstruction::CephFsSourceError> for FileServiceError {
    fn from(error: crate::ceph_reconstruction::CephFsSourceError) -> Self {
        use crate::ceph_reconstruction::CephFsSourceError;

        match error {
            CephFsSourceError::InvalidInput(message) => Self::InvalidInput(message.to_string()),
            CephFsSourceError::IncompleteNamespace
            | CephFsSourceError::RetainedIncompleteSource => Self::Unsupported(error.to_string()),
            CephFsSourceError::PresenceNotProven(_) => Self::Unsupported(error.to_string()),
            CephFsSourceError::CapabilityInsufficient { .. } => {
                Self::Unsupported(error.to_string())
            }
            CephFsSourceError::StalePublication => Self::Other(error.to_string()),
            CephFsSourceError::ProcessingBusy => Self::Other(error.to_string()),
            CephFsSourceError::NamespaceAssembly(_)
            | CephFsSourceError::InconsistentState(_)
            | CephFsSourceError::Namespace(_) => Self::Other(error.to_string()),
            CephFsSourceError::Database(error) => Self::Db(error),
            CephFsSourceError::Io(error) => Self::Io(error),
        }
    }
}

impl From<persistence_sqlite::repositories::ceph_fs_namespace_repo::CephFsNamespaceRepoError>
    for FileServiceError
{
    fn from(
        error: persistence_sqlite::repositories::ceph_fs_namespace_repo::CephFsNamespaceRepoError,
    ) -> Self {
        use persistence_sqlite::repositories::ceph_fs_namespace_repo::CephFsNamespaceRepoError;

        match error {
            CephFsNamespaceRepoError::Database(error) => Self::Db(error),
            CephFsNamespaceRepoError::Invalid(_)
            | CephFsNamespaceRepoError::DeterminismConflict => Self::Other(error.to_string()),
        }
    }
}

impl transport::ServiceErrorCategory for FileServiceError {
    fn category(&self) -> transport::ErrorCategory {
        match self {
            Self::Db(_) | Self::Io(_) => transport::ErrorCategory::Io,
            Self::NotFound(_) => transport::ErrorCategory::Validation,
            Self::InvalidInput(_) | Self::PathTraversal(_) => transport::ErrorCategory::Validation,
            Self::Unsupported(_) => transport::ErrorCategory::Unsupported,
            Self::Security(_) => transport::ErrorCategory::Security,
            Self::Integrity(_) => transport::ErrorCategory::Parser,
            Self::Other(_) => transport::ErrorCategory::Internal,
        }
    }
}
