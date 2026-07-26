mod aggregation;
mod error;
mod extraction;
mod persistence;
mod query;
mod registry;
mod source_routing;

pub use aggregation::{
    get_artifact_families_for_case, get_artifact_family_counts_for_case,
    get_artifact_rows_for_case, get_artifact_rows_page_for_case,
    get_artifact_rows_page_with_cursor_for_case,
};
pub use error::ArtifactServiceError;
pub use extraction::{
    run_extractors_on_file, run_extractors_parallel, run_targeted_evidence_scan,
    ArtifactExtractionStats, EvidenceScanStats,
};
pub use persistence::store_artifacts;
pub use query::{
    get_artifact_families_from_db, get_artifact_family_counts, get_artifact_row_by_id,
    get_artifact_rows_from_db,
};
pub use registry::create_registry;
pub use source_routing::get_artifact_row_by_id_for_case;

pub(crate) use aggregation::{
    get_source_attributed_artifact_family_counts_for_case, SourceArtifactFamilyCount,
};

#[cfg(test)]
#[path = "../tests/unit/artifact_service.rs"]
mod tests;
