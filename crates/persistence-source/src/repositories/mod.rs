//! Source-scoped detail repositories.
//!
//! These repositories read and write the per-source `source.db` control tables
//! and the sibling `source.fjall` detail store. They are owned by this crate;
//! `persistence-sqlite` keeps only app.db / case-graph.sqlite repositories plus
//! the generic connection and migration utilities.

pub mod artifact_repo;
pub mod catalog_file_repo;
pub mod ceph_fs_namespace_repo;
pub mod content_index_repo;
pub mod correlation_repo;
pub mod file_repo;
pub mod infrastructure_network_fact_repo;
mod staging_merge_validation;
pub mod staging_repo;
pub mod systemd_unit_repo;
pub mod timeline_repo;

pub use staging_repo::StagingRepo;
