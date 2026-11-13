//! Source build finalize/seal/publish.

mod analysis_gc;
pub mod build;
mod gc_scheduler;

pub use analysis_gc::{
    has_pending_analysis_gc, reclaim_analysis_generations, AnalysisGenerationGcOutcome,
};
pub(crate) use gc_scheduler::notify_generation_source_idle;
pub use gc_scheduler::{drain_analysis_generation_gc, request_analysis_generation_gc};

pub use build::{
    discard_source_build_db, finalize_source_build_db, open_fresh_source_build_db,
    preserve_unpublished_source_build_db, publish_source_build_db, source_build_db_path,
    verify_finalized_source_db, SOURCE_BUILD_WAL_AUTOCHECKPOINT_PAGES,
};
