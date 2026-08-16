//! Application services layer for the Forensics Workbench.
//!
//! This crate orchestrates backend business logic: case management, evidence
//! ingestion, filesystem enumeration, artifact/timeline/search analysis,
//! correlation, reporting, and governance snapshots. It sits between the Tauri
//! command layer and the domain/infrastructure crates, and owns the SQLite
//! case database schema through `persistence-sqlite`.

#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

pub mod active_case;
pub mod analysis_service;
pub mod artifact_service;
pub mod batch_service;
pub mod bitlocker_runtime;
pub mod bitlocker_service;
pub mod case_service;
pub mod ceph_reconstruction;
pub mod cluster_service;
pub mod connection;
pub mod correlation;
pub mod datasource_service;
pub mod deleted_recovery;
mod derived_source_catalog;
pub mod derived_source_service;
mod e01_reader_cache;
pub mod emulation_bypass;
pub mod emulation_cow_reader;
pub mod emulation_efi_fallback;
pub mod emulation_fs_repair;
pub mod emulation_linux_bypass;
pub mod emulation_osdata;
pub mod entity_resolution;
pub mod file_service;
pub mod governance;
pub mod graph_service;
pub mod hash_service;
pub mod import_analysis;
pub mod import_pipeline;
pub mod import_precheck;
pub mod import_scheduler;
pub mod import_state;
pub mod job_service;
pub mod mount_service;
pub mod notebook_service;
pub mod parallel_enum;
mod partition_capabilities;
pub mod performance;
pub mod plugin_action_service;
pub mod plugin_loader;
pub mod processing_phase_service;
pub mod report;
pub mod rule_pack;
pub mod runtime_resources;
pub mod search_service;
pub mod source_db;
pub mod staging;
pub mod step_recorder;
pub mod text_service;
pub mod timeline_service;
pub mod v2_governance_service;
pub mod v3_governance_service;
pub mod wechat_key_service;
