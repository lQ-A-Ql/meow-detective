use std::sync::atomic::AtomicBool;

use domain::{DataSource, DataSourceId};
use thiserror::Error;

mod catalog_build;
mod catalog_manifest;
mod filesystem;
mod finalizer;
mod materialization;
mod orchestration;

pub use orchestration::{
    finalize_rbd_source_processing, finalize_rbd_source_processing_with_cancel,
    materialize_rbd_sources_for_cluster, materialize_rbd_sources_for_cluster_with_cancel,
    verify_derived_source_catalog,
};

#[derive(Debug, Error)]
pub enum DerivedSourceError {
    #[error("Ceph cluster {0} was not found")]
    ClusterNotFound(String),
    #[error("Ceph cluster {cluster_id} is not ready: {state}")]
    ClusterNotReady { cluster_id: String, state: String },
    #[error("Ceph cluster has no complete OSD source set")]
    IncompleteCluster,
    #[error("Ceph source {data_source_id} has no usable OSD inventory")]
    MissingInventory { data_source_id: String },
    #[error("Ceph source {data_source_id} has conflicting OSD inventory")]
    ConflictingInventory { data_source_id: String },
    #[error("RBD reconstruction failed: {0}")]
    Reconstruction(String),
    #[error("RBD image {0} was not found")]
    ImageNotFound(String),
    #[error("RBD image {0} has no supported filesystem")]
    NoFilesystem(String),
    #[error("RBD derived source has an invalid {field}")]
    InvalidIdentity { field: &'static str },
    #[error("RBD derived source processing phase '{phase}' is already running")]
    ProcessingBusy { phase: &'static str },
    #[error("RBD derived source processing was cancelled")]
    ProcessingCancelled,
    #[error(
        "RBD derived source post-Catalog processing is incomplete: {failed_count} failed, {deferred_count} deferred, {unfinished_count} unfinished"
    )]
    IncompleteProcessing {
        failed_count: usize,
        deferred_count: usize,
        unfinished_count: usize,
    },
    #[error("RBD derived source state is inconsistent: {0}")]
    InconsistentState(String),
    #[error(
        "RBD derived source Catalog is incomplete because {diagnostic_count} filesystem entries or directories could not be enumerated reliably ({diagnostic_breakdown})"
    )]
    IncompleteCatalog {
        diagnostic_count: usize,
        diagnostic_breakdown: String,
    },
    #[error("RBD logical volume layout is unsupported: {0}")]
    UnsupportedLvm(String),
    #[error("RBD derived source database failed: {0}")]
    Database(#[from] persistence_sqlite::DbError),
    #[error("RBD derived source I/O failed: {0}")]
    Io(#[from] std::io::Error),
}

pub type DerivedSourceResult<T> = Result<T, DerivedSourceError>;

#[derive(Debug, Clone)]
pub struct MaterializedRbdSource {
    pub data_source: DataSource,
    pub file_count: u64,
    pub directory_count: u64,
    pub total_size: u64,
    pub created_count: u64,
    pub modified_count: u64,
    pub accessed_count: u64,
    pub changed_count: u64,
    pub catalog_digest: String,
}

pub(super) fn derived_data_source_id(
    cluster_id: &str,
    image_id: &str,
) -> DerivedSourceResult<DataSourceId> {
    validate_identity_component("cluster ID", cluster_id)?;
    validate_identity_component("image ID", image_id)?;
    Ok(DataSourceId(format!("rbd-{cluster_id}-{image_id}")))
}

fn validate_identity_component(field: &'static str, value: &str) -> DerivedSourceResult<()> {
    if value.is_empty()
        || value.len() > 128
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    {
        return Err(DerivedSourceError::InvalidIdentity { field });
    }
    Ok(())
}

fn ensure_not_cancelled(cancel_token: &AtomicBool) -> DerivedSourceResult<()> {
    if cancel_token.load(std::sync::atomic::Ordering::Relaxed) {
        Err(DerivedSourceError::ProcessingCancelled)
    } else {
        Ok(())
    }
}

#[cfg(test)]
#[path = "../tests/unit/derived_source_service.rs"]
mod tests;
