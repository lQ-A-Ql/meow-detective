use super::super::progress::CandidateProgressResult;
use super::*;
use crate::analysis_service::candidates::LINUX_CATALOG_METADATA_KEY;
use chrono::Utc;

pub(super) struct ReuseRequest<'a> {
    pub conn: &'a Connection,
    pub selected: &'a [AnalysisCapability],
    pub candidates: &'a [EvidenceCandidate],
    pub linux_catalog: Option<&'a LinuxCatalogMetadata>,
    pub checkpoints: &'a ExistingCheckpoints<'a>,
    pub preloads: &'a RunPreloads,
    pub plugins: &'a [&'a dyn ArtifactExtractor],
    pub plugin_loads: &'a [PluginLoadRecord],
    pub plugin_rejections: &'a [PluginRejection],
    pub cancel: &'a AtomicBool,
    pub discovery_started: Instant,
}

pub(super) fn try_reuse(
    request: ReuseRequest<'_>,
    progress: &mut ExtractionProgressReporter<'_>,
) -> Result<Option<AnalysisExtractionExecution>, AnalysisServiceError> {
    if !request.plugins.is_empty()
        || !catalog_unchanged(request.conn, request.linux_catalog)?
        || request.candidates.iter().any(|candidate| {
            capability_for_candidate(request.selected, candidate)
                .is_none_or(|capability| !request.checkpoints.has_candidate(candidate, capability))
        })
    {
        return Ok(None);
    }
    ensure_not_cancelled(request.cancel)?;
    verify_reused(&request, progress)?;
    let mut state = ExtractionState::new(request.selected);
    record_preload_warnings(request.selected, request.preloads, &mut state);
    progress.begin_extraction();
    for candidate in request.candidates {
        ensure_not_cancelled(request.cancel)?;
        replay_candidate(&request, candidate, &mut state, progress);
    }
    progress.begin_persisting();
    ensure_not_cancelled(request.cancel)?;
    let persistence_started = Instant::now();
    let failures = state.retryable_failure_count;
    let dto = state.into_dto(request.conn, Utc::now().to_rfc3339())?;
    progress.report_work(
        AnalysisWorkStageDto::ResultBuilding,
        persistence_started,
        dto.artifact_count,
        0,
        true,
    );
    if failures == 0 {
        progress.complete();
    } else {
        progress.fail(failures);
    }
    Ok(Some(AnalysisExtractionExecution {
        dto,
        retryable_failure_count: failures,
        discovery_elapsed_ms: elapsed_millis(request.discovery_started),
        processing_elapsed_ms: 0,
        persistence_elapsed_ms: elapsed_millis(persistence_started),
        work_metrics: progress.take_work_metrics(),
        source_read_count: 0,
        source_read_elapsed_ms: 0,
        filesystem_read_metrics: Default::default(),
        rados_read_metrics: Default::default(),
        plugin_loads: request.plugin_loads.to_vec(),
        plugin_rejections: request.plugin_rejections.to_vec(),
        plugin_extract_failures: Vec::new(),
    }))
}

fn replay_candidate(
    request: &ReuseRequest<'_>,
    candidate: &EvidenceCandidate,
    state: &mut ExtractionState,
    progress: &mut ExtractionProgressReporter<'_>,
) {
    let capability = capability_for_candidate(request.selected, candidate)
        .expect("reusable candidate has a selected capability");
    let key = (
        candidate.file_id.0.clone(),
        capability.key.to_owned(),
        candidate.size,
        candidate.content_identity.clone(),
    );
    let mut result = CandidateProgressResult {
        checkpoint_hit: true,
        ..Default::default()
    };
    if let Some(warnings) = request.checkpoints.diagnostic.get(&key) {
        result.warning = !warnings.is_empty();
        state.replay_diagnostic(capability, warnings);
    } else if request.checkpoints.clean.contains(&key) {
        state.replay_clean(capability);
    } else if let Some(scan) = request.checkpoints.complete.get(&key) {
        result.artifact_count = scan.artifact_count;
        result.timeline_event_count = scan.timeline_event_count;
        result.warning = !scan.warnings.is_empty();
        state.replay_complete(capability, scan);
    }
    progress.finish_candidate(capability, candidate, result);
}

fn verify_reused(
    request: &ReuseRequest<'_>,
    progress: &mut ExtractionProgressReporter<'_>,
) -> Result<(), AnalysisServiceError> {
    let store = persistence_source::detail::require_source_store(request.conn)?;
    let source_id = persistence_source::detail::source_id(request.conn)?
        .ok_or_else(|| AnalysisServiceError::InvalidInput("source identity is missing".into()))?;
    let started = Instant::now();
    let mut observed = (0, 0);
    persistence_source::fjall::verify_source_generation_with_control(
        &store,
        &source_id,
        request.cancel,
        |records, bytes| {
            observed = (records, bytes);
            progress.report_work(
                AnalysisWorkStageDto::GenerationVerify,
                started,
                records,
                bytes,
                false,
            );
        },
    )
    .map_err(native_output::clone_error)?;
    progress.report_work(
        AnalysisWorkStageDto::GenerationVerify,
        started,
        observed.0,
        observed.1,
        true,
    );
    Ok(())
}

fn catalog_unchanged(
    conn: &Connection,
    current: Option<&LinuxCatalogMetadata>,
) -> Result<bool, AnalysisServiceError> {
    let Some(current) = current else {
        return Ok(true);
    };
    let Some(stored) =
        persistence_sqlite::repositories::source_meta_repo::SourceMetaRepo::new(conn)
            .read(LINUX_CATALOG_METADATA_KEY)?
    else {
        return Ok(false);
    };
    let stored: LinuxCatalogMetadata = serde_json::from_str(&stored).map_err(|error| {
        AnalysisServiceError::Other(format!("decode Linux catalog metadata: {error}"))
    })?;
    Ok(&stored == current)
}
