use super::browser_preload::{prepare_browser_preload, BrowserPreloadContext};
use super::candidate_order::order_candidates_for_extraction;
use super::candidate_processing::{
    capability_for_candidate, discovery_categories, encrypted_candidate_warning,
    process_candidates, CandidateProcessingContext, CandidateSource, ExistingCheckpoints,
    PreloadContexts,
};
use super::checkpoint_validation::existing_complete_scan_keys;
use super::linux::{resolve_linux_log_time, LinuxLogTimeContext};
use super::output_persistence::flush_pending_outputs;
use super::preparation::prepare_registry_preload;
use super::progress::{ExtractionProgressReporter, ExtractionProgressUpdate};
use super::registry_preload::RegistryPreloadContext;
use super::scheduler::acquire_extraction_slot;
use super::state::{existing_clean_scan_keys, existing_diagnostic_scan_keys, ExtractionState};
use super::{PluginExtractFailure, PluginLoadRecord};
use crate::analysis_service::cancellation::ensure_not_cancelled;
use crate::analysis_service::candidates::{
    discover_plugin_candidates, evidence_candidates_for_categories_with_cancel, EvidenceCandidate,
};
use crate::analysis_service::capability::{
    retain_active_plugin_capability, AnalysisCapability, CandidateReadPolicy, PLUGIN_CAPABILITY_KEY,
};
use crate::analysis_service::error::AnalysisServiceError;
use crate::analysis_service::platforms::analyzer_for;
use crate::plugin_loader::{PluginExtractor, PluginLoadReport, PluginRejection};
use artifacts_core::ArtifactExtractor;
use chrono::Utc;
use domain::{DataSourcePlatform, FileEntryId};
use persistence_sqlite::repositories::analysis_scan_repo::AnalysisScanRepo;
use rusqlite::Connection;
use std::io::Read;
use std::sync::atomic::AtomicBool;
use std::time::Instant;
use transport::dto::AnalysisExtractionRunDto;

pub(crate) struct AnalysisExtractionExecution {
    pub(crate) dto: AnalysisExtractionRunDto,
    pub(crate) retryable_failure_count: u64,
    pub(crate) discovery_elapsed_ms: u64,
    pub(crate) processing_elapsed_ms: u64,
    pub(crate) persistence_elapsed_ms: u64,
    pub(crate) source_read_count: u64,
    pub(crate) source_read_elapsed_ms: u64,
    pub(crate) filesystem_read_metrics: evidence_core::FileSystemReadMetrics,
    pub(crate) rados_read_metrics: crate::ceph_reconstruction::RadosProviderReadMetrics,
    /// Plugins that loaded for this run (audit: `plugin.load`).
    pub(crate) plugin_loads: Vec<PluginLoadRecord>,
    /// Plugin DLLs refused during this run (audit: `plugin.reject`).
    pub(crate) plugin_rejections: Vec<PluginRejection>,
    /// Plugin extraction failures (audit: `plugin.extract_failed`).
    pub(crate) plugin_extract_failures: Vec<PluginExtractFailure>,
}

pub fn run_analysis_extraction<E: std::fmt::Display>(
    conn: &Connection,
    case_id: &str,
    platform: DataSourcePlatform,
    categories: &[&str],
    mut file_reader: impl FnMut(&FileEntryId) -> Result<Box<dyn Read>, E>,
) -> Result<AnalysisExtractionRunDto, AnalysisServiceError> {
    let cancel_token = AtomicBool::new(false);
    run_analysis_extraction_with_cancel(
        conn,
        case_id,
        platform,
        categories,
        &cancel_token,
        |file_id, _read_limit| file_reader(file_id),
    )
}

pub fn run_analysis_extraction_with_reader_limits<E: std::fmt::Display>(
    conn: &Connection,
    case_id: &str,
    platform: DataSourcePlatform,
    categories: &[&str],
    file_reader: impl FnMut(&FileEntryId, usize) -> Result<Box<dyn Read>, E>,
) -> Result<AnalysisExtractionRunDto, AnalysisServiceError> {
    let cancel_token = AtomicBool::new(false);
    run_analysis_extraction_with_cancel(
        conn,
        case_id,
        platform,
        categories,
        &cancel_token,
        file_reader,
    )
}

pub fn run_analysis_extraction_with_cancel<E: std::fmt::Display>(
    conn: &Connection,
    case_id: &str,
    platform: DataSourcePlatform,
    categories: &[&str],
    cancel_token: &AtomicBool,
    mut file_reader: impl FnMut(&FileEntryId, usize) -> Result<Box<dyn Read>, E>,
) -> Result<AnalysisExtractionRunDto, AnalysisServiceError> {
    let mut ignore_progress = |_update: ExtractionProgressUpdate| {};
    run_analysis_extraction_with_source(
        conn,
        case_id,
        platform,
        categories,
        cancel_token,
        &mut ignore_progress,
        |candidate, read_limit| {
            file_reader(&candidate.file_id, read_limit)
                .map(CandidateSource::Reader)
                .map_err(|error| error.to_string())
        },
    )
    .map(|execution| execution.dto)
}

pub(crate) fn run_analysis_extraction_with_source_and_progress<E: std::fmt::Display>(
    conn: &Connection,
    case_id: &str,
    platform: DataSourcePlatform,
    categories: &[&str],
    cancel_token: &AtomicBool,
    progress: &mut dyn FnMut(ExtractionProgressUpdate),
    mut file_reader: impl FnMut(&EvidenceCandidate, usize) -> Result<CandidateSource, E>,
) -> Result<AnalysisExtractionExecution, AnalysisServiceError> {
    run_analysis_extraction_with_source(
        conn,
        case_id,
        platform,
        categories,
        cancel_token,
        progress,
        |candidate, read_limit| {
            file_reader(candidate, read_limit).map_err(|error| error.to_string())
        },
    )
}

fn run_analysis_extraction_with_source(
    conn: &Connection,
    case_id: &str,
    platform: DataSourcePlatform,
    categories: &[&str],
    cancel_token: &AtomicBool,
    progress: &mut dyn FnMut(ExtractionProgressUpdate),
    file_reader: impl FnMut(&EvidenceCandidate, usize) -> Result<CandidateSource, String>,
) -> Result<AnalysisExtractionExecution, AnalysisServiceError> {
    run_analysis_extraction_with_plugin_loader(
        conn,
        case_id,
        platform,
        categories,
        cancel_token,
        progress,
        file_reader,
        &crate::plugin_loader::load_all_report,
    )
}

/// Full extraction run with an injectable plugin loader. The loader is only
/// invoked when the `PluginArtifacts` capability is selected, so runs that
/// stay on built-in capabilities never touch plugin discovery; an empty or
/// disabled plugin set drops the capability before any candidate work.
#[allow(clippy::too_many_arguments)]
fn run_analysis_extraction_with_plugin_loader(
    conn: &Connection,
    case_id: &str,
    platform: DataSourcePlatform,
    categories: &[&str],
    cancel_token: &AtomicBool,
    progress: &mut dyn FnMut(ExtractionProgressUpdate),
    mut file_reader: impl FnMut(&EvidenceCandidate, usize) -> Result<CandidateSource, String>,
    plugin_loader: &dyn Fn() -> PluginLoadReport,
) -> Result<AnalysisExtractionExecution, AnalysisServiceError> {
    let discovery_started = Instant::now();
    ensure_not_cancelled(cancel_token)?;
    let mut selected = analyzer_for(platform)?.select_capabilities(categories)?;
    let (plugins, plugin_loads, plugin_rejections) =
        load_run_plugins(platform, &mut selected, plugin_loader);
    let plugin_extractors = plugins
        .iter()
        .map(|plugin| plugin as &dyn ArtifactExtractor)
        .collect::<Vec<_>>();
    let mut progress_reporter = ExtractionProgressReporter::new(platform, &selected, progress);
    let _extraction_slot = acquire_extraction_slot(cancel_token, |waited| {
        progress_reporter.emit_waiting_for_scheduler(waited);
    })?;
    progress_reporter.emit_discovering();
    let discovery_categories = discovery_categories(&selected);
    let mut candidates =
        evidence_candidates_for_categories_with_cancel(conn, &discovery_categories, cancel_token)?;
    candidates.append(&mut discover_plugin_candidates(
        conn,
        &plugin_extractors,
        cancel_token,
    )?);
    order_candidates_for_extraction(conn, platform, &mut candidates);
    register_candidates(&mut progress_reporter, &selected, &candidates);
    progress_reporter.emit_preparing();
    let existing_clean_scans = existing_clean_scan_keys(conn)?;
    let existing_diagnostic_scans = existing_diagnostic_scan_keys(conn)?;
    let existing_complete_scans = existing_complete_scan_keys(conn)?;
    let checkpoint_storage_available = AnalysisScanRepo::new(conn).storage_available()?;
    let checkpoints = ExistingCheckpoints {
        clean: &existing_clean_scans,
        diagnostic: &existing_diagnostic_scans,
        complete: &existing_complete_scans,
        storage_available: checkpoint_storage_available,
    };
    let mut guarded_file_reader = |candidate: &EvidenceCandidate, read_limit: usize| {
        if let Some(warning) = encrypted_candidate_warning(candidate) {
            return Err(warning);
        }
        file_reader(candidate, read_limit)
    };
    let preloads = prepare_run_preloads(
        conn,
        platform,
        &candidates,
        &selected,
        &checkpoints,
        cancel_token,
        &mut guarded_file_reader,
    )?;
    ensure_not_cancelled(cancel_token)?;
    progress_reporter.begin_extraction();
    let discovery_elapsed_ms = elapsed_millis(discovery_started);
    let mut state = ExtractionState::new(&selected);
    record_preload_warnings(&selected, &preloads, &mut state);
    let processing_started = Instant::now();
    let processing_context = CandidateProcessingContext::new(
        conn,
        case_id,
        &selected,
        &checkpoints,
        preloads.as_refs(),
        &plugin_extractors,
        cancel_token,
    );
    process_candidates(
        &processing_context,
        candidates,
        &mut guarded_file_reader,
        &mut state,
        &mut progress_reporter,
    )?;
    let processing_elapsed_ms = elapsed_millis(processing_started);
    ensure_not_cancelled(cancel_token)?;
    progress_reporter.begin_persisting();
    finalize_run(
        conn,
        case_id,
        state,
        &mut progress_reporter,
        discovery_elapsed_ms,
        processing_elapsed_ms,
        plugin_loads,
        plugin_rejections,
    )
}

/// Persist pending outputs, build the run DTO, complete the progress
/// reporter, and assemble the execution record.
#[allow(clippy::too_many_arguments)]
fn finalize_run(
    conn: &Connection,
    case_id: &str,
    mut state: ExtractionState,
    progress_reporter: &mut ExtractionProgressReporter<'_>,
    discovery_elapsed_ms: u64,
    processing_elapsed_ms: u64,
    plugin_loads: Vec<PluginLoadRecord>,
    plugin_rejections: Vec<PluginRejection>,
) -> Result<AnalysisExtractionExecution, AnalysisServiceError> {
    let persistence_elapsed_ms = flush_pending_outputs(conn, case_id, &mut state)?;
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
        rados_read_metrics: crate::ceph_reconstruction::RadosProviderReadMetrics::default(),
        plugin_loads,
        plugin_rejections,
        plugin_extract_failures,
    })
}

/// Keep only plugins whose declared evidence platform matches the run
/// platform (design doc §4.1: Windows modules appear under Windows sources).
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

/// Load the plugin set for one run (only when the `PluginArtifacts`
/// capability was selected), drop the capability when no platform-matching
/// plugin loaded, and collect the load/reject audit records.
fn load_run_plugins(
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

/// Owned preload contexts for one extraction run.
struct RunPreloads {
    registry: RegistryPreloadContext,
    browser: BrowserPreloadContext,
    linux_log_time: LinuxLogTimeContext,
    timezone_warnings: Vec<String>,
}

impl RunPreloads {
    fn as_refs(&self) -> PreloadContexts<'_> {
        PreloadContexts {
            registry: &self.registry,
            browser: &self.browser,
            linux_log_time: &self.linux_log_time,
        }
    }
}

/// Build the registry/browser preload contexts and resolve the host timezone
/// for Linux log parsing (non-Linux runs use UTC).
fn prepare_run_preloads(
    conn: &Connection,
    platform: DataSourcePlatform,
    candidates: &[EvidenceCandidate],
    selected: &[AnalysisCapability],
    checkpoints: &ExistingCheckpoints<'_>,
    cancel_token: &AtomicBool,
    file_reader: &mut impl FnMut(&EvidenceCandidate, usize) -> Result<CandidateSource, String>,
) -> Result<RunPreloads, AnalysisServiceError> {
    let registry = prepare_registry_preload(
        conn,
        candidates,
        selected,
        checkpoints,
        cancel_token,
        file_reader,
    )?;
    let browser = prepare_browser_preload(conn, candidates, cancel_token, file_reader)?;
    let (linux_log_time, timezone_warnings) = if platform == DataSourcePlatform::Linux {
        resolve_linux_log_time(conn, cancel_token, file_reader)
    } else {
        (LinuxLogTimeContext::utc(), Vec::new())
    };
    Ok(RunPreloads {
        registry,
        browser,
        linux_log_time,
        timezone_warnings,
    })
}

/// Record timezone resolution warnings as informational (non-retryable)
/// warnings against any selected Linux capability.
fn record_log_time_warnings(
    selected: &[AnalysisCapability],
    warnings: &[String],
    state: &mut ExtractionState,
) {
    let Some(capability) = selected
        .iter()
        .find(|capability| capability.read_policy == CandidateReadPolicy::LinuxPathAware)
        .copied()
    else {
        return;
    };
    for warning in warnings {
        state.record_informational_warning(capability, warning.clone());
    }
}

fn record_preload_warnings(
    selected: &[AnalysisCapability],
    preloads: &RunPreloads,
    state: &mut ExtractionState,
) {
    if let Some(registry) = selected
        .iter()
        .find(|capability| capability.read_policy == CandidateReadPolicy::RegistryPreload)
    {
        for warning in preloads.registry.warnings.iter().cloned() {
            state.record_warning(*registry, warning);
        }
    }
    if let Some(browser) = selected
        .iter()
        .find(|capability| capability.key == "BrowserHistory")
    {
        for warning in preloads.browser.warnings.iter().cloned() {
            state.record_warning(*browser, warning);
        }
    }
    record_log_time_warnings(selected, &preloads.timezone_warnings, state);
}

fn register_candidates(
    progress: &mut ExtractionProgressReporter<'_>,
    selected: &[AnalysisCapability],
    candidates: &[EvidenceCandidate],
) {
    for candidate in candidates {
        if let Some(capability) = capability_for_candidate(selected, candidate) {
            progress.register_candidate(capability, candidate);
        }
    }
}

fn elapsed_millis(started: Instant) -> u64 {
    u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX)
}

#[cfg(test)]
#[path = "../../../tests/unit/analysis_service/extraction/runner.rs"]
mod tests;
