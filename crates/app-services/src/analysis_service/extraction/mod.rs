mod artifact_query;
mod attr_mapping;
pub(crate) mod browser;
mod browser_lsa;
mod browser_preload;
mod candidate_order;
mod candidate_processing;
mod checkpoint_validation;
pub(crate) mod email;
pub(crate) mod evtx;
mod evtx_persistence;
pub(crate) mod linux;
mod linux_sections;
mod observability;
mod output_digest;
mod output_persistence;
mod preparation;
mod progress;
mod reader;
pub(crate) mod registry;
mod registry_preload;
mod runner;
mod scheduler;
mod state;
mod summary;

pub use self::evtx::extract_evtx_candidate;
pub use self::linux::extract_linux_candidate;
pub(crate) use self::progress::ExtractionProgressUpdate;
pub(crate) use self::reader::{encrypted_candidate_warning, CandidateSource};
pub use self::registry::extract_registry_candidate;
pub use self::runner::run_analysis_extraction;
pub use self::runner::{
    run_analysis_extraction_with_cancel, run_analysis_extraction_with_reader_limits,
};
pub(crate) use self::runner::{
    run_analysis_extraction_with_source_and_progress, AnalysisExtractionExecution,
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
