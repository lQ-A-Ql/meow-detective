//! Import pipeline execution.
//!
//! Orchestrates data-source attachment, filesystem enumeration, staging merge,
//! and post-import analysis. Tauri-specific command wrappers live in the
//! `forensics-desktop` crate.

mod ceph;
mod ceph_metadata_audit;
mod ceph_persistence;
pub use ::ceph_reconstruction::import_pipeline::ceph_bluestore_omap::{
    BlueStoreOmapError, BlueStoreOmapFragment, BlueStoreOmapLimits, BlueStoreOmapOwner,
    BlueStoreOmapOwnerKind, BlueStoreOmapPoolScope, BlueStoreOmapScope, BlueStoreOmapScopeRecord,
    BlueStoreOmapSnapshot, BlueStoreRbdDirectoryMapping, BlueStoreRbdHeader,
};
pub(crate) use ::ceph_reconstruction::import_pipeline::{
    ceph_bluefs_file_reader, ceph_bluefs_records, ceph_bluefs_replay, ceph_bluestore_omap_records,
    ceph_device_binding, ceph_rocksdb_inventory,
};
mod context;
mod emit;
mod execute;
mod fjall_projection;
pub(crate) use fjall_projection::{
    begin_partition_projection, mark_catalog_projection_seeded,
    mirror_analysis_records_before_sqlite_commit, mirror_catalog_records_from_connection,
    mirror_partition_data_for_import,
};
pub mod options;
pub mod partition;
mod phases;
mod post_ready;
pub(crate) mod profile;
mod runtime;
mod setup;

pub use emit::{ImportEventSink, NoopImportEventSink};
pub use execute::{
    execute_import_job, execute_import_job_profiled, execute_import_job_with_counts,
};
pub use options::{ImportJobOptions, JobOutcomeCounts};
pub use partition::{
    build_partition_work, enumerate_image_data_source, enumerate_partition_with_fs,
    format_partition_progress_detail, format_partition_record_root_name,
    format_partition_root_name, PartitionEnumerationRequest,
};
pub use profile::ImportProfiler;

#[cfg(test)]
#[path = "../../tests/unit/import_pipeline/mod.rs"]
mod tests;
