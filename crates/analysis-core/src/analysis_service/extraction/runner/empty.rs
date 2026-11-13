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
use std::sync::atomic::AtomicBool;
use std::time::Instant;

/// Complete a run without candidates or pending outputs.
pub(super) fn complete_empty_run(
    conn: &Connection,
    selected: &[AnalysisCapability],
    progress_reporter: &mut ExtractionProgressReporter<'_>,
    discovery_started: Instant,
    plugin_loads: Vec<PluginLoadRecord>,
    plugin_rejections: Vec<PluginRejection>,
    cancel_token: &AtomicBool,
) -> Result<AnalysisExtractionExecution, AnalysisServiceError> {
    progress_reporter.emit_preparing();
    progress_reporter.begin_extraction();
    progress_reporter.begin_persisting();
    crate::analysis_service::cancellation::ensure_not_cancelled(cancel_token)?;
    let persistence_started = Instant::now();
    let dto = ExtractionState::new(selected).into_dto(conn, Utc::now().to_rfc3339())?;
    progress_reporter.report_work(
        super::AnalysisWorkStageDto::ResultBuilding,
        persistence_started,
        dto.artifact_count,
        0,
        true,
    );
    progress_reporter.complete();
    Ok(AnalysisExtractionExecution {
        dto,
        retryable_failure_count: 0,
        discovery_elapsed_ms: super::elapsed_millis(discovery_started),
        processing_elapsed_ms: 0,
        persistence_elapsed_ms: super::elapsed_millis(persistence_started),
        work_metrics: progress_reporter.take_work_metrics(),
        source_read_count: 0,
        source_read_elapsed_ms: 0,
        filesystem_read_metrics: evidence_core::FileSystemReadMetrics::default(),
        rados_read_metrics:
            ceph_reconstruction::ceph_reconstruction::RadosProviderReadMetrics::default(),
        plugin_loads,
        plugin_rejections,
        plugin_extract_failures: Vec::new(),
    })
}
