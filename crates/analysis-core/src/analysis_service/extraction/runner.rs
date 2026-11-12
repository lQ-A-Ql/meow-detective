use super::browser_preload::{prepare_browser_preload, BrowserPreloadContext};
use super::candidate_order::order_candidates_for_extraction;
use super::candidate_processing::{
    capability_for_candidate, discovery_categories, encrypted_candidate_warning,
    process_candidates, CandidateProcessingContext, CandidateSource, ExistingCheckpoints,
    PreloadContexts,
};
use super::checkpoint_validation::existing_complete_scan_keys;
use super::linux::{resolve_linux_log_time, LinuxLogTimeContext};
use super::native_output;
use super::output_persistence::resolve_output_data_source_id;
use super::preparation::prepare_registry_preload;
use super::progress::{ExtractionProgressReporter, ExtractionProgressUpdate};
use super::registry_preload::RegistryPreloadContext;
use super::scheduler::acquire_extraction_slot;
use super::state::{
    existing_clean_scan_keys, existing_diagnostic_scan_keys, AnalysisOutputProducer,
    AnalysisOutputReplacement, ExtractionState,
};
use super::{PluginExtractFailure, PluginLoadRecord};
use crate::analysis_service::cancellation::ensure_not_cancelled;
use crate::analysis_service::candidates::{
    discover_plugin_candidates, evidence_candidates_for_categories_with_cancel_and_linux_catalog,
    EvidenceCandidate, LinuxCatalogMetadata,
};
use crate::analysis_service::capability::{
    AnalysisCapability, CandidateReadPolicy, PLUGIN_CAPABILITY_KEY,
};
use crate::analysis_service::error::AnalysisServiceError;
use crate::analysis_service::platforms::analyzer_for;
use crate::plugin_loader::{PluginExtractor, PluginLoadReport, PluginRejection};
use artifacts_core::ArtifactExtractor;
use domain::{DataSourcePlatform, FileEntryId};
use persistence_sqlite::repositories::analysis_scan_repo::AnalysisScanRepo;
use rusqlite::Connection;
use std::io::Read;
use std::sync::atomic::AtomicBool;
use std::time::Instant;
use transport::dto::AnalysisExtractionRunDto;

mod discovery;
mod finalization;

use self::discovery::{
    begin_streamed_output, discover_run_candidates, guarded_candidate_reader, load_checkpoint_scans,
};
use self::finalization::{finalize_run, load_run_plugins};
pub struct AnalysisExtractionExecution {
    pub dto: AnalysisExtractionRunDto,
    pub retryable_failure_count: u64,
    pub discovery_elapsed_ms: u64,
    pub processing_elapsed_ms: u64,
    pub persistence_elapsed_ms: u64,
    pub source_read_count: u64,
    pub source_read_elapsed_ms: u64,
    pub filesystem_read_metrics: evidence_core::FileSystemReadMetrics,
    pub rados_read_metrics: ceph_reconstruction::ceph_reconstruction::RadosProviderReadMetrics,
    /// Plugins that loaded for this run (audit: `plugin.load`).
    pub plugin_loads: Vec<PluginLoadRecord>,
    /// Plugin DLLs refused during this run (audit: `plugin.reject`).
    pub plugin_rejections: Vec<PluginRejection>,
    /// Plugin extraction failures (audit: `plugin.extract_failed`).
    pub plugin_extract_failures: Vec<PluginExtractFailure>,
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

#[allow(clippy::too_many_arguments)]
pub fn run_analysis_extraction_with_source_and_progress<E: std::fmt::Display>(
    conn: &Connection,
    case_id: &str,
    platform: DataSourcePlatform,
    categories: &[&str],
    cancel_token: &AtomicBool,
    browser_password: Option<&str>,
    progress: &mut dyn FnMut(ExtractionProgressUpdate),
    mut file_reader: impl FnMut(&EvidenceCandidate, usize) -> Result<CandidateSource, E>,
) -> Result<AnalysisExtractionExecution, AnalysisServiceError> {
    run_analysis_extraction_with_plugin_loader_with_password(
        conn,
        case_id,
        platform,
        categories,
        cancel_token,
        browser_password,
        progress,
        |candidate, read_limit| {
            file_reader(candidate, read_limit).map_err(|error| error.to_string())
        },
        &crate::plugin_loader::load_all_report,
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
/// Full extraction run with an injectable plugin loader.
#[allow(clippy::too_many_arguments)]
fn run_analysis_extraction_with_plugin_loader(
    conn: &Connection,
    case_id: &str,
    platform: DataSourcePlatform,
    categories: &[&str],
    cancel_token: &AtomicBool,
    progress: &mut dyn FnMut(ExtractionProgressUpdate),
    file_reader: impl FnMut(&EvidenceCandidate, usize) -> Result<CandidateSource, String>,
    plugin_loader: &dyn Fn() -> PluginLoadReport,
) -> Result<AnalysisExtractionExecution, AnalysisServiceError> {
    run_analysis_extraction_with_plugin_loader_with_password(
        conn,
        case_id,
        platform,
        categories,
        cancel_token,
        None,
        progress,
        file_reader,
        plugin_loader,
    )
}
#[allow(clippy::too_many_arguments)]
fn run_analysis_extraction_with_plugin_loader_with_password(
    conn: &Connection,
    _case_id: &str,
    platform: DataSourcePlatform,
    categories: &[&str],
    cancel_token: &AtomicBool,
    browser_password: Option<&str>,
    progress: &mut dyn FnMut(ExtractionProgressUpdate),
    file_reader: impl FnMut(&EvidenceCandidate, usize) -> Result<CandidateSource, String>,
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
    let (mut candidates, linux_catalog) =
        discover_run_candidates(conn, platform, &selected, &plugin_extractors, cancel_token)?;
    register_candidates(&mut progress_reporter, &selected, &candidates);
    progress_reporter.emit_preparing();
    let checkpoint_scans = load_checkpoint_scans(conn)?;
    let checkpoints = checkpoint_scans.existing();
    let mut guarded_file_reader = guarded_candidate_reader(file_reader);
    let preloads = prepare_run_preloads(
        conn,
        platform,
        &candidates,
        &selected,
        &checkpoints,
        cancel_token,
        browser_password,
        &mut guarded_file_reader,
    )?;
    preloads.systemd.annotate(&mut candidates);
    ensure_not_cancelled(cancel_token)?;
    let output = begin_streamed_output(
        conn,
        &candidates,
        &selected,
        &plugin_extractors,
        &checkpoints,
    )?;
    let discovery_elapsed_ms = elapsed_millis(discovery_started);
    progress_reporter.begin_extraction();
    let mut state = ExtractionState::new(&selected);
    if let Some(catalog) = linux_catalog {
        state.record_linux_catalog(catalog);
    }
    record_preload_warnings(&selected, &preloads, &mut state);
    let processing_started = Instant::now();
    let processing_context = CandidateProcessingContext::new(
        conn,
        &selected,
        &checkpoints,
        preloads.as_refs(),
        &plugin_extractors,
        &output,
        cancel_token,
    );
    process_candidates(
        &processing_context,
        candidates,
        &mut guarded_file_reader,
        &mut state,
        &mut progress_reporter,
    )?;
    ensure_not_cancelled(cancel_token)?;
    let processing_elapsed_ms = elapsed_millis(processing_started);
    progress_reporter.begin_persisting();
    let persistence_started = Instant::now();
    output.rebuild_derived_metrics()?;
    let staged_generation = output.seal()?;
    let result = finalize_run(
        conn,
        state,
        &mut progress_reporter,
        staged_generation,
        discovery_elapsed_ms,
        processing_elapsed_ms,
        persistence_started,
        plugin_loads,
        plugin_rejections,
    )?;
    preloads.systemd.persist_markers(conn)?;
    Ok(result)
}

fn candidate_replacements(
    candidates: &[EvidenceCandidate],
    selected: &[AnalysisCapability],
    plugins: &[&dyn ArtifactExtractor],
    checkpoints: &ExistingCheckpoints<'_>,
) -> Vec<AnalysisOutputReplacement> {
    let mut replacements = Vec::new();
    for candidate in candidates {
        let Some(capability) = capability_for_candidate(selected, candidate) else {
            continue;
        };
        if checkpoints.has_candidate(candidate, capability) {
            continue;
        }
        if capability.key == PLUGIN_CAPABILITY_KEY {
            for producer in super::plugin::matching_producer_ids(plugins, candidate) {
                push_replacement(
                    &mut replacements,
                    AnalysisOutputReplacement {
                        source_object_id: candidate.file_id.0.clone(),
                        producer: AnalysisOutputProducer::ExtractorId(producer),
                    },
                );
            }
        } else {
            push_replacement(
                &mut replacements,
                AnalysisOutputReplacement {
                    source_object_id: candidate.file_id.0.clone(),
                    producer: AnalysisOutputProducer::Prefix(capability.producer_prefix()),
                },
            );
        }
    }
    replacements
}

fn push_replacement(
    replacements: &mut Vec<AnalysisOutputReplacement>,
    replacement: AnalysisOutputReplacement,
) {
    if !replacements.contains(&replacement) {
        replacements.push(replacement);
    }
}

/// Owned preload contexts for one extraction run.
struct RunPreloads {
    systemd: super::systemd_preload::SystemdPreload,
    registry: RegistryPreloadContext,
    browser: BrowserPreloadContext,
    linux_log_time: LinuxLogTimeContext,
    timezone_warnings: Vec<String>,
}

impl RunPreloads {
    fn as_refs(&self) -> PreloadContexts<'_> {
        PreloadContexts {
            systemd: Some(&self.systemd),
            registry: &self.registry,
            browser: &self.browser,
            linux_log_time: &self.linux_log_time,
        }
    }
}

/// Build registry/browser preloads and resolve Linux log timezone.
#[allow(clippy::too_many_arguments)]
fn prepare_run_preloads(
    conn: &Connection,
    platform: DataSourcePlatform,
    candidates: &[EvidenceCandidate],
    selected: &[AnalysisCapability],
    checkpoints: &ExistingCheckpoints<'_>,
    cancel_token: &AtomicBool,
    browser_password: Option<&str>,
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
    let browser = prepare_browser_preload(
        conn,
        candidates,
        cancel_token,
        browser_password,
        file_reader,
    )?;
    let systemd = if platform == DataSourcePlatform::Linux {
        super::systemd_preload::prepare(conn, candidates, cancel_token, file_reader)?
    } else {
        super::systemd_preload::SystemdPreload::default()
    };
    let (linux_log_time, timezone_warnings) = if platform == DataSourcePlatform::Linux {
        resolve_linux_log_time(conn, cancel_token, file_reader)
    } else {
        (LinuxLogTimeContext::utc(), Vec::new())
    };
    Ok(RunPreloads {
        systemd,
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
