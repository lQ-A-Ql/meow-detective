//! Bounded post-import analysis with per-worker staging and a single merge writer.

mod budget;
pub mod error;
mod extractor_policy;
mod finalize;
mod options;
pub mod priority_queue;
mod progress;
mod task_feed;
pub mod tier;
mod worker_coordinator;
mod worker_model;
mod worker_pool;
mod worker_runtime;
mod worker_staging;

pub use budget::{
    content_budget_for_mode, default_memory_hard_limit_mb, default_memory_soft_limit_mb,
    ContentBudget,
};
pub use error::ImportAnalysisError;
pub use options::{
    AnalysisProgressCallback, ImportAnalysisMode, ImportAnalysisOptions, ImportAnalysisStats,
    JobOutcomeCounts, PostImportPipelineError, PostImportPipelineOptions,
};
pub use progress::{current_rss_mb, peak_rss_mb};
pub use worker_pool::{
    default_analysis_worker_count, resolve_analysis_worker_count, run_import_analysis_staging,
    run_post_import_pipeline_with_counts,
};

#[cfg(test)]
#[path = "../../tests/unit/import_analysis/mod.rs"]
mod tests;
