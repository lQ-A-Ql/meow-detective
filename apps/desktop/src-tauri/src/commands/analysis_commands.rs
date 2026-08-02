//! Data source analysis command facade.

mod extraction;
mod governance;
mod queries;
mod support;

pub use extraction::{
    generate_analysis_summary, get_browser_history_summary, get_email_extraction_summary,
    get_evtx_event_summary, get_linux_artifact_summary, get_registry_extraction_summary,
    get_registry_structured_summary, run_analysis_extraction, run_evidence_classification,
};
pub use governance::{
    get_case_overview_snapshot, get_correlation_snapshot, get_v2_governance_snapshot,
    get_v3_governance_snapshot,
};
pub use queries::{
    classify_files, get_evidence_classification_summary, get_file_classification_board,
    get_system_info,
};

#[cfg(test)]
#[path = "../../tests/unit/commands/analysis_commands_test.rs"]
mod tests;
