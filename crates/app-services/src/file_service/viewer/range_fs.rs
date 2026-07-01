//! Filesystem-specific fast paths for reading a byte range from an image-backed
//! file without materializing the whole file.

use crate::file_service::viewer::{
    descriptor_image_path_candidates, entry_image_path_candidates, is_exfat_filesystem_kind,
    is_fat_filesystem_kind, open_e01_reader_cached, PreviewDescriptor, PreviewPartitionCandidate,
};
use crate::file_service::FileServiceError;
use persistence_sqlite::repositories::file_repo::FileRepo;
use rusqlite::Connection;
use std::path::Path;

pub(crate) fn try_read_ntfs_image_range_for_descriptor(
    descriptor: &PreviewDescriptor,
    offset: u64,
    length: usize,
    reasons: &mut Vec<String>,
) -> Result<Option<Vec<u8>>, FileServiceError> {
    if !matches!(descriptor.source_kind.as_str(), "e01" | "raw") {
        return Ok(None);
    }
    if descriptor.partition_candidates.is_empty() {
        return Ok(None);
    }

    let source_path = Path::new(&descriptor.source_path);
    let path_candidates = descriptor_image_path_candidates(descriptor);
    match descriptor.source_kind.as_str() {
        "e01" => {
            let case_id = descriptor.case_id.clone();
            try_read_ntfs_image_range_from_candidates(
                source_path,
                &descriptor.partition_candidates,
                &path_candidates,
                offset,
                length,
                move |path| {
                    open_e01_reader_cached(path, &case_id)
                        .map(|reader| Box::new(reader) as Box<dyn evidence_core::EvidenceReader>)
                },
                reasons,
            )
        }
        "raw" => try_read_ntfs_image_range_from_candidates(
            source_path,
            &descriptor.partition_candidates,
            &path_candidates,
            offset,
            length,
            |path| {
                evidence_core::RawImageReader::open(path)
                    .map(|reader| Box::new(reader) as Box<dyn evidence_core::EvidenceReader>)
            },
            reasons,
        ),
        _ => Ok(None),
    }
}

pub(crate) fn try_read_ntfs_image_range_for_entry(
    conn: &Connection,
    repo: &FileRepo<'_>,
    entry: &domain::FileEntry,
    offset: u64,
    length: usize,
) -> Result<Option<Vec<u8>>, FileServiceError> {
    let (source_kind, source_path) = repo
        .find_data_source_location(&entry.data_source_id)?
        .ok_or_else(|| FileServiceError::not_found("Data source not found"))?;
    let expected_partition_index =
        crate::file_service::viewer::root_partition_index_for_entry(repo, entry);

    match source_kind.as_str() {
        "e01" => {
            let candidates = crate::file_service::viewer::e01_partition_candidates(
                conn,
                entry,
                expected_partition_index,
            )?;
            let path_candidates = entry_image_path_candidates(entry);
            try_read_ntfs_image_range_from_candidates(
                Path::new(&source_path),
                &candidates,
                &path_candidates,
                offset,
                length,
                |path| {
                    open_e01_reader_cached(path, "")
                        .map(|reader| Box::new(reader) as Box<dyn evidence_core::EvidenceReader>)
                },
                &mut Vec::new(),
            )
        }
        "raw" => {
            let candidates = crate::file_service::viewer::raw_partition_candidates(
                &source_path,
                expected_partition_index,
            )?;
            let path_candidates = entry_image_path_candidates(entry);
            try_read_ntfs_image_range_from_candidates(
                Path::new(&source_path),
                &candidates,
                &path_candidates,
                offset,
                length,
                |path| {
                    evidence_core::RawImageReader::open(path)
                        .map(|reader| Box::new(reader) as Box<dyn evidence_core::EvidenceReader>)
                },
                &mut Vec::new(),
            )
        }
        _ => Ok(None),
    }
}

pub(crate) fn try_read_fat_image_range_for_descriptor(
    descriptor: &PreviewDescriptor,
    offset: u64,
    length: usize,
    reasons: &mut Vec<String>,
) -> Result<Option<Vec<u8>>, FileServiceError> {
    if !matches!(descriptor.source_kind.as_str(), "e01" | "raw") {
        return Ok(None);
    }
    if descriptor.partition_candidates.is_empty() {
        return Ok(None);
    }

    let source_path = Path::new(&descriptor.source_path);
    let path_candidates = descriptor_image_path_candidates(descriptor);
    match descriptor.source_kind.as_str() {
        "e01" => {
            let case_id = descriptor.case_id.clone();
            try_read_fat_image_range_from_candidates(
                source_path,
                &descriptor.partition_candidates,
                &path_candidates,
                offset,
                length,
                move |path| {
                    open_e01_reader_cached(path, &case_id)
                        .map(|reader| Box::new(reader) as Box<dyn evidence_core::EvidenceReader>)
                },
                reasons,
            )
        }
        "raw" => try_read_fat_image_range_from_candidates(
            source_path,
            &descriptor.partition_candidates,
            &path_candidates,
            offset,
            length,
            |path| {
                evidence_core::RawImageReader::open(path)
                    .map(|reader| Box::new(reader) as Box<dyn evidence_core::EvidenceReader>)
            },
            reasons,
        ),
        _ => Ok(None),
    }
}

pub(crate) fn try_read_fat_image_range_for_entry(
    conn: &Connection,
    repo: &FileRepo<'_>,
    entry: &domain::FileEntry,
    offset: u64,
    length: usize,
) -> Result<Option<Vec<u8>>, FileServiceError> {
    let (source_kind, source_path) = repo
        .find_data_source_location(&entry.data_source_id)?
        .ok_or_else(|| FileServiceError::not_found("Data source not found"))?;
    let expected_partition_index =
        crate::file_service::viewer::root_partition_index_for_entry(repo, entry);

    match source_kind.as_str() {
        "e01" => {
            let candidates = crate::file_service::viewer::e01_partition_candidates(
                conn,
                entry,
                expected_partition_index,
            )?;
            let path_candidates = entry_image_path_candidates(entry);
            try_read_fat_image_range_from_candidates(
                Path::new(&source_path),
                &candidates,
                &path_candidates,
                offset,
                length,
                |path| {
                    open_e01_reader_cached(path, "")
                        .map(|reader| Box::new(reader) as Box<dyn evidence_core::EvidenceReader>)
                },
                &mut Vec::new(),
            )
        }
        "raw" => {
            let candidates = crate::file_service::viewer::raw_partition_candidates(
                &source_path,
                expected_partition_index,
            )?;
            let path_candidates = entry_image_path_candidates(entry);
            try_read_fat_image_range_from_candidates(
                Path::new(&source_path),
                &candidates,
                &path_candidates,
                offset,
                length,
                |path| {
                    evidence_core::RawImageReader::open(path)
                        .map(|reader| Box::new(reader) as Box<dyn evidence_core::EvidenceReader>)
                },
                &mut Vec::new(),
            )
        }
        _ => Ok(None),
    }
}

pub(crate) fn try_read_exfat_image_range_for_descriptor(
    descriptor: &PreviewDescriptor,
    offset: u64,
    length: usize,
    reasons: &mut Vec<String>,
) -> Result<Option<Vec<u8>>, FileServiceError> {
    if !matches!(descriptor.source_kind.as_str(), "e01" | "raw") {
        return Ok(None);
    }
    if descriptor.partition_candidates.is_empty() {
        return Ok(None);
    }

    let source_path = Path::new(&descriptor.source_path);
    let path_candidates = descriptor_image_path_candidates(descriptor);
    match descriptor.source_kind.as_str() {
        "e01" => {
            let case_id = descriptor.case_id.clone();
            try_read_exfat_image_range_from_candidates(
                source_path,
                &descriptor.partition_candidates,
                &path_candidates,
                offset,
                length,
                move |path| {
                    open_e01_reader_cached(path, &case_id)
                        .map(|reader| Box::new(reader) as Box<dyn evidence_core::EvidenceReader>)
                },
                reasons,
            )
        }
        "raw" => try_read_exfat_image_range_from_candidates(
            source_path,
            &descriptor.partition_candidates,
            &path_candidates,
            offset,
            length,
            |path| {
                evidence_core::RawImageReader::open(path)
                    .map(|reader| Box::new(reader) as Box<dyn evidence_core::EvidenceReader>)
            },
            reasons,
        ),
        _ => Ok(None),
    }
}

pub(crate) fn try_read_exfat_image_range_for_entry(
    conn: &Connection,
    repo: &FileRepo<'_>,
    entry: &domain::FileEntry,
    offset: u64,
    length: usize,
) -> Result<Option<Vec<u8>>, FileServiceError> {
    let (source_kind, source_path) = repo
        .find_data_source_location(&entry.data_source_id)?
        .ok_or_else(|| FileServiceError::not_found("Data source not found"))?;
    let expected_partition_index =
        crate::file_service::viewer::root_partition_index_for_entry(repo, entry);

    match source_kind.as_str() {
        "e01" => {
            let candidates = crate::file_service::viewer::e01_partition_candidates(
                conn,
                entry,
                expected_partition_index,
            )?;
            let path_candidates = entry_image_path_candidates(entry);
            try_read_exfat_image_range_from_candidates(
                Path::new(&source_path),
                &candidates,
                &path_candidates,
                offset,
                length,
                |path| {
                    open_e01_reader_cached(path, "")
                        .map(|reader| Box::new(reader) as Box<dyn evidence_core::EvidenceReader>)
                },
                &mut Vec::new(),
            )
        }
        "raw" => {
            let candidates = crate::file_service::viewer::raw_partition_candidates(
                &source_path,
                expected_partition_index,
            )?;
            let path_candidates = entry_image_path_candidates(entry);
            try_read_exfat_image_range_from_candidates(
                Path::new(&source_path),
                &candidates,
                &path_candidates,
                offset,
                length,
                |path| {
                    evidence_core::RawImageReader::open(path)
                        .map(|reader| Box::new(reader) as Box<dyn evidence_core::EvidenceReader>)
                },
                &mut Vec::new(),
            )
        }
        _ => Ok(None),
    }
}

pub(crate) fn try_read_fat_image_range_from_candidates<F>(
    source_path: &Path,
    partition_candidates: &[PreviewPartitionCandidate],
    path_candidates: &[String],
    offset: u64,
    length: usize,
    mut open_reader: F,
    reasons: &mut Vec<String>,
) -> Result<Option<Vec<u8>>, FileServiceError>
where
    F: FnMut(&Path) -> std::io::Result<Box<dyn evidence_core::EvidenceReader>>,
{
    for candidate in partition_candidates {
        if !is_fat_filesystem_kind(&candidate.filesystem_kind) {
            continue;
        }

        let boxed_reader = open_reader(source_path)?;
        let fs = match fs_fat::FatReader::open(boxed_reader, candidate.offset) {
            Ok(fs) => fs,
            Err(error) => {
                let reason = format!(
                    "FAT partition {} @{} open failed: {}",
                    candidate.partition_index, candidate.offset, error
                );
                tracing::warn!(%reason, "Descriptor FAT range open failed");
                reasons.push(reason);
                continue;
            }
        };

        for path in path_candidates {
            match fs.read_file_range(path, offset, length) {
                Ok(bytes) => return Ok(Some(bytes)),
                Err(error) => {
                    let reason = format!(
                        "FAT partition {} @{} path '{}' range read failed: {}",
                        candidate.partition_index, candidate.offset, path, error
                    );
                    tracing::warn!(%reason, "Descriptor FAT range read failed for path candidate");
                    reasons.push(reason);
                }
            }
        }
    }

    Ok(None)
}

pub(crate) fn try_read_exfat_image_range_from_candidates<F>(
    source_path: &Path,
    partition_candidates: &[PreviewPartitionCandidate],
    path_candidates: &[String],
    offset: u64,
    length: usize,
    mut open_reader: F,
    reasons: &mut Vec<String>,
) -> Result<Option<Vec<u8>>, FileServiceError>
where
    F: FnMut(&Path) -> std::io::Result<Box<dyn evidence_core::EvidenceReader>>,
{
    for candidate in partition_candidates {
        let mut boxed_reader = open_reader(source_path)?;
        let looks_like_exfat = is_exfat_filesystem_kind(&candidate.filesystem_kind)
            || crate::file_service::viewer::looks_like_exfat_boot_sector(
                boxed_reader.as_mut(),
                candidate.offset,
            )
            .unwrap_or(false);
        if !looks_like_exfat {
            continue;
        }

        let fs = match fs_exfat::ExfatReader::open(boxed_reader, candidate.offset) {
            Ok(fs) => fs,
            Err(error) => {
                let reason = format!(
                    "exFAT partition {} @{} open failed: {}",
                    candidate.partition_index, candidate.offset, error
                );
                tracing::warn!(%reason, "Descriptor exFAT range open failed");
                reasons.push(reason);
                continue;
            }
        };

        for path in path_candidates {
            match fs.read_file_range(path, offset, length) {
                Ok(bytes) => return Ok(Some(bytes)),
                Err(error) => {
                    let reason = format!(
                        "exFAT partition {} @{} path '{}' range read failed: {}",
                        candidate.partition_index, candidate.offset, path, error
                    );
                    tracing::warn!(%reason, "Descriptor exFAT range read failed for path candidate");
                    reasons.push(reason);
                }
            }
        }
    }

    Ok(None)
}

pub(crate) fn try_read_ntfs_image_range_from_candidates<F>(
    source_path: &Path,
    partition_candidates: &[PreviewPartitionCandidate],
    path_candidates: &[String],
    offset: u64,
    length: usize,
    mut open_reader: F,
    reasons: &mut Vec<String>,
) -> Result<Option<Vec<u8>>, FileServiceError>
where
    F: FnMut(&Path) -> std::io::Result<Box<dyn evidence_core::EvidenceReader>>,
{
    for candidate in partition_candidates {
        if candidate.filesystem_kind != "NTFS" {
            continue;
        }

        let boxed_reader = open_reader(source_path)?;
        let fs = match fs_ntfs::NtfsReader::open(boxed_reader, candidate.offset) {
            Ok(fs) => fs,
            Err(error) => {
                let reason = format!(
                    "NTFS partition {} @{} open failed: {}",
                    candidate.partition_index, candidate.offset, error
                );
                tracing::warn!(%reason, "Descriptor NTFS range open failed");
                reasons.push(reason);
                continue;
            }
        };

        for path in path_candidates {
            match fs.read_file_range(path, offset, length) {
                Ok(bytes) => return Ok(Some(bytes)),
                Err(error) => {
                    let reason = format!(
                        "NTFS partition {} @{} path '{}' range read failed: {}",
                        candidate.partition_index, candidate.offset, path, error
                    );
                    tracing::warn!(%reason, "Descriptor NTFS range read failed for path candidate");
                    reasons.push(reason);
                }
            }
        }
    }

    Ok(None)
}
