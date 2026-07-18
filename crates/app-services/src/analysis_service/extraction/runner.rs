use super::artifact_query::already_has_v1_artifacts;
use super::browser::extract_browser_candidate;
use super::candidate_order::order_candidates_for_extraction;
use super::checkpoint_validation::existing_complete_scan_keys;
use super::email::extract_email_candidate;
use super::evtx::extract_evtx_candidate;
use super::linux::{extract_linux_candidate, linux_candidate_read_limit};
use super::linux_sections::{linux_artifact_section, LinuxArtifactSection};
use super::output_persistence::flush_pending_outputs;
use super::registry::extract_registry_candidate;
use super::registry_preload::{preload_registry_context, RegistryPreloadContext};
use super::state::{
    existing_clean_scan_keys, existing_diagnostic_scan_keys, AnalysisCheckpointKey, CleanScanKeys,
    DiagnosticScanKeys, ExtractionState,
};
use super::ExtractionOutcome;
use crate::analysis_service::cancellation::ensure_not_cancelled;
use crate::analysis_service::candidates::{
    evidence_candidates_for_categories_with_cancel, normalize_evidence_path, EvidenceCandidate,
};
use crate::analysis_service::capability::{
    AnalysisCapability, CandidateReadPolicy, LINUX_UMBRELLA_KEY,
};
use crate::analysis_service::error::AnalysisServiceError;
use crate::analysis_service::platforms::analyzer_for;
use chrono::Utc;
use domain::{DataSourcePlatform, FileEntryId};
use persistence_sqlite::repositories::analysis_scan_repo::AnalysisScanRepo;
use rusqlite::Connection;
use std::collections::HashMap;
use std::io::{Cursor, Read};
use std::sync::atomic::AtomicBool;
use std::time::Instant;
use transport::dto::AnalysisExtractionRunDto;

const READ_BUFFER_BYTES: usize = 64 * 1024;

#[derive(Debug)]
enum CandidateExtractionError {
    Warning(String),
    Cancelled,
}

enum CandidateSource {
    Reader(Box<dyn Read>),
    Bytes(Vec<u8>),
}

struct ExistingCheckpoints<'a> {
    clean: &'a CleanScanKeys,
    diagnostic: &'a DiagnosticScanKeys,
    complete: &'a HashMap<
        AnalysisCheckpointKey,
        persistence_sqlite::repositories::analysis_scan_repo::CompleteAnalysisCandidateScan,
    >,
    storage_available: bool,
}

struct CandidateProcessingContext<'a> {
    conn: &'a Connection,
    selected: &'a [AnalysisCapability],
    checkpoints: &'a ExistingCheckpoints<'a>,
    preload: &'a RegistryPreloadContext,
    cancel_token: &'a AtomicBool,
}

impl ExistingCheckpoints<'_> {
    fn has_candidate(&self, candidate: &EvidenceCandidate, capability: AnalysisCapability) -> bool {
        let key = (
            candidate.file_id.0.clone(),
            capability.key.to_string(),
            candidate.size,
            candidate.content_identity.clone(),
        );
        self.clean.contains(&key)
            || self.diagnostic.contains_key(&key)
            || self.complete.contains_key(&key)
    }
}

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
    run_analysis_extraction_with_source(
        conn,
        case_id,
        platform,
        categories,
        cancel_token,
        |candidate, read_limit| {
            file_reader(&candidate.file_id, read_limit)
                .map(CandidateSource::Reader)
                .map_err(|error| error.to_string())
        },
    )
    .map(|execution| execution.dto)
}

pub(crate) fn run_analysis_extraction_with_bytes_and_cancel<E: std::fmt::Display>(
    conn: &Connection,
    case_id: &str,
    platform: DataSourcePlatform,
    categories: &[&str],
    cancel_token: &AtomicBool,
    mut file_reader: impl FnMut(&EvidenceCandidate, usize) -> Result<Vec<u8>, E>,
) -> Result<AnalysisExtractionExecution, AnalysisServiceError> {
    run_analysis_extraction_with_source(
        conn,
        case_id,
        platform,
        categories,
        cancel_token,
        |candidate, read_limit| {
            file_reader(candidate, read_limit)
                .map(CandidateSource::Bytes)
                .map_err(|error| error.to_string())
        },
    )
}

fn run_analysis_extraction_with_source(
    conn: &Connection,
    case_id: &str,
    platform: DataSourcePlatform,
    categories: &[&str],
    cancel_token: &AtomicBool,
    mut file_reader: impl FnMut(&EvidenceCandidate, usize) -> Result<CandidateSource, String>,
) -> Result<AnalysisExtractionExecution, AnalysisServiceError> {
    let discovery_started = Instant::now();
    ensure_not_cancelled(cancel_token)?;
    let selected = analyzer_for(platform)?.select_capabilities(categories)?;
    let discovery_categories = discovery_categories(&selected);
    let mut candidates =
        evidence_candidates_for_categories_with_cancel(conn, &discovery_categories, cancel_token)?;
    order_candidates_for_extraction(conn, platform, &mut candidates);
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
    let preload = prepare_registry_preload(
        conn,
        &candidates,
        &selected,
        &checkpoints,
        cancel_token,
        &mut file_reader,
    )?;
    ensure_not_cancelled(cancel_token)?;
    let discovery_elapsed_ms = elapsed_millis(discovery_started);
    let mut state = ExtractionState::new(&selected);
    if let Some(registry) = selected
        .iter()
        .find(|capability| capability.read_policy == CandidateReadPolicy::RegistryPreload)
    {
        for warning in preload.warnings.iter().cloned() {
            state.record_warning(*registry, warning);
        }
    }
    let processing_started = Instant::now();
    let processing_context = CandidateProcessingContext {
        conn,
        selected: &selected,
        checkpoints: &checkpoints,
        preload: &preload,
        cancel_token,
    };
    process_candidates(
        &processing_context,
        candidates,
        &mut file_reader,
        &mut state,
    )?;
    let processing_elapsed_ms = elapsed_millis(processing_started);
    ensure_not_cancelled(cancel_token)?;
    let persistence_elapsed_ms = flush_pending_outputs(conn, case_id, &mut state)?;
    let retryable_failure_count = state.retryable_failure_count;
    let dto = build_run_dto(conn, state)?;
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
    })
}

fn prepare_registry_preload(
    conn: &Connection,
    candidates: &[EvidenceCandidate],
    selected: &[AnalysisCapability],
    checkpoints: &ExistingCheckpoints<'_>,
    cancel_token: &AtomicBool,
    file_reader: &mut impl FnMut(&EvidenceCandidate, usize) -> Result<CandidateSource, String>,
) -> Result<RegistryPreloadContext, AnalysisServiceError> {
    let candidates_by_id = candidates
        .iter()
        .map(|candidate| (candidate.file_id.0.as_str(), candidate))
        .collect::<HashMap<_, _>>();
    let mut registry_reader = |file_id: &FileEntryId, read_limit: usize| {
        let candidate = candidates_by_id
            .get(file_id.0.as_str())
            .ok_or_else(|| format!("analysis candidate '{}' was not discovered", file_id.0))?;
        file_reader(candidate, read_limit).map(|source| match source {
            CandidateSource::Reader(reader) => reader,
            CandidateSource::Bytes(bytes) => Box::new(Cursor::new(bytes)) as Box<dyn Read>,
        })
    };
    preload_registry_context(
        conn,
        candidates,
        cancel_token,
        &mut registry_reader,
        |candidate| {
            let checkpoint_exists = capability_for_candidate(selected, candidate)
                .is_some_and(|capability| checkpoints.has_candidate(candidate, capability));
            if checkpoint_exists {
                Ok(true)
            } else if checkpoints.storage_available {
                Ok(false)
            } else {
                already_has_v1_artifacts(conn, candidate)
            }
        },
    )
}

fn process_candidates(
    context: &CandidateProcessingContext<'_>,
    candidates: Vec<EvidenceCandidate>,
    file_reader: &mut impl FnMut(&EvidenceCandidate, usize) -> Result<CandidateSource, String>,
    state: &mut ExtractionState,
) -> Result<(), AnalysisServiceError> {
    for candidate in candidates {
        ensure_not_cancelled(context.cancel_token)?;
        let Some(capability) = capability_for_candidate(context.selected, &candidate) else {
            continue;
        };
        let checkpoint_key = (
            candidate.file_id.0.clone(),
            capability.key.to_string(),
            candidate.size,
            candidate.content_identity.clone(),
        );
        if let Some(warnings) = context.checkpoints.diagnostic.get(&checkpoint_key) {
            state.replay_diagnostic(capability, warnings);
            continue;
        }
        if context.checkpoints.clean.contains(&checkpoint_key) {
            state.replay_clean(capability);
            continue;
        }
        if let Some(scan) = context.checkpoints.complete.get(&checkpoint_key) {
            state.replay_complete(capability, scan);
            continue;
        }
        if !context.checkpoints.storage_available
            && already_has_v1_artifacts(context.conn, &candidate)?
        {
            continue;
        }
        match extract_candidate(
            &candidate,
            capability,
            context.preload,
            context.cancel_token,
            file_reader,
        ) {
            Ok(outcome) => state.record_outcome(capability, &candidate, outcome),
            Err(CandidateExtractionError::Warning(warning)) => {
                state.record_warning(capability, warning);
            }
            Err(CandidateExtractionError::Cancelled) => {
                return ensure_not_cancelled(context.cancel_token);
            }
        }
        ensure_not_cancelled(context.cancel_token)?;
    }
    Ok(())
}

fn extract_candidate(
    candidate: &EvidenceCandidate,
    capability: AnalysisCapability,
    preload: &RegistryPreloadContext,
    cancel_token: &AtomicBool,
    file_reader: &mut impl FnMut(&EvidenceCandidate, usize) -> Result<CandidateSource, String>,
) -> Result<ExtractionOutcome, CandidateExtractionError> {
    check_candidate_cancelled(cancel_token)?;
    if capability.read_policy == CandidateReadPolicy::RegistryPreload {
        let bytes = preload.registry_bytes(candidate).ok_or_else(|| {
            CandidateExtractionError::Warning(format!(
                "{} registry bytes not preloaded",
                candidate.path
            ))
        })?;
        let boot_key = preload.boot_key(candidate);
        let (txlog1, txlog2) = preload.txlogs(candidate);
        let outcome = extract_registry_candidate(candidate, bytes, boot_key, txlog1, txlog2);
        check_candidate_cancelled(cancel_token)?;
        return Ok(outcome);
    }

    let normalized = normalize_evidence_path(&candidate.path);
    let read_limit = match capability.read_policy {
        CandidateReadPolicy::Bounded(limit) => limit,
        CandidateReadPolicy::LinuxPathAware => linux_candidate_read_limit(&normalized),
        CandidateReadPolicy::RegistryPreload => {
            return Err(CandidateExtractionError::Warning(format!(
                "{} has an invalid registry read policy",
                candidate.path
            )));
        }
    };
    let bytes = read_candidate_bytes(candidate, read_limit, cancel_token, file_reader)?;
    let outcome = match candidate.category.as_str() {
        "BrowserHistory" => extract_browser_candidate(candidate, &bytes),
        "Email" => extract_email_candidate(candidate, &bytes),
        "EventLogs" => extract_evtx_candidate(candidate, &bytes),
        LINUX_UMBRELLA_KEY => extract_linux_candidate(candidate, &bytes),
        _ => ExtractionOutcome::default(),
    };
    check_candidate_cancelled(cancel_token)?;
    Ok(outcome)
}

fn read_candidate_bytes(
    candidate: &EvidenceCandidate,
    read_limit: usize,
    cancel_token: &AtomicBool,
    file_reader: &mut impl FnMut(&EvidenceCandidate, usize) -> Result<CandidateSource, String>,
) -> Result<Vec<u8>, CandidateExtractionError> {
    check_candidate_cancelled(cancel_token)?;
    let source = file_reader(candidate, read_limit).map_err(|error| {
        CandidateExtractionError::Warning(format!("{} read failed: {error}", candidate.path))
    })?;
    check_candidate_cancelled(cancel_token)?;
    let mut reader = match source {
        CandidateSource::Reader(reader) => reader,
        CandidateSource::Bytes(mut bytes) => {
            bytes.truncate(read_limit);
            return Ok(bytes);
        }
    };
    let mut bytes = Vec::with_capacity(read_limit.min(READ_BUFFER_BYTES));
    let mut buffer = [0u8; READ_BUFFER_BYTES];
    let mut limited = reader.by_ref().take(read_limit as u64);
    loop {
        check_candidate_cancelled(cancel_token)?;
        let read = limited.read(&mut buffer).map_err(|error| {
            CandidateExtractionError::Warning(format!("{} read failed: {error}", candidate.path))
        })?;
        if read == 0 {
            break;
        }
        bytes.extend_from_slice(&buffer[..read]);
    }
    check_candidate_cancelled(cancel_token)?;
    Ok(bytes)
}

fn check_candidate_cancelled(cancel_token: &AtomicBool) -> Result<(), CandidateExtractionError> {
    ensure_not_cancelled(cancel_token).map_err(|_| CandidateExtractionError::Cancelled)
}

fn capability_for_candidate(
    selected: &[AnalysisCapability],
    candidate: &EvidenceCandidate,
) -> Option<AnalysisCapability> {
    if candidate.category == LINUX_UMBRELLA_KEY {
        let normalized = normalize_evidence_path(&candidate.path);
        let section = linux_artifact_section(&normalized);
        debug_assert!(LinuxArtifactSection::ALL.contains(&section));
        debug_assert_eq!(LinuxArtifactSection::from_key(section.key()), Some(section));
        let capability = selected
            .iter()
            .find(|item| item.key == section.key())
            .copied();
        return capability;
    }
    selected
        .iter()
        .find(|item| item.candidate_category == candidate.category)
        .copied()
}

fn discovery_categories(selected: &[AnalysisCapability]) -> Vec<&str> {
    let mut categories = Vec::new();
    for capability in selected {
        if !categories.contains(&capability.candidate_category) {
            categories.push(capability.candidate_category);
        }
    }
    categories
}

fn build_run_dto(
    conn: &Connection,
    state: ExtractionState,
) -> Result<AnalysisExtractionRunDto, AnalysisServiceError> {
    state.into_dto(conn, Utc::now().to_rfc3339())
}

fn elapsed_millis(started: Instant) -> u64 {
    u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX)
}

#[cfg(test)]
#[path = "../../../tests/unit/analysis_service/extraction/runner.rs"]
mod tests;
