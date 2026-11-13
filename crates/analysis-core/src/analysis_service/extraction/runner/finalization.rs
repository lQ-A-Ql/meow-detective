use super::super::output_persistence::commit_analysis_control;
use super::*;
use crate::analysis_service::capability::{retain_active_plugin_capability, PLUGIN_CAPABILITY_KEY};
use crate::plugin_loader::PluginLoadReport;
use chrono::Utc;

#[allow(clippy::too_many_arguments)]
pub(super) fn persist_processed_output(
    conn: &Connection,
    state: ExtractionState,
    output: native_output::StreamingGeneration,
    progress: &mut ExtractionProgressReporter<'_>,
    discovery_elapsed_ms: u64,
    processing_elapsed_ms: u64,
    plugin_loads: Vec<PluginLoadRecord>,
    plugin_rejections: Vec<PluginRejection>,
    cancel: &AtomicBool,
) -> Result<AnalysisExtractionExecution, AnalysisServiceError> {
    progress.begin_persisting();
    ensure_not_cancelled(cancel)?;
    let persistence_started = Instant::now();
    let rebuilding = Instant::now();
    let mut rebuilt = 0;
    output.rebuild_derived_metrics(cancel, |records| {
        rebuilt = records;
        progress.report_work(
            AnalysisWorkStageDto::CountRebuild,
            rebuilding,
            records,
            0,
            false,
        );
    })?;
    progress.report_work(
        AnalysisWorkStageDto::CountRebuild,
        rebuilding,
        rebuilt,
        0,
        true,
    );
    let sealing = Instant::now();
    let staged = output.seal(cancel, |records, bytes| {
        progress.report_work(
            AnalysisWorkStageDto::GenerationSeal,
            sealing,
            records,
            bytes,
            false,
        );
    })?;
    progress.report_work(AnalysisWorkStageDto::GenerationSeal, sealing, 0, 0, true);
    ensure_not_cancelled(cancel)?;
    finalize_run(
        conn,
        state,
        progress,
        staged,
        discovery_elapsed_ms,
        processing_elapsed_ms,
        persistence_started,
        plugin_loads,
        plugin_rejections,
        cancel,
    )
}

/// Persist pending outputs, build the run DTO, complete the progress
/// reporter, and assemble the execution record.
#[allow(clippy::too_many_arguments)]
pub(super) fn finalize_run(
    conn: &Connection,
    mut state: ExtractionState,
    progress_reporter: &mut ExtractionProgressReporter<'_>,
    staged_generation: native_output::StagedGeneration,
    discovery_elapsed_ms: u64,
    processing_elapsed_ms: u64,
    persistence_started: Instant,
    plugin_loads: Vec<PluginLoadRecord>,
    plugin_rejections: Vec<PluginRejection>,
    cancel_token: &AtomicBool,
) -> Result<AnalysisExtractionExecution, AnalysisServiceError> {
    ensure_not_cancelled(cancel_token)?;
    let publication = Instant::now();
    commit_analysis_control(conn, &mut state, staged_generation)?;
    progress_reporter.report_work(AnalysisWorkStageDto::Publication, publication, 1, 0, true);
    let retryable_failure_count = state.retryable_failure_count;
    let plugin_extract_failures = state.take_plugin_failures();
    let building = Instant::now();
    let dto = state.into_dto(conn, Utc::now().to_rfc3339())?;
    progress_reporter.report_work(
        AnalysisWorkStageDto::ResultBuilding,
        building,
        dto.artifact_count,
        0,
        true,
    );
    if retryable_failure_count == 0 {
        progress_reporter.complete();
    } else {
        progress_reporter.fail(retryable_failure_count);
    }
    let persistence_elapsed_ms = elapsed_millis(persistence_started);
    Ok(AnalysisExtractionExecution {
        dto,
        retryable_failure_count,
        discovery_elapsed_ms,
        processing_elapsed_ms,
        persistence_elapsed_ms,
        work_metrics: progress_reporter.take_work_metrics(),
        source_read_count: 0,
        source_read_elapsed_ms: 0,
        filesystem_read_metrics: evidence_core::FileSystemReadMetrics::default(),
        rados_read_metrics:
            ceph_reconstruction::ceph_reconstruction::RadosProviderReadMetrics::default(),
        plugin_loads,
        plugin_rejections,
        plugin_extract_failures,
    })
}

/// Keep plugins whose declared evidence platform matches the run platform.
fn plugins_for_platform(
    plugins: Vec<PluginExtractor>,
    platform: DataSourcePlatform,
) -> Vec<PluginExtractor> {
    plugins
        .into_iter()
        .filter(|plugin| {
            matches!(
                (plugin.evidence_platform(), platform),
                (
                    plugin_api::MeowEvidencePlatform::Windows,
                    DataSourcePlatform::Windows
                ) | (
                    plugin_api::MeowEvidencePlatform::Linux,
                    DataSourcePlatform::Linux
                )
            )
        })
        .collect()
}

/// Load matching plugins for one run and collect load/reject audit records.
pub(super) fn load_run_plugins(
    platform: DataSourcePlatform,
    selected: &mut Vec<AnalysisCapability>,
    plugin_loader: &dyn Fn() -> PluginLoadReport,
) -> (
    Vec<PluginExtractor>,
    Vec<PluginLoadRecord>,
    Vec<PluginRejection>,
) {
    let plugin_report = if selected
        .iter()
        .any(|capability| capability.key == PLUGIN_CAPABILITY_KEY)
    {
        plugin_loader()
    } else {
        PluginLoadReport::default()
    };
    let plugins = plugins_for_platform(plugin_report.plugins, platform);
    retain_active_plugin_capability(selected, !plugins.is_empty());
    let plugin_loads = plugins
        .iter()
        .map(|plugin| PluginLoadRecord {
            plugin_id: plugin.id().to_string(),
            plugin_version: plugin.plugin_version().to_string(),
        })
        .collect::<Vec<_>>();
    (plugins, plugin_loads, plugin_report.rejections)
}
