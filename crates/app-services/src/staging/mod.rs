//! Staging lifecycle for parallel import and analysis workers.

mod cleanup;
mod error;
mod merge;
mod partition_root;
mod schema;
mod writer;

pub use cleanup::cleanup_staging;
pub use error::StagingError;
pub use merge::{
    merge_all_staging_to_main, merge_all_staging_to_main_with_stats,
    merge_analysis_staging_to_main, AnalysisMergeStats, StagingMergeStats,
};
pub use schema::{
    analysis_staging_db_path, enum_staging_db_path, open_analysis_staging, open_enum_staging,
    open_partition_staging, staging_db_path, staging_dir, ImportPhase, PartitionEntry,
    PartitionStatus, StagingManifest,
};
pub use writer::{
    analysis_staging_counts, get_staging_meta, get_worker_meta, set_staging_meta, set_worker_meta,
    staging_db_row_count,
};
