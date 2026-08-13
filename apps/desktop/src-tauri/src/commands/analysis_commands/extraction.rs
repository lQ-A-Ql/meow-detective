//! Analysis extraction command facade.

mod plugin_modules;
mod report;
mod runs;
mod summaries;

pub use plugin_modules::{get_plugin_family_entries, list_plugin_modules};
pub use report::generate_analysis_summary;
pub use runs::{run_analysis_extraction, run_evidence_classification};
pub use summaries::{
    get_browser_history_summary, get_email_extraction_summary, get_evtx_event_summary,
    get_linux_artifact_summary, get_registry_extraction_summary, get_registry_structured_summary,
};
