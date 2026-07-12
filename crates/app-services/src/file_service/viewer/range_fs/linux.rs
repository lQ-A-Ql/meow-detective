use std::path::Path;

use evidence_core::FileSystemReader;
use persistence_sqlite::repositories::file_repo::FileRepo;
use rusqlite::Connection;

use crate::file_service::{
    viewer::{
        image_open::LvmPoolRequestCache, is_linux_filesystem_kind, PreviewDescriptor,
        PreviewPartitionCandidate,
    },
    FileServiceError,
};

use super::common::{read_descriptor_range, read_entry_range, record_failure, ReaderFactory};

pub(crate) fn try_read_linux_image_range_for_descriptor(
    descriptor: &PreviewDescriptor,
    offset: u64,
    length: usize,
    reasons: &mut Vec<String>,
) -> Result<Option<Vec<u8>>, FileServiceError> {
    read_descriptor_range(descriptor, offset, length, reasons, read_candidates)
}

pub(crate) fn try_read_linux_image_range_for_entry(
    conn: &Connection,
    repo: &FileRepo<'_>,
    entry: &domain::FileEntry,
    offset: u64,
    length: usize,
) -> Result<Option<Vec<u8>>, FileServiceError> {
    read_entry_range(conn, repo, entry, offset, length, read_candidates)
}

fn read_candidates(
    source_path: &Path,
    candidates: &[PreviewPartitionCandidate],
    paths: &[String],
    offset: u64,
    length: usize,
    open_reader: &mut ReaderFactory<'_>,
    reasons: &mut Vec<String>,
) -> Result<Option<Vec<u8>>, FileServiceError> {
    let mut lvm_cache = LvmPoolRequestCache::new();
    for candidate in candidates {
        if !is_linux_filesystem_kind(&candidate.filesystem_kind) {
            continue;
        }
        let (reader, fs_offset) =
            match crate::file_service::viewer::image_open::open_candidate_block_reader_with_lvm_cache(
                source_path,
                candidate,
                open_reader,
                &mut lvm_cache,
            ) {
                Ok(reader) => reader,
                Err(error) => {
                    record_failure(
                        reasons,
                        format!(
                            "{} partition {} @{} reader open failed: {}",
                            candidate.filesystem_kind,
                            candidate.partition_index,
                            candidate.offset,
                            error
                        ),
                        "Descriptor Linux range reader open failed",
                    );
                    continue;
                }
            };
        let Some(fs) = open_filesystem(candidate, reader, fs_offset, reasons) else {
            continue;
        };
        for path in paths {
            match fs.read_file_range(path, offset, length) {
                Ok(bytes) => return Ok(Some(bytes)),
                Err(error) => record_failure(
                    reasons,
                    format!(
                        "{} partition {} @{} path '{}' range read failed: {}",
                        candidate.filesystem_kind,
                        candidate.partition_index,
                        candidate.offset,
                        path,
                        error
                    ),
                    "Descriptor Linux range read failed for path candidate",
                ),
            }
        }
    }
    Ok(None)
}

fn open_filesystem(
    candidate: &PreviewPartitionCandidate,
    reader: Box<dyn evidence_core::EvidenceReader>,
    fs_offset: u64,
    reasons: &mut Vec<String>,
) -> Option<Box<dyn FileSystemReader>> {
    let result: std::io::Result<Box<dyn FileSystemReader>> = match candidate
        .filesystem_kind
        .as_str()
    {
        kind if kind.eq_ignore_ascii_case("ext4") => fs_ext4::Ext4Reader::open(reader, fs_offset)
            .map(|fs| Box::new(fs) as Box<dyn FileSystemReader>),
        kind if kind.eq_ignore_ascii_case("xfs") => fs_xfs::XfsReader::open(reader, fs_offset)
            .map(|fs| Box::new(fs) as Box<dyn FileSystemReader>),
        kind if kind.eq_ignore_ascii_case("btrfs") => {
            fs_btrfs::BtrfsReader::open(reader, fs_offset)
                .map(|fs| Box::new(fs) as Box<dyn FileSystemReader>)
        }
        _ => return None,
    };
    match result {
        Ok(fs) => Some(fs),
        Err(error) => {
            record_failure(
                reasons,
                format!(
                    "{} partition {} @{} open failed: {}",
                    candidate.filesystem_kind, candidate.partition_index, fs_offset, error
                ),
                "Descriptor Linux range filesystem open failed",
            );
            None
        }
    }
}
