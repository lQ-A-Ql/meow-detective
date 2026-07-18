mod artifact_query;
mod attr_mapping;
pub(crate) mod browser;
mod candidate_order;
mod checkpoint_validation;
pub(crate) mod email;
pub(crate) mod evtx;
pub(crate) mod linux;
mod linux_sections;
mod observability;
mod output_persistence;
pub(crate) mod registry;
mod registry_preload;
mod runner;
mod state;
mod summary;

pub use self::evtx::extract_evtx_candidate;
pub use self::linux::extract_linux_candidate;
pub use self::registry::extract_registry_candidate;
pub use self::runner::run_analysis_extraction;
pub(crate) use self::runner::{
    run_analysis_extraction_with_bytes_and_cancel, AnalysisExtractionExecution,
};
pub use self::runner::{
    run_analysis_extraction_with_cancel, run_analysis_extraction_with_reader_limits,
};
pub use self::summary::{
    get_browser_history_summary, get_email_extraction_summary, get_evtx_event_summary,
    get_linux_artifact_summary, get_registry_extraction_summary, get_registry_structured_summary,
};
use domain::{Artifact, TimelineEvent};

#[derive(Default)]
pub struct ExtractionOutcome {
    pub artifacts: Vec<Artifact>,
    pub timeline_events: Vec<TimelineEvent>,
    pub warnings: Vec<String>,
}
