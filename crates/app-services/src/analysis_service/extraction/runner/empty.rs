//! Completion path for analysis runs that discovered nothing to extract.
use super::super::progress::ExtractionProgressReporter;
use super::super::state::ExtractionState;
use super::super::PluginLoadRecord;
use super::AnalysisExtractionExecution;
use crate::analysis_service::capability::AnalysisCapability;
use crate::analysis_service::error::AnalysisServiceError;
use crate::plugin_loader::PluginRejection;
use chrono::Utc;
use rusqlite::Connection;
use std::time::Instant;

/// Finish a run with no candidates, no Linux catalog, and no plugins without
/// cloning the published source store or binding a new generation: there is
/// nothing to replace, checkpoint, or seal. The DTO is assembled exactly like
/// the normal path through `ExtractionState::into_dto`.
pub(super) fn complete_empty_run(
    conn: &Connection,
    selected: &[AnalysisCapability],
    progress_reporter: &mut ExtractionProgressReporter<'_>,
    discovery_started: Instant,
    plugin_loads: Vec<PluginLoadRecord>,
    plugin_rejections: Vec<PluginRejection>,
) -> Result<AnalysisExtractionExecution, AnalysisServiceError> {
    progress_reporter.emit_preparing();
    progress_reporter.begin_extraction();
    progress_reporter.begin_persisting();
    progress_reporter.complete();
    let dto = ExtractionState::new(selected).into_dto(conn, Utc::now().to_rfc3339())?;
    Ok(AnalysisExtractionExecution {
        dto,
        retryable_failure_count: 0,
        discovery_elapsed_ms: super::elapsed_millis(discovery_started),
        processing_elapsed_ms: 0,
        persistence_elapsed_ms: 0,
        source_read_count: 0,
        source_read_elapsed_ms: 0,
        filesystem_read_metrics: evidence_core::FileSystemReadMetrics::default(),
        rados_read_metrics: crate::ceph_reconstruction::RadosProviderReadMetrics::default(),
        plugin_loads,
        plugin_rejections,
        plugin_extract_failures: Vec::new(),
    })
}
