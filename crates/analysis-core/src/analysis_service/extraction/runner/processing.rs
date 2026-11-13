use super::*;

#[allow(clippy::too_many_arguments)]
pub(super) fn run(
    conn: &Connection,
    selected: &[AnalysisCapability],
    candidates: Vec<EvidenceCandidate>,
    linux_catalog: Option<LinuxCatalogMetadata>,
    checkpoints: &ExistingCheckpoints<'_>,
    preloads: &RunPreloads,
    output: native_output::StreamingGeneration,
    reader: &mut impl FnMut(&EvidenceCandidate, usize) -> Result<CandidateSource, String>,
    progress: &mut ExtractionProgressReporter<'_>,
    discovery_started: Instant,
    plugins: &[&dyn ArtifactExtractor],
    plugin_loads: Vec<PluginLoadRecord>,
    plugin_rejections: Vec<PluginRejection>,
    cancel: &AtomicBool,
) -> Result<AnalysisExtractionExecution, AnalysisServiceError> {
    let discovery_elapsed_ms = elapsed_millis(discovery_started);
    progress.begin_extraction();
    let mut state = ExtractionState::new(selected);
    if let Some(catalog) = linux_catalog {
        state.record_linux_catalog(catalog);
    }
    record_preload_warnings(selected, preloads, &mut state);
    let started = Instant::now();
    let context = CandidateProcessingContext::new(
        conn,
        selected,
        checkpoints,
        preloads.as_refs(),
        plugins,
        &output,
        cancel,
    );
    process_candidates(&context, candidates, reader, &mut state, progress)?;
    ensure_not_cancelled(cancel)?;
    let processing_elapsed_ms = elapsed_millis(started);
    progress.report_work(
        AnalysisWorkStageDto::Processing,
        started,
        state.scanned_count,
        0,
        true,
    );
    let result = super::finalization::persist_processed_output(
        conn,
        state,
        output,
        progress,
        discovery_elapsed_ms,
        processing_elapsed_ms,
        plugin_loads,
        plugin_rejections,
        cancel,
    )?;
    preloads.systemd.persist_markers(conn)?;
    Ok(result)
}
