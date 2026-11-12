use super::super::state::{AnalysisCheckpointKey, CleanScanKeys, DiagnosticScanKeys};
use super::*;
use persistence_sqlite::repositories::analysis_scan_repo::CompleteAnalysisCandidateScan;
use std::collections::HashMap;

type CompleteScanKeys = HashMap<AnalysisCheckpointKey, CompleteAnalysisCandidateScan>;

pub(super) struct CheckpointScans {
    clean: CleanScanKeys,
    diagnostic: DiagnosticScanKeys,
    complete: CompleteScanKeys,
    storage_available: bool,
}

impl CheckpointScans {
    pub(super) fn existing(&self) -> ExistingCheckpoints<'_> {
        ExistingCheckpoints {
            clean: &self.clean,
            diagnostic: &self.diagnostic,
            complete: &self.complete,
            storage_available: self.storage_available,
        }
    }
}

pub(super) fn load_checkpoint_scans(
    conn: &Connection,
) -> Result<CheckpointScans, AnalysisServiceError> {
    Ok(CheckpointScans {
        clean: existing_clean_scan_keys(conn)?,
        diagnostic: existing_diagnostic_scan_keys(conn)?,
        complete: existing_complete_scan_keys(conn)?,
        storage_available: AnalysisScanRepo::new(conn).storage_available()?,
    })
}

pub(super) fn guarded_candidate_reader<F>(
    mut file_reader: F,
) -> impl FnMut(&EvidenceCandidate, usize) -> Result<CandidateSource, String>
where
    F: FnMut(&EvidenceCandidate, usize) -> Result<CandidateSource, String>,
{
    move |candidate, read_limit| {
        if let Some(warning) = encrypted_candidate_warning(candidate) {
            return Err(warning);
        }
        file_reader(candidate, read_limit)
    }
}

pub(super) fn discover_run_candidates(
    conn: &Connection,
    platform: DataSourcePlatform,
    selected: &[AnalysisCapability],
    plugin_extractors: &[&dyn ArtifactExtractor],
    cancel_token: &AtomicBool,
) -> Result<(Vec<EvidenceCandidate>, Option<LinuxCatalogMetadata>), AnalysisServiceError> {
    let categories = discovery_categories(selected);
    let (mut candidates, kernel) =
        evidence_candidates_for_categories_with_cancel_and_linux_catalog(
            conn,
            &categories,
            cancel_token,
        )?;
    candidates.append(&mut discover_plugin_candidates(
        conn,
        plugin_extractors,
        cancel_token,
    )?);
    order_candidates_for_extraction(conn, platform, &mut candidates);
    let linux_catalog = (platform == DataSourcePlatform::Linux)
        .then(|| LinuxCatalogMetadata::from_discovery(&candidates, kernel));
    Ok((candidates, linux_catalog))
}

pub(super) fn begin_streamed_output(
    conn: &Connection,
    candidates: &[EvidenceCandidate],
    selected: &[AnalysisCapability],
    plugin_extractors: &[&dyn ArtifactExtractor],
    checkpoints: &ExistingCheckpoints<'_>,
) -> Result<native_output::StreamingGeneration, AnalysisServiceError> {
    let output_source_id = resolve_output_data_source_id(conn)?;
    let replacements =
        super::candidate_replacements(candidates, selected, plugin_extractors, checkpoints);
    native_output::begin_streaming(conn, &output_source_id, &replacements)
}
