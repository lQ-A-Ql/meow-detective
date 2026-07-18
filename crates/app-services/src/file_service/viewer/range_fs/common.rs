use std::path::Path;

use persistence_sqlite::repositories::file_repo::FileRepo;
use rusqlite::Connection;

use crate::file_service::{
    viewer::{
        descriptor_image_path_candidates, e01_partition_candidates, entry_image_path_candidates,
        exact_partition_candidate, open_e01_reader_cached, raw_partition_candidates,
        resolve_partition_index_for_entry, PreviewDescriptor, PreviewPartitionCandidate,
    },
    FileServiceError,
};

pub(super) type ReaderFactory<'a> =
    dyn FnMut(&Path) -> std::io::Result<Box<dyn evidence_core::EvidenceReader>> + 'a;

pub(super) type CandidateRangeReader = fn(
    &Path,
    &[PreviewPartitionCandidate],
    &[String],
    u64,
    usize,
    &mut ReaderFactory<'_>,
    &mut Vec<String>,
) -> Result<Option<Vec<u8>>, FileServiceError>;

pub(super) fn read_descriptor_range(
    descriptor: &PreviewDescriptor,
    offset: u64,
    length: usize,
    reasons: &mut Vec<String>,
    read_candidates: CandidateRangeReader,
) -> Result<Option<Vec<u8>>, FileServiceError> {
    if !matches!(descriptor.source_kind.as_str(), "e01" | "raw")
        || descriptor.partition_candidates.is_empty()
    {
        return Ok(None);
    }
    let candidate = exact_partition_candidate(descriptor)?;
    let candidates = std::slice::from_ref(candidate);
    let source_path = Path::new(&descriptor.source_path);
    let path_candidates = descriptor_image_path_candidates(descriptor);
    match descriptor.source_kind.as_str() {
        "e01" => {
            let case_id = descriptor.case_id.clone();
            let mut factory = move |path: &Path| {
                open_e01_reader_cached(path, &case_id)
                    .map(|reader| Box::new(reader) as Box<dyn evidence_core::EvidenceReader>)
            };
            read_candidates(
                source_path,
                candidates,
                &path_candidates,
                offset,
                length,
                &mut factory,
                reasons,
            )
        }
        "raw" => {
            let mut factory = |path: &Path| {
                evidence_core::RawImageReader::open(path)
                    .map(|reader| Box::new(reader) as Box<dyn evidence_core::EvidenceReader>)
            };
            read_candidates(
                source_path,
                candidates,
                &path_candidates,
                offset,
                length,
                &mut factory,
                reasons,
            )
        }
        _ => Ok(None),
    }
}

pub(super) fn read_entry_range(
    conn: &Connection,
    repo: &FileRepo<'_>,
    entry: &domain::FileEntry,
    offset: u64,
    length: usize,
    read_candidates: CandidateRangeReader,
) -> Result<Option<Vec<u8>>, FileServiceError> {
    let (source_kind, source_path) = repo
        .find_data_source_location(&entry.data_source_id)?
        .ok_or_else(|| FileServiceError::not_found("Data source not found"))?;
    let partition_index = resolve_partition_index_for_entry(repo, entry)?;
    let path_candidates = entry_image_path_candidates(entry);
    match source_kind.as_str() {
        "e01" => {
            let candidates = e01_partition_candidates(conn, entry, partition_index)?;
            let mut factory = |path: &Path| {
                open_e01_reader_cached(path, "")
                    .map(|reader| Box::new(reader) as Box<dyn evidence_core::EvidenceReader>)
            };
            read_candidates(
                Path::new(&source_path),
                &candidates,
                &path_candidates,
                offset,
                length,
                &mut factory,
                &mut Vec::new(),
            )
        }
        "raw" => {
            let candidates = raw_partition_candidates(&source_path, partition_index)?;
            let mut factory = |path: &Path| {
                evidence_core::RawImageReader::open(path)
                    .map(|reader| Box::new(reader) as Box<dyn evidence_core::EvidenceReader>)
            };
            read_candidates(
                Path::new(&source_path),
                &candidates,
                &path_candidates,
                offset,
                length,
                &mut factory,
                &mut Vec::new(),
            )
        }
        _ => Ok(None),
    }
}

pub(super) fn record_failure(reasons: &mut Vec<String>, reason: String, context: &'static str) {
    tracing::warn!(%reason, context);
    reasons.push(reason);
}
