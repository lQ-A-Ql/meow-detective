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
use super::state::{existing_clean_scan_keys, existing_diagnostic_scan_keys, ExtractionState};
use super::{PluginExtractFailure, PluginLoadRecord};
use crate::analysis_service::cancellation::ensure_not_cancelled;
use crate::analysis_service::candidates::{
    discover_plugin_candidates, evidence_candidates_for_categories_with_preparation,
    EvidenceCandidate, LinuxCatalogMetadata, PreparationCatalog,
};
use crate::analysis_service::capability::{AnalysisCapability, CandidateReadPolicy};
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
use transport::dto::{AnalysisExtractionRunDto, AnalysisWorkProgressDto, AnalysisWorkStageDto};

mod discovery;
mod empty;
mod finalization;
mod observations;
mod processing;
mod reuse;

use self::discovery::{
    begin_streamed_output, discover_run_candidates, guarded_candidate_reader, load_checkpoint_scans,
};
use self::finalization::load_run_plugins;
use self::observations::{record_preload_warnings, register_candidates};
pub struct AnalysisExtractionExecution {
    pub dto: AnalysisExtractionRunDto,
    pub retryable_failure_count: u64,
    pub discovery_elapsed_ms: u64,
    pub processing_elapsed_ms: u64,
    pub persistence_elapsed_ms: u64,
    pub work_metrics: Vec<AnalysisWorkProgressDto>,
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
    let (candidates, linux_catalog, preparation_catalog) = discover_run_candidates(
        conn,
        platform,
        &selected,
        &plugin_extractors,
        cancel_token,
        &mut progress_reporter,
    )?;
    if candidates.is_empty() && linux_catalog.is_none() && plugin_extractors.is_empty() {
        return empty::complete_empty_run(
            conn,
            &selected,
            &mut progress_reporter,
            discovery_started,
            plugin_loads,
            plugin_rejections,
            cancel_token,
        );
    }
    register_candidates(&mut progress_reporter, &selected, &candidates);
    progress_reporter.emit_preparing();
    extract_discovered_candidates(
        conn,
        platform,
        &selected,
        candidates,
        linux_catalog,
        preparation_catalog,
        cancel_token,
        browser_password,
        &mut progress_reporter,
        file_reader,
        discovery_started,
        &plugin_extractors,
        plugin_loads,
        plugin_rejections,
    )
}

/// Preload, stream, process, and persist a discovery result that has candidates
/// or platform/plugin side effects (Linux catalog or loaded plugins).
#[allow(clippy::too_many_arguments)]
fn extract_discovered_candidates(
    conn: &Connection,
    platform: DataSourcePlatform,
    selected: &[AnalysisCapability],
    mut candidates: Vec<EvidenceCandidate>,
    linux_catalog: Option<LinuxCatalogMetadata>,
    preparation_catalog: PreparationCatalog,
    cancel_token: &AtomicBool,
    browser_password: Option<&str>,
    progress_reporter: &mut ExtractionProgressReporter<'_>,
    file_reader: impl FnMut(&EvidenceCandidate, usize) -> Result<CandidateSource, String>,
    discovery_started: Instant,
    plugin_extractors: &[&dyn ArtifactExtractor],
    plugin_loads: Vec<PluginLoadRecord>,
    plugin_rejections: Vec<PluginRejection>,
) -> Result<AnalysisExtractionExecution, AnalysisServiceError> {
    let checkpoint_scans = load_checkpoint_scans(conn, progress_reporter)?;
    let checkpoints = checkpoint_scans.existing();
    let mut guarded_file_reader = guarded_candidate_reader(file_reader);
    let preloads = prepare_run_preloads(
        conn,
        platform,
        &candidates,
        selected,
        &checkpoints,
        &preparation_catalog,
        progress_reporter,
        cancel_token,
        browser_password,
        &mut guarded_file_reader,
    )?;
    preloads.systemd.annotate(&mut candidates);
    ensure_not_cancelled(cancel_token)?;
    if let Some(execution) = reuse::try_reuse(
        reuse::ReuseRequest {
            conn,
            selected,
            candidates: &candidates,
            linux_catalog: linux_catalog.as_ref(),
            checkpoints: &checkpoints,
            preloads: &preloads,
            plugins: plugin_extractors,
            plugin_loads: &plugin_loads,
            plugin_rejections: &plugin_rejections,
            cancel: cancel_token,
            discovery_started,
        },
        progress_reporter,
    )? {
        return Ok(execution);
    }
    let output = begin_streamed_output(
        conn,
        &candidates,
        selected,
        plugin_extractors,
        &checkpoints,
        cancel_token,
        progress_reporter,
    )?;
    processing::run(
        conn,
        selected,
        candidates,
        linux_catalog,
        &checkpoints,
        &preloads,
        output,
        &mut guarded_file_reader,
        progress_reporter,
        discovery_started,
        plugin_extractors,
        plugin_loads,
        plugin_rejections,
        cancel_token,
    )
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
    catalog: &PreparationCatalog,
    progress: &mut ExtractionProgressReporter<'_>,
    cancel_token: &AtomicBool,
    browser_password: Option<&str>,
    file_reader: &mut impl FnMut(&EvidenceCandidate, usize) -> Result<CandidateSource, String>,
) -> Result<RunPreloads, AnalysisServiceError> {
    let started = Instant::now();
    let registry = prepare_registry_preload(
        conn,
        candidates,
        selected,
        checkpoints,
        cancel_token,
        file_reader,
    )?;
    progress.report_work(
        AnalysisWorkStageDto::RegistryPreload,
        started,
        registry.prepared_records(),
        0,
        true,
    );
    let pending_browser = candidates
        .iter()
        .filter(|candidate| {
            candidate.category == "BrowserHistory"
                && capability_for_candidate(selected, candidate)
                    .is_some_and(|capability| !checkpoints.has_candidate(candidate, capability))
        })
        .cloned()
        .collect::<Vec<_>>();
    let started = Instant::now();
    let browser = prepare_browser_preload(
        catalog,
        &registry,
        &pending_browser,
        cancel_token,
        browser_password,
        file_reader,
    )?;
    progress.report_work(
        AnalysisWorkStageDto::BrowserPreload,
        started,
        pending_browser.len() as u64,
        0,
        true,
    );
    let started = Instant::now();
    let systemd = if platform == DataSourcePlatform::Linux {
        super::systemd_preload::prepare(catalog, candidates, cancel_token, file_reader)?
    } else {
        super::systemd_preload::SystemdPreload::default()
    };
    progress.report_work(
        AnalysisWorkStageDto::SystemdPreload,
        started,
        systemd.identities.len() as u64,
        0,
        true,
    );
    let started = Instant::now();
    let (linux_log_time, timezone_warnings) = if platform == DataSourcePlatform::Linux {
        resolve_linux_log_time(catalog, cancel_token, file_reader)
    } else {
        (LinuxLogTimeContext::utc(), Vec::new())
    };
    progress.report_work(AnalysisWorkStageDto::TimezonePreload, started, 0, 0, true);
    Ok(RunPreloads {
        systemd,
        registry,
        browser,
        linux_log_time,
        timezone_warnings,
    })
}

fn elapsed_millis(started: Instant) -> u64 {
    u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX)
}

#[cfg(test)]
#[path = "../../../tests/unit/analysis_service/extraction/runner.rs"]
mod tests;
