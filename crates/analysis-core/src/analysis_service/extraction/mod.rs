mod artifact_query;
mod attr_mapping;
pub(crate) mod browser;
mod browser_catalog;
mod browser_lsa;
mod browser_preload;
mod candidate_order;
mod candidate_processing;
mod checkpoint_validation;
mod containerd_images;
pub(crate) mod email;
pub(crate) mod evtx;
pub(crate) mod linux;
mod linux_sections;
mod native_output;
mod observability;
mod output_digest;
mod output_persistence;
mod plugin;
mod preparation;
mod progress;
mod reader;
pub(crate) mod registry;
mod registry_preload;
mod runner;
mod scheduler;
mod state;
mod summary;
mod systemd_preload;

pub use self::evtx::extract_evtx_candidate;
pub use self::linux::extract_linux_candidate;
pub use self::native_output::StagedGeneration as StagedSourceGeneration;
pub(crate) use self::progress::ExtractionProgressUpdate;
pub(crate) use self::reader::{encrypted_candidate_warning, CandidateSource};
pub use self::registry::extract_registry_candidate;
pub use self::runner::run_analysis_extraction;
pub use self::runner::{
    run_analysis_extraction_with_cancel, run_analysis_extraction_with_reader_limits,
};
pub use self::runner::{
    run_analysis_extraction_with_source_and_progress, AnalysisExtractionExecution,
};
pub use self::summary::warm_linux_artifact_summary;
pub use self::summary::{
    get_browser_history_summary, get_email_extraction_summary, get_evtx_event_summary,
    get_linux_artifact_summary, get_linux_system_config_page, get_plugin_family_entries,
    get_registry_extraction_summary, get_registry_structured_summary, list_plugin_modules,
};
use super::error::AnalysisServiceError;
use domain::{Artifact, TimelineEvent};

pub fn stage_native_records(
    conn: &rusqlite::Connection,
    case_root: &std::path::Path,
    source_id: &str,
    artifacts: &[Artifact],
    events: &[TimelineEvent],
) -> Result<StagedSourceGeneration, AnalysisServiceError> {
    native_output::stage(conn, case_root, source_id, artifacts, events, &[])
}

#[derive(Default)]
pub struct ExtractionOutcome {
    pub artifacts: Vec<Artifact>,
    pub timeline_events: Vec<TimelineEvent>,
    pub warnings: Vec<String>,
}

/// One plugin `meow_plugin_extract` failure during an analysis run. Surfaced
/// as a run warning and audited by the use-case layer (`plugin.extract_failed`).
#[derive(Debug, Clone)]
pub struct PluginExtractFailure {
    pub(crate) plugin_id: String,
    pub(crate) source_path: String,
    pub(crate) error: String,
}

/// One plugin DLL that passed the ABI handshake during an analysis run.
#[derive(Debug, Clone)]
pub struct PluginLoadRecord {
    pub(crate) plugin_id: String,
    pub(crate) plugin_version: String,
}
