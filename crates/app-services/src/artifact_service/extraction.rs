use std::io::Read;

use artifacts_core::{ArtifactContext, ExtractorRegistry, VecSink};
use domain::{FileEntry, FileEntryId};
use rayon::prelude::*;

use super::{
    persistence::{already_has_artifact_for_source, store_artifacts},
    registry::create_registry,
    ArtifactServiceError,
};

const ARTIFACT_FILE_LIMIT_BYTES: u64 = infrastructure::constants::ARTIFACT_FILE_LIMIT_BYTES;
pub(super) const PARALLEL_EXTRACTION_BATCH_SIZE: usize = 2;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ArtifactExtractionStats {
    pub warning_count: u32,
    pub skipped_count: u32,
    pub failed_count: u32,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EvidenceScanStats {
    pub candidate_count: u32,
    pub scanned_count: u32,
    pub artifact_count: u32,
    pub warning_count: u32,
    pub skipped_count: u32,
    pub failed_count: u32,
    pub warnings: Vec<String>,
}

pub fn run_extractors_on_file(
    registry: &ExtractorRegistry,
    file_id: &FileEntryId,
    file_path: &str,
    reader: Box<dyn Read>,
    sink: &mut VecSink,
) -> Result<ArtifactExtractionStats, ArtifactServiceError> {
    let extractors = registry.find_for_path(file_path);
    if extractors.is_empty() {
        return Ok(ArtifactExtractionStats::default());
    }

    let (bytes, mut stats) = read_artifact_bytes(reader, file_path)?;
    run_extractors(&extractors, file_id, file_path, &bytes, sink, &mut stats);
    Ok(stats)
}

pub fn run_targeted_evidence_scan(
    conn: &rusqlite::Connection,
    case_id: &str,
    categories: &[&str],
    file_reader: impl Fn(&FileEntryId) -> Result<Box<dyn Read>, ArtifactServiceError>,
) -> Result<EvidenceScanStats, ArtifactServiceError> {
    let registry = create_registry();
    let selected = selected_categories(categories);
    let candidates = crate::analysis_service::evidence_candidates_for_categories(conn, &selected)?;
    let mut stats = EvidenceScanStats {
        candidate_count: candidates.len() as u32,
        ..EvidenceScanStats::default()
    };

    for candidate in candidates {
        if registry.find_for_path(&candidate.path).is_empty()
            || already_has_artifact_for_source(conn, &candidate.file_id.0)?
        {
            stats.skipped_count += 1;
            continue;
        }
        let reader = match file_reader(&candidate.file_id) {
            Ok(reader) => reader,
            Err(error) => {
                record_read_warning(&mut stats, &candidate.path, error);
                continue;
            }
        };
        let mut sink = VecSink::new();
        let file_stats = run_extractors_on_file(
            &registry,
            &candidate.file_id,
            &candidate.path,
            reader,
            &mut sink,
        )?;
        merge_extraction_stats(&mut stats, file_stats);
        if !sink.artifacts.is_empty() {
            store_artifacts(conn, &sink.artifacts, case_id, &candidate.data_source_id)?;
            stats.artifact_count += sink.artifacts.len() as u32;
        }
        stats.scanned_count += 1;
    }
    Ok(stats)
}

pub fn run_extractors_parallel<F>(
    registry: &ExtractorRegistry,
    files: &[FileEntry],
    file_reader: F,
    limit: usize,
) -> (Vec<domain::Artifact>, ArtifactExtractionStats)
where
    F: Fn(&FileEntryId) -> Result<Box<dyn Read>, ArtifactServiceError>,
{
    let candidates = files
        .iter()
        .filter(|file| !registry.find_for_path(&file.path).is_empty())
        .take(limit)
        .collect::<Vec<_>>();
    let mut results = Vec::with_capacity(candidates.len());
    for batch in candidates.chunks(PARALLEL_EXTRACTION_BATCH_SIZE) {
        let prepared = batch
            .iter()
            .map(|file| prepare_extraction(file, &file_reader))
            .collect::<Vec<_>>();
        let batch_results = prepared
            .into_par_iter()
            .map(|item| extract_prepared_file(registry, item))
            .collect::<Vec<_>>();
        results.extend(batch_results);
    }
    combine_parallel_results(results)
}

fn selected_categories<'a>(categories: &'a [&'a str]) -> Vec<&'a str> {
    if !categories.is_empty() {
        return categories.to_vec();
    }
    crate::analysis_service::evidence_category_defs()
        .iter()
        .filter(|definition| !definition.artifact_families.is_empty())
        .map(|definition| definition.category)
        .collect()
}

fn read_artifact_bytes(
    reader: Box<dyn Read>,
    file_path: &str,
) -> Result<(Vec<u8>, ArtifactExtractionStats), ArtifactServiceError> {
    let mut bytes = Vec::new();
    let bytes_read = reader
        .take(ARTIFACT_FILE_LIMIT_BYTES)
        .read_to_end(&mut bytes)?;
    let mut stats = ArtifactExtractionStats::default();
    if bytes_read as u64 >= ARTIFACT_FILE_LIMIT_BYTES {
        stats.warning_count += 1;
        stats.skipped_count += 1;
        tracing::warn!(
            "Artifact extraction truncated at {} bytes for file: {}",
            bytes_read,
            file_path
        );
    }
    Ok((bytes, stats))
}

fn run_extractors(
    extractors: &[&dyn artifacts_core::ArtifactExtractor],
    file_id: &FileEntryId,
    file_path: &str,
    bytes: &[u8],
    sink: &mut VecSink,
    stats: &mut ArtifactExtractionStats,
) {
    for extractor in extractors {
        let context = ArtifactContext {
            file_id: file_id.clone(),
            file_path: file_path.to_string(),
            reader: Box::new(std::io::Cursor::new(bytes.to_vec())),
        };
        if let Err(error) = extractor.run(context, sink) {
            stats.warning_count += 1;
            tracing::warn!("Extractor {} error: {}", extractor.id(), error);
        }
    }
}

enum PreparedExtraction<'a> {
    Ready {
        file: &'a FileEntry,
        bytes: Vec<u8>,
        stats: ArtifactExtractionStats,
    },
    Skipped(ArtifactExtractionStats),
}

fn prepare_extraction<'a, F>(file: &'a FileEntry, file_reader: &F) -> PreparedExtraction<'a>
where
    F: Fn(&FileEntryId) -> Result<Box<dyn Read>, ArtifactServiceError>,
{
    let reader = match file_reader(&file.id) {
        Ok(reader) => reader,
        Err(error) => {
            tracing::warn!(
                "Artifact extraction skipped unreadable file {}: {}",
                file.path,
                error
            );
            return PreparedExtraction::Skipped(ArtifactExtractionStats {
                warning_count: 1,
                skipped_count: 1,
                failed_count: 0,
            });
        }
    };
    match read_artifact_bytes(reader, &file.path) {
        Ok((bytes, stats)) => PreparedExtraction::Ready { file, bytes, stats },
        Err(error) => {
            tracing::warn!(
                "Artifact extraction read error for {}: {}",
                file.path,
                error
            );
            PreparedExtraction::Skipped(ArtifactExtractionStats {
                warning_count: 1,
                skipped_count: 1,
                failed_count: 0,
            })
        }
    }
}

fn extract_prepared_file(
    registry: &ExtractorRegistry,
    prepared: PreparedExtraction<'_>,
) -> (Vec<domain::Artifact>, ArtifactExtractionStats) {
    let (file, bytes, mut stats) = match prepared {
        PreparedExtraction::Ready { file, bytes, stats } => (file, bytes, stats),
        PreparedExtraction::Skipped(stats) => return (Vec::new(), stats),
    };
    let mut sink = VecSink::new();
    let extractors = registry.find_for_path(&file.path);
    run_extractors(
        &extractors,
        &file.id,
        &file.path,
        &bytes,
        &mut sink,
        &mut stats,
    );
    (sink.artifacts, stats)
}

fn combine_parallel_results(
    results: Vec<(Vec<domain::Artifact>, ArtifactExtractionStats)>,
) -> (Vec<domain::Artifact>, ArtifactExtractionStats) {
    let mut artifacts = Vec::new();
    let mut total = ArtifactExtractionStats::default();
    for (batch, stats) in results {
        artifacts.extend(batch);
        total.warning_count += stats.warning_count;
        total.skipped_count += stats.skipped_count;
        total.failed_count += stats.failed_count;
    }
    (artifacts, total)
}

fn record_read_warning(stats: &mut EvidenceScanStats, path: &str, error: ArtifactServiceError) {
    stats.warning_count += 1;
    stats.skipped_count += 1;
    stats.warnings.push(format!("{path}: {error}"));
}

fn merge_extraction_stats(stats: &mut EvidenceScanStats, file: ArtifactExtractionStats) {
    stats.warning_count += file.warning_count;
    stats.skipped_count += file.skipped_count;
    stats.failed_count += file.failed_count;
}
