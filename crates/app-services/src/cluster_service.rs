use thiserror::Error;

use crate::datasource_service;

mod audit_parser;
mod etcd_bolt;
mod etcd_wal;
mod kind;
mod kubeconfig_parser;
mod kubernetes_analysis;
mod kubernetes_dispatch;
mod kubernetes_inventory;
mod kubernetes_parser_error;
mod kubernetes_plan;
mod kubernetes_yaml;
mod linux_import;
mod manifest_parser;
mod pve_plan;

pub mod kubernetes_paths;

pub use kind::LinuxClusterKind;
pub use kubernetes_paths::KubernetesArtifactKind;

pub use audit_parser::{
    parse_kubernetes_audit_log, KubernetesAuditEvent, KubernetesAuditIndicator,
    KubernetesAuditParseResult,
};
pub use etcd_bolt::{parse_etcd_bolt_metadata, EtcdBoltEntry, EtcdBoltSummary};
pub use etcd_wal::{parse_etcd_wal, EtcdWalRecord, EtcdWalSummary};
pub use kubeconfig_parser::{
    parse_kubeconfig, KubeconfigCluster, KubeconfigContext, KubeconfigSummary, KubeconfigUser,
};
pub use kubernetes_analysis::get_source_kubernetes_cluster_summary;
pub use kubernetes_dispatch::{parse_kubernetes_artifact, KubernetesParsedArtifact};
pub use kubernetes_inventory::{
    discover_kubernetes_cluster_artifacts, discover_kubernetes_member_artifacts,
    KubernetesClusterArtifactInventory, KubernetesMemberArtifact,
    KubernetesMemberArtifactInventory,
};
pub use kubernetes_parser_error::KubernetesParserError;
pub use kubernetes_plan::{
    plan_kubernetes_cluster_import, KubernetesClusterPlan, KUBERNETES_CLUSTER_PROFILE,
};
pub use linux_import::{
    assess_linux_cluster_cephfs_presence, plan_linux_cluster_import,
    read_linux_cluster_coverage_report, register_linux_cluster_import,
    update_linux_cluster_import_state, write_linux_cluster_coverage_report,
    write_linux_cluster_manifest, LinuxClusterImportPlan, LinuxClusterMemberPlan,
};
pub use manifest_parser::{
    parse_static_pod_manifests, ManifestContainer, StaticPodManifestSummary,
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
    #[error("cluster id is invalid")]
    InvalidClusterId,
    #[error("linux cluster coverage report is invalid")]
    InvalidCoverageReport,
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
            Self::InsufficientSources
            | Self::InvalidClusterRoot
            | Self::InvalidClusterId
            | Self::InvalidCoverageReport
            | Self::NoSupportedImages => transport::ErrorCategory::Validation,
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
