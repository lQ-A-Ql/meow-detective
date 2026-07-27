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
pub mod case_service;
pub mod ceph_reconstruction;
pub mod cluster_service;
pub mod connection;
pub mod correlation;
pub use correlation::get_correlation_snapshot;
pub mod datasource_service;
pub mod deleted_recovery;
mod e01_reader_cache;
pub mod entity_extraction;
pub mod entity_resolution;
pub mod error_ext;
pub mod file_carving;
pub mod file_service;
pub mod governance;
pub mod graph_service;
pub mod hash_service;
pub mod import_analysis;
pub mod import_pipeline;
pub mod import_precheck;
pub mod import_report;
pub mod import_scheduler;
pub mod import_state;
pub mod job_service;
pub mod notebook_service;
pub mod parallel_enum;
pub mod performance;
pub mod processing_phase_service;
pub mod report;
pub use report::{
    generate_csv_artifacts, generate_csv_correlation, generate_html_report, generate_json_export,
    get_report_history, get_report_templates,
};
pub mod rule_pack;
pub mod search_service;
pub mod source_db;
pub mod staging;
pub mod step_recorder;
pub mod step_replay;
pub mod streaming;
pub mod text_service;
pub mod timeline_service;
pub mod v2_governance_service;
pub mod v3_governance_service;
