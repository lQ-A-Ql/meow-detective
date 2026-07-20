use thiserror::Error;

#[derive(Debug, Error)]
pub enum CephFsSourceError {
    #[error("CephFS derived source has invalid input: {0}")]
    InvalidInput(&'static str),
    #[error("CephFS namespace is incomplete and was retained without publication")]
    IncompleteNamespace,
    #[error("CephFS derived source already has an unpublished retained source database")]
    RetainedIncompleteSource,
    #[error("CephFS published source database is stale and must be rebuilt")]
    StalePublication,
    #[error("CephFS presence proof is insufficient for reconstruction: {0}")]
    PresenceNotProven(&'static str),
    #[error("CephFS namespace assembly failed: {0}")]
    NamespaceAssembly(#[from] ceph_wire::CephWireError),
    #[error("CephFS Catalog materialization is already running")]
    ProcessingBusy,
    #[error("CephFS derived source state is inconsistent: {0}")]
    InconsistentState(String),
    #[error(
        "CephFS source capability '{actual}' does not satisfy required capability '{required}'"
    )]
    CapabilityInsufficient {
        required: &'static str,
        actual: String,
    },
    #[error("CephFS namespace persistence failed: {0}")]
    Namespace(
        #[from] persistence_sqlite::repositories::ceph_fs_namespace_repo::CephFsNamespaceRepoError,
    ),
    #[error("CephFS derived source database failed: {0}")]
    Database(#[from] persistence_sqlite::DbError),
    #[error("CephFS derived source I/O failed: {0}")]
    Io(#[from] std::io::Error),
}

impl transport::ServiceErrorCategory for CephFsSourceError {
    fn category(&self) -> transport::ErrorCategory {
        match self {
            Self::InvalidInput(_) => transport::ErrorCategory::Validation,
            Self::IncompleteNamespace | Self::RetainedIncompleteSource => {
                transport::ErrorCategory::Unsupported
            }
            Self::StalePublication | Self::PresenceNotProven(_) | Self::NamespaceAssembly(_) => {
                transport::ErrorCategory::Parser
            }
            Self::ProcessingBusy => transport::ErrorCategory::Timeout,
            Self::InconsistentState(_) | Self::Namespace(_) => transport::ErrorCategory::Parser,
            Self::CapabilityInsufficient { .. } => transport::ErrorCategory::Unsupported,
            Self::Database(_) | Self::Io(_) => transport::ErrorCategory::Io,
        }
    }
}

pub type CephFsSourceResult<T> = Result<T, CephFsSourceError>;
