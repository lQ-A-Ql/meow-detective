mod artifact_query;
mod attr_mapping;
pub(crate) mod browser;
pub(crate) mod email;
pub(crate) mod evtx;
pub(crate) mod linux;
mod linux_sections;
pub(crate) mod macos;
mod observability;
pub(crate) mod registry;
mod registry_preload;
mod runner;
mod summary;

pub use self::evtx::extract_evtx_candidate;
pub use self::linux::extract_linux_candidate;
pub use self::macos::extract_macos_candidate;
pub use self::registry::extract_registry_candidate;
pub use self::runner::run_analysis_extraction;
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
