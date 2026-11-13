use super::super::candidate_processing::capability_for_candidate;
use super::super::replacement_index::ReplacementSet;
use super::super::state::{
    AnalysisCheckpointKey, AnalysisOutputProducer, AnalysisOutputReplacement, CleanScanKeys,
    DiagnosticScanKeys,
};
use super::*;
use crate::analysis_service::capability::PLUGIN_CAPABILITY_KEY;
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
    progress: &mut ExtractionProgressReporter<'_>,
) -> Result<CheckpointScans, AnalysisServiceError> {
    let started = Instant::now();
    let scans = CheckpointScans {
        clean: existing_clean_scan_keys(conn)?,
        diagnostic: existing_diagnostic_scan_keys(conn)?,
        complete: existing_complete_scan_keys(conn)?,
        storage_available: AnalysisScanRepo::new(conn).storage_available()?,
    };
    let records = scans.clean.len() + scans.diagnostic.len() + scans.complete.len();
    progress.report_work(
        AnalysisWorkStageDto::CheckpointValidation,
        started,
        records as u64,
        0,
        true,
    );
    Ok(scans)
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
    progress: &mut ExtractionProgressReporter<'_>,
) -> Result<
    (
        Vec<EvidenceCandidate>,
        Option<LinuxCatalogMetadata>,
        PreparationCatalog,
    ),
    AnalysisServiceError,
> {
    let categories = discovery_categories(selected);
    let started = Instant::now();
    let (mut candidates, kernel, mut preparation_catalog) =
        evidence_candidates_for_categories_with_preparation(
            conn,
            &categories,
            cancel_token,
            |records| {
                progress.report_work(AnalysisWorkStageDto::Discovery, started, records, 0, false);
            },
        )?;
    candidates.append(&mut discover_plugin_candidates(
        conn,
        plugin_extractors,
        cancel_token,
    )?);
    if preparation_catalog.scanned_records() == 0
        && candidates
            .iter()
            .any(|candidate| candidate.category == PLUGIN_CAPABILITY_KEY)
    {
        preparation_catalog =
            PreparationCatalog::load_from_source(conn, cancel_token, |records| {
                progress.report_work(AnalysisWorkStageDto::Discovery, started, records, 0, false);
            })?;
    }
    order_candidates_for_extraction(conn, platform, &mut candidates);
    let linux_catalog = (platform == DataSourcePlatform::Linux
        && selected
            .iter()
            .any(|capability| capability.read_policy == CandidateReadPolicy::LinuxPathAware))
    .then(|| LinuxCatalogMetadata::from_discovery(&candidates, kernel));
    progress.report_work(
        AnalysisWorkStageDto::Discovery,
        started,
        preparation_catalog.scanned_records(),
        0,
        true,
    );
    Ok((candidates, linux_catalog, preparation_catalog))
}

pub(super) fn begin_streamed_output(
    conn: &Connection,
    candidates: &[EvidenceCandidate],
    selected: &[AnalysisCapability],
    plugin_extractors: &[&dyn ArtifactExtractor],
    checkpoints: &ExistingCheckpoints<'_>,
    cancel_token: &AtomicBool,
    progress: &mut ExtractionProgressReporter<'_>,
) -> Result<native_output::StreamingGeneration, AnalysisServiceError> {
    let output_source_id = resolve_output_data_source_id(conn)?;
    let replacements = candidate_replacements(candidates, selected, plugin_extractors, checkpoints);
    let started = Instant::now();
    let output = native_output::begin_streaming(
        conn,
        &output_source_id,
        &replacements,
        cancel_token,
        |copy| {
            progress.report_work(
                AnalysisWorkStageDto::GenerationClone,
                started,
                copy.records_copied,
                copy.bytes_copied,
                copy.durable,
            );
        },
        |_| {},
    )?;
    Ok(output)
}

fn candidate_replacements(
    candidates: &[EvidenceCandidate],
    selected: &[AnalysisCapability],
    plugins: &[&dyn ArtifactExtractor],
    checkpoints: &ExistingCheckpoints<'_>,
) -> Vec<AnalysisOutputReplacement> {
    let mut replacements = ReplacementSet::default();
    for candidate in candidates {
        let Some(capability) = capability_for_candidate(selected, candidate) else {
            continue;
        };
        if checkpoints.has_candidate(candidate, capability) {
            continue;
        }
        if capability.key == PLUGIN_CAPABILITY_KEY {
            for producer in super::super::plugin::matching_producer_ids(plugins, candidate) {
                replacements.insert(AnalysisOutputReplacement {
                    source_object_id: candidate.file_id.0.clone(),
                    producer: AnalysisOutputProducer::ExtractorId(producer),
                });
            }
        } else {
            replacements.insert(AnalysisOutputReplacement {
                source_object_id: candidate.file_id.0.clone(),
                producer: AnalysisOutputProducer::Prefix(capability.producer_prefix()),
            });
        }
    }
    replacements.into_vec()
}
