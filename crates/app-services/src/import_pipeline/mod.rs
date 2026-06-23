//! Import pipeline execution.
//!
//! Orchestrates data-source attachment, filesystem enumeration, staging merge,
//! and post-import analysis. Tauri-specific command wrappers live in the
//! `forensics-desktop` crate.

mod emit;
pub mod execute;
pub mod options;
pub mod partition;

pub use execute::{execute_import_job, execute_import_job_with_counts};
pub use options::{ImportJobOptions, JobOutcomeCounts};
pub use partition::{
    build_partition_work, enumerate_image_data_source, enumerate_partition_with_fs,
    format_partition_progress_detail, format_partition_record_root_name,
    format_partition_root_name,
};

#[cfg(test)]
mod tests;
