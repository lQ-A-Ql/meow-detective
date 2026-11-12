//! Read-only evidence access layer.
//!
//! This crate owns the evidence-facing read surface shared by every other
//! application-service domain: source attachment and LVM expansion, partition
//! projection, source runtime factories, file browsing/enumeration, preview and
//! range readers, BitLocker runtime unlocks, mdraid reader assembly, digest and
//! hashing helpers, and the source-local catalog projection helpers used by
//! enumeration and import. It sits above `source-kernel` and
//! `ceph-reconstruction` and must never depend on `app-services`.

pub(crate) use ::ceph_reconstruction::ceph_reconstruction;
pub use source_kernel::{
    active_case, artifact_storage_service, connection, derived_source_catalog, e01_reader_cache,
    performance, processing_phase_service, resource_budget, runtime_resources, source_db,
    storage_manifest, storage_runtime,
};

pub mod bitlocker_runtime;
pub mod bolt_reader_service;
pub mod catalog_projection;
pub mod datasource_service;
pub mod digest_service;
pub mod file_service;
pub mod hash_service;
pub mod linux_identity_service;
pub mod mdraid;
pub mod nested_lineage;
pub mod partition_capabilities;
pub mod partition_query_service;
pub mod partition_storage_service;
pub mod source_runtime;
pub mod text_service;

#[cfg(test)]
#[path = "../tests/unit/mod.rs"]
mod tests;
