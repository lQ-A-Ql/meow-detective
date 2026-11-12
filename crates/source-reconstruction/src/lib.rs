//! Source reconstruction capability layer.
//!
//! Owns cluster/CEPH scope reconstruction, derived-source materialization,
//! deleted-file recovery and read-only mount orchestration. It sits above
//! `import-core` and `evidence-access` and must never depend on `app-services`
//! or Tauri.

pub mod cluster_service;
pub mod deleted_recovery;
pub mod derived_source_service;
pub mod mount_service;

#[cfg(test)]
#[path = "../tests/unit/mod.rs"]
mod tests;
