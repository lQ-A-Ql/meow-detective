use std::path::Path;

use persistence_sqlite::repositories::file_repo::FileRepo;
use rusqlite::Connection;

use crate::file_service::{
    viewer::{
        image_open::LvmPoolRequestCache, is_linux_filesystem_kind, PreviewDescriptor,
        PreviewPartitionCandidate,
    },
    FileServiceError,
};

use super::{
    common::{read_descriptor_range, read_entry_range, record_failure, ReaderFactory},
    factory::open_filesystem_reader,
};

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
        let fs = match open_filesystem_reader(candidate, reader, fs_offset) {
            Ok(fs) => fs,
            Err(error) => {
                record_failure(
                    reasons,
                    format!(
                        "{} partition {} @{} open failed: {}",
                        candidate.filesystem_kind, candidate.partition_index, fs_offset, error
                    ),
                    "Descriptor Linux range filesystem open failed",
                );
                continue;
            }
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
