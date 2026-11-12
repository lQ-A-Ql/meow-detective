//! Application services layer for the Forensics Workbench.
//!
//! This crate orchestrates backend business logic: case management, evidence
//! ingestion, filesystem enumeration, artifact/timeline/search analysis,
//! correlation, reporting, and governance snapshots. It sits between the Tauri
//! command layer and the domain/infrastructure crates, and owns the SQLite
//! case database schema through `persistence-sqlite`.

pub use source_kernel::e01_reader_cache::{
    clear_e01_reader_cache, clear_e01_reader_cache_for_case, open_e01_reader_cached,
};
pub use source_kernel::{
    active_case, artifact_storage_service, connection, performance, processing_phase_service,
    resource_budget, runtime_resources, source_db, storage_manifest, storage_runtime,
};

pub mod analysis_service;
pub mod artifact_service;
pub mod batch_service;
pub mod bitlocker_runtime;
pub mod bitlocker_service;
pub mod bolt_reader_service;
pub mod carving_service;
pub mod case_projection_service;
pub mod case_service;
pub mod ceph_reconstruction;
pub mod cluster_service;
pub mod content_index_service;
pub mod correlation;
pub mod datasource_service;
pub mod deleted_recovery;
pub mod derived_source_service;
pub mod digest_service;
pub mod emulation_bypass;
pub mod emulation_cow_reader;
pub mod emulation_efi_fallback;
pub mod emulation_ext4_repair;
pub mod emulation_fs_repair;
pub mod emulation_linux_bypass;
pub mod emulation_osdata;
pub mod entity_resolution;
pub mod evidence_tasks;
pub mod file_service;
pub mod governance;
pub mod graph_service;
pub mod hash_service;
pub mod import_analysis;
pub mod import_group_gate;
pub mod import_job_coordinator;
pub mod import_pipeline;
pub mod import_precheck;
pub mod import_scheduler;
pub mod import_state;
pub mod infrastructure_graph_service;
pub mod job_service;
pub mod ledger_service;
pub mod linux_artifact_job;
pub mod mcp_host_service;
pub mod mount_service;
pub mod nested_evidence_service;
pub mod notebook_service;
pub mod parallel_enum;
mod partition_capabilities;
pub mod partition_query_service;
pub mod partition_storage_service;
pub mod plugin_action_service;
pub mod plugin_loader;
pub mod registry_browser_service;
pub mod report;
pub mod rule_pack;
mod search_metadata_projection;
pub mod search_service;
pub mod source_runtime;
pub mod staging;
pub mod step_recorder;
pub mod storage_recovery_service;
pub mod text_service;
pub mod timeline_service;
pub mod v2_governance_service;
pub mod v3_governance_service;
pub mod wechat_key_service;

#[cfg(test)]
#[path = "../tests/unit/mod.rs"]
mod tests;
