//! Storage, connection, and resource kernel shared by application services.
//!
//! This crate owns the business-neutral foundation of the application-service
//! layer: case/source database connections, source build and ready-state
//! routing, source storage handles, process resource budgets and admission,
//! E01 reader plans and caching, source publication leases, and the partition
//! probe model. It must never depend on `app-services`.

pub mod active_case;
pub mod artifact_storage_service;
pub mod connection;
pub mod derived_source_catalog;
pub mod e01_reader_cache;
pub mod partition_probe;
pub mod performance;
pub mod processing_phase_service;
pub mod resource_budget;
pub mod runtime_resources;
pub mod source_db;
pub mod source_runtime;
pub mod storage_manifest;
pub mod storage_runtime;

pub use e01_reader_cache::{
    clear_e01_reader_cache, clear_e01_reader_cache_for_case, open_e01_reader_cached,
};
