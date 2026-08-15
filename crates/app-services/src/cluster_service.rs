use thiserror::Error;

use crate::datasource_service;

mod linux_import;
mod pve_plan;

pub use linux_import::{
    assess_linux_cluster_cephfs_presence, plan_linux_cluster_import, register_linux_cluster_import,
    update_linux_cluster_import_state, write_linux_cluster_manifest, LinuxClusterImportPlan,
    LinuxClusterMemberPlan,
};
pub use pve_plan::{
    parse_cluster, plan_cluster_parse, ClusterEvidenceSource, ClusterParseBoundary,
    ClusterParsePlan, ClusterParseRequest,
};

#[derive(Debug, Error)]
pub enum ClusterServiceError {
    #[error("cluster parsing is planned but not implemented in this milestone")]
    Unsupported,
    #[error("at least two evidence sources are required for cluster parsing")]
    InsufficientSources,
    #[error("cluster root must point to a readable directory")]
    InvalidClusterRoot,
    #[error("linux cluster import did not find supported E01/RAW images in the selected folder")]
    NoSupportedImages,
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("data source classification error: {0}")]
    Classification(#[from] datasource_service::DataSourceError),
    #[error("database error: {0}")]
    Db(#[from] persistence_sqlite::DbError),
    #[error("serialization error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("CephFS presence assessment failed: {0}")]
    CephFsPresence(#[from] crate::ceph_reconstruction::CephFsPresenceError),
}

impl transport::ServiceErrorCategory for ClusterServiceError {
    fn category(&self) -> transport::ErrorCategory {
        match self {
            Self::Unsupported => transport::ErrorCategory::Unsupported,
            Self::InsufficientSources | Self::InvalidClusterRoot | Self::NoSupportedImages => {
                transport::ErrorCategory::Validation
            }
            Self::Io(_) | Self::Db(_) => transport::ErrorCategory::Io,
            Self::Classification(e) => e.category(),
            Self::Json(_) => transport::ErrorCategory::Internal,
            Self::CephFsPresence(error) => error.category(),
        }
    }
}

pub type Result<T> = std::result::Result<T, ClusterServiceError>;

#[cfg(test)]
#[path = "../tests/unit/cluster_service.rs"]
mod tests;
