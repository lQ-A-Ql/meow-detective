use super::super::output_persistence::commit_analysis_control;
use super::*;
use crate::analysis_service::capability::retain_active_plugin_capability;
use crate::plugin_loader::PluginLoadReport;
use chrono::Utc;

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
) -> Result<AnalysisExtractionExecution, AnalysisServiceError> {
    commit_analysis_control(conn, &mut state, staged_generation)?;
    let persistence_elapsed_ms = elapsed_millis(persistence_started);
    let retryable_failure_count = state.retryable_failure_count;
    let plugin_extract_failures = state.take_plugin_failures();
    let dto = state.into_dto(conn, Utc::now().to_rfc3339())?;
    if retryable_failure_count == 0 {
        progress_reporter.complete();
    } else {
        progress_reporter.fail(retryable_failure_count);
    }
    Ok(AnalysisExtractionExecution {
        dto,
        retryable_failure_count,
        discovery_elapsed_ms,
        processing_elapsed_ms,
        persistence_elapsed_ms,
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
