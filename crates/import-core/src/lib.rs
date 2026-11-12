//! Import pipeline capability layer.
//!
//! Owns import planning, staging, enumeration, scheduling, checkpointing and
//! publication for evidence sources. It sits above `analysis-core` and
//! `evidence-access` and must never depend on `app-services` or Tauri.

pub mod import_analysis;
pub mod import_group_gate;
pub mod import_job_coordinator;
pub mod import_pipeline;
pub mod import_precheck;
pub mod import_scheduler;
pub mod import_state;
pub mod parallel_enum;
pub(crate) mod resume;
pub mod search_metadata_projection;
pub mod staging;
pub mod step_recorder;
pub mod timeline_projection;

#[cfg(test)]
#[path = "../tests/unit/mod.rs"]
mod tests;
