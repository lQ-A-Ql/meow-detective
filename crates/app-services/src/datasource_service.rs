//! Facade over `evidence-access`; keeps `crate::datasource_service::*` paths
//! stable.
//!
//! The partition probe model lives in `source-kernel::partition_probe` and is
//! re-exported through `evidence-access::datasource_service`, so the historical
//! `crate::datasource_service::{probe, fs_magic, partition_index, types, ...}`
//! paths keep resolving.
pub use ::evidence_access::datasource_service::*;
