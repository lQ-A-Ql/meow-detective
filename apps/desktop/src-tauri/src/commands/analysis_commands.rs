//! Data source analysis command facade.

mod classification;
mod extraction;
mod governance;
mod linux_cluster;
mod linux_events;
mod queries;
mod support;

pub use classification::{classify_files, get_file_classification_board};
pub use extraction::{
    generate_analysis_summary, get_browser_history_summary, get_email_extraction_summary,
    get_evtx_event_summary, get_linux_artifact_summary, get_plugin_family_entries,
    get_registry_extraction_summary, get_registry_structured_summary, list_plugin_modules,
    run_analysis_extraction, run_evidence_classification,
};
pub use governance::{
    get_case_overview_snapshot, get_correlation_snapshot, get_v2_governance_snapshot,
    get_v3_governance_snapshot,
};
pub use linux_cluster::{
    get_linux_evidence_set_summary, list_linux_evidence_sets, run_kubernetes_cluster_analysis,
};
pub use linux_events::get_linux_evidence_events;
pub use queries::{
    get_android_device_info, get_android_package_summary, get_evidence_classification_summary,
    get_kubernetes_cluster_summary, get_system_info, run_android_analysis,
};

#[cfg(test)]
#[path = "../../tests/unit/commands/analysis_commands_test.rs"]
mod tests;
