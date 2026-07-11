//! Data source analysis service.
//!
//! Provides system information status reporting and bounded file classification.

mod artifact_builders;
mod candidates;
mod capability;
mod classification;
mod demo;
mod error;
mod extraction;
mod platforms;
mod provenance;
mod summary;
mod system_info;
mod use_cases;

pub use candidates::{
    collect_file_entries, discover_evidence_candidates, evidence_candidates_for_categories,
    evidence_category_defs, get_evidence_classification_summary, EvidenceCandidate,
    EvidenceCategoryDef,
};
pub use classification::{classify_files_by_magic, classify_files_by_metadata};
pub use demo::seed_analysis_demo_data;
pub use error::AnalysisServiceError;
pub use extraction::{
    extract_evtx_candidate, extract_linux_candidate, extract_registry_candidate,
    get_browser_history_summary, get_email_extraction_summary, get_evtx_event_summary,
    get_linux_artifact_summary, get_registry_extraction_summary, get_registry_structured_summary,
    run_analysis_extraction, ExtractionOutcome,
};
pub use platforms::{
    resolve_data_source_platform, select_evidence_scan_categories, validate_analysis_categories,
    validate_data_source_analysis_categories,
};
pub use summary::generate_analysis_summary;
pub use system_info::extract_system_info_for_case;
pub use use_cases::{
    classify_source_files, generate_source_analysis_summary, get_source_browser_summary,
    get_source_email_summary, get_source_evidence_summary, get_source_evtx_summary,
    get_source_linux_summary, get_source_registry_structured_summary, get_source_registry_summary,
    get_source_system_info, run_source_analysis_extraction, run_source_evidence_scan,
};

pub const DEFAULT_SAMPLE_SIZE: u32 = 1000;
pub const MAX_SAMPLE_SIZE: u32 = 5000;
pub const MAGIC_HEADER_LIMIT: usize = 8 * 1024;
pub const MAX_REGISTRY_ANALYSIS_BYTES: usize = 256 * 1024 * 1024;
pub const MAX_ANALYSIS_SOURCE_BYTES: usize = 128 * 1024 * 1024;
pub(crate) const ANALYSIS_EXTRACTOR_VERSION: &str = "1.0.0";

#[cfg(test)]
pub(crate) use provenance::{
    EVTX_BOOT_SHUTDOWN_PARSER, REGISTRY_SOFTWARE_PARSER, REGISTRY_SYSTEM_PARSER,
};

#[cfg(test)]
mod tests;
