//! Read-only Ceph reconstruction capability layer.
//!
//! This crate owns the business-neutral Ceph reconstruction surface: RBD/CephFS
//! object reconstruction over source databases, BlueStore and RocksDB wire
//! decoding for the metadata-only evidence path, and the Ceph scope coverage
//! report artifacts. It sits above `source-kernel` (source database and probe
//! primitives) and below the evidence access / analysis / import layers, and it
//! must never depend on `app-services`.

pub mod ceph_reconstruction;
pub mod cluster_scope;
pub mod import_pipeline;
