use std::path::Path;

use persistence_sqlite::repositories::file_repo::FileRepo;
use rusqlite::Connection;

use crate::file_service::{
    viewer::{
        image_open::LvmPoolRequestCache, is_exfat_filesystem_kind, looks_like_exfat_boot_sector,
        PreviewDescriptor, PreviewPartitionCandidate,
    },
    FileServiceError,
};

use super::common::{read_descriptor_range, read_entry_range, record_failure, ReaderFactory};

pub(crate) fn try_read_exfat_image_range_for_descriptor(
    descriptor: &PreviewDescriptor,
    offset: u64,
    length: usize,
    reasons: &mut Vec<String>,
) -> Result<Option<Vec<u8>>, FileServiceError> {
    read_descriptor_range(descriptor, offset, length, reasons, read_candidates)
}

pub(crate) fn try_read_exfat_image_range_for_entry(
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
        let (mut reader, fs_offset) =
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
                            "exFAT partition {} @{} reader open failed: {}",
                            candidate.partition_index, candidate.offset, error
                        ),
                        "Descriptor exFAT range reader open failed",
                    );
                    continue;
                }
            };
        if !is_exfat_filesystem_kind(&candidate.filesystem_kind)
            && !looks_like_exfat_boot_sector(reader.as_mut(), fs_offset).unwrap_or(false)
        {
            continue;
        }
        let fs = match fs_exfat::ExfatReader::open(reader, fs_offset) {
            Ok(fs) => fs,
            Err(error) => {
                record_failure(
                    reasons,
                    format!(
                        "exFAT partition {} @{} open failed: {}",
                        candidate.partition_index, fs_offset, error
                    ),
                    "Descriptor exFAT range open failed",
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
                        "exFAT partition {} @{} path '{}' range read failed: {}",
                        candidate.partition_index, candidate.offset, path, error
                    ),
                    "Descriptor exFAT range read failed for path candidate",
                ),
            }
        }
    }
    Ok(None)
}
