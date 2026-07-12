use std::{io::Read, path::Path};

use domain::FileEntry;
use evidence_core::{EvidenceReader, FileSystemReader};
use persistence_sqlite::repositories::partition_repo::{DataSourcePartitionRecord, PartitionRepo};
use rusqlite::Connection;

use crate::file_service::{
    viewer::{
        entry_image_path_candidates, is_exfat_filesystem_kind, is_linux_filesystem_kind,
        is_preview_image_filesystem_kind, looks_like_exfat_boot_sector, open_e01_reader_cached,
        open_first_image_path,
        partition::{is_previewable_partition_status, preview_partition_candidate_from_record},
        PreviewPartitionCandidate, RangeContentReader,
    },
    FileServiceError,
};

use super::{descriptor::open_linux_image_candidate, lvm::open_candidate_block_reader};

pub(crate) fn open_e01_file(
    conn: &Connection,
    source_path: &str,
    entry: &FileEntry,
    expected_partition_index: Option<usize>,
) -> Result<Box<dyn Read>, FileServiceError> {
    open_e01_file_with_reader_factory(conn, source_path, entry, expected_partition_index, |path| {
        open_e01_reader_cached(path, "").map(|reader| Box::new(reader) as Box<dyn EvidenceReader>)
    })
}

pub(crate) fn open_e01_file_with_reader_factory<F>(
    conn: &Connection,
    source_path: &str,
    entry: &FileEntry,
    expected_partition_index: Option<usize>,
    mut open_reader: F,
) -> Result<Box<dyn Read>, FileServiceError>
where
    F: FnMut(&Path) -> std::io::Result<Box<dyn EvidenceReader>>,
{
    let partitions = load_partitions(conn, entry)?;
    validate_entry_path(entry)?;
    let candidates = select_partitions(&partitions, expected_partition_index)?;
    let paths = entry_image_path_candidates(entry);
    for target in candidates {
        let fs_kind = target.filesystem.as_deref().unwrap_or(&target.kind_label);
        let candidate = preview_partition_candidate_from_record(target);
        match try_open_partition(
            source_path,
            entry,
            target,
            fs_kind,
            &candidate,
            &paths,
            &mut open_reader,
        ) {
            Ok(Some(reader)) => return Ok(reader),
            Ok(None) => {}
            Err(error) => tracing::warn!(
                path = %entry.path,
                partition = %target.name,
                kind = %fs_kind,
                error = %error,
                "E01 file not found on partition"
            ),
        }
    }
    Err(FileServiceError::other(format!(
        "Cannot open image-backed file '{}' from any partition",
        entry.path
    )))
}

fn load_partitions(
    conn: &Connection,
    entry: &FileEntry,
) -> Result<Vec<DataSourcePartitionRecord>, FileServiceError> {
    let partitions = PartitionRepo::new(conn)
        .find_by_data_source(&entry.data_source_id.0)
        .map_err(|error| FileServiceError::other(format!("Failed to query partitions: {error}")))?;
    if partitions.is_empty() {
        return Err(FileServiceError::other(
            "No partition metadata found for this data source. Re-import the E01 image.",
        ));
    }
    Ok(partitions)
}

fn validate_entry_path(entry: &FileEntry) -> Result<(), FileServiceError> {
    if !entry.path.contains('/') && !entry.path.contains('\\') {
        return Err(FileServiceError::other(format!(
            "Cannot preview '{}': path reconstruction did not resolve the parent directory. Re-import.",
            entry.path
        )));
    }
    Ok(())
}

fn select_partitions(
    partitions: &[DataSourcePartitionRecord],
    expected_partition_index: Option<usize>,
) -> Result<Vec<&DataSourcePartitionRecord>, FileServiceError> {
    let selected = match expected_partition_index {
        Some(expected) => partitions
            .iter()
            .filter(|partition| {
                partition.partition_index as usize == expected
                    && is_previewable_partition_status(&partition.status)
            })
            .collect::<Vec<_>>(),
        None => partitions
            .iter()
            .filter(|partition| {
                is_previewable_partition_status(&partition.status)
                    && is_preview_image_filesystem_kind(
                        partition
                            .filesystem
                            .as_deref()
                            .unwrap_or(&partition.kind_label),
                    )
            })
            .collect::<Vec<_>>(),
    };
    if selected.is_empty() {
        return Err(FileServiceError::other(match expected_partition_index {
            Some(expected) => {
                format!("Partition index {expected} not found or is encrypted. Re-import.")
            }
            None => {
                "Cannot determine which partition this file belongs to. Re-import the E01 image."
                    .to_string()
            }
        }));
    }
    Ok(selected)
}

#[allow(clippy::too_many_arguments)]
fn try_open_partition<F>(
    source_path: &str,
    entry: &FileEntry,
    target: &DataSourcePartitionRecord,
    fs_kind: &str,
    candidate: &PreviewPartitionCandidate,
    paths: &[String],
    open_reader: &mut F,
) -> Result<Option<Box<dyn Read>>, FileServiceError>
where
    F: FnMut(&Path) -> std::io::Result<Box<dyn EvidenceReader>>,
{
    let result = match fs_kind {
        "NTFS" => open_ntfs(source_path, entry, candidate, open_reader),
        "FAT" | "FAT32" | "FAT16" | "FAT12" => open_fat(source_path, candidate, paths, open_reader),
        _ if exfat_hint(source_path, target, fs_kind)? => {
            open_exfat(source_path, candidate, paths, open_reader)
        }
        _ if is_linux_filesystem_kind(fs_kind) => {
            open_linux_image_candidate(Path::new(source_path), candidate, paths, open_reader)
                .map(range_content_reader_into_read)
        }
        _ => return Ok(None),
    };
    result
        .map(Some)
        .map_err(|error| FileServiceError::other(error.to_string()))
}

fn open_ntfs<F>(
    source_path: &str,
    entry: &FileEntry,
    candidate: &PreviewPartitionCandidate,
    open_reader: &mut F,
) -> std::io::Result<Box<dyn Read>>
where
    F: FnMut(&Path) -> std::io::Result<Box<dyn EvidenceReader>>,
{
    let (reader, offset) =
        open_candidate_block_reader(Path::new(source_path), candidate, open_reader)?;
    let fs = fs_ntfs::NtfsReader::open(reader, offset)?;
    fs.open_file(&entry.path)
        .or_else(|_| fs.open_file(&entry.id.0))
}

fn open_fat<F>(
    source_path: &str,
    candidate: &PreviewPartitionCandidate,
    paths: &[String],
    open_reader: &mut F,
) -> std::io::Result<Box<dyn Read>>
where
    F: FnMut(&Path) -> std::io::Result<Box<dyn EvidenceReader>>,
{
    let (reader, offset) =
        open_candidate_block_reader(Path::new(source_path), candidate, open_reader)?;
    match fs_fat::FatReader::open(reader, offset) {
        Ok(fs) => open_first_image_path(&fs, paths),
        Err(fat_error) => {
            let (reader, offset) =
                open_candidate_block_reader(Path::new(source_path), candidate, open_reader)?;
            match fs_exfat::ExfatReader::open(reader, offset) {
                Ok(fs) => open_first_image_path(&fs, paths),
                Err(exfat_error) => Err(std::io::Error::new(
                    exfat_error.kind(),
                    format!("FAT open failed: {fat_error}; exFAT open failed: {exfat_error}"),
                )),
            }
        }
    }
}

fn open_exfat<F>(
    source_path: &str,
    candidate: &PreviewPartitionCandidate,
    paths: &[String],
    open_reader: &mut F,
) -> std::io::Result<Box<dyn Read>>
where
    F: FnMut(&Path) -> std::io::Result<Box<dyn EvidenceReader>>,
{
    let (reader, offset) =
        open_candidate_block_reader(Path::new(source_path), candidate, open_reader)?;
    let fs = fs_exfat::ExfatReader::open(reader, offset)?;
    open_first_image_path(&fs, paths)
}

fn exfat_hint(
    source_path: &str,
    target: &DataSourcePartitionRecord,
    fs_kind: &str,
) -> Result<bool, FileServiceError> {
    if is_exfat_filesystem_kind(fs_kind) {
        return Ok(true);
    }
    if is_linux_filesystem_kind(fs_kind) || target.lvm_pv_offsets_json.is_some() {
        return Ok(false);
    }
    let mut reader = open_e01_reader_cached(Path::new(source_path), "")?;
    Ok(looks_like_exfat_boot_sector(&mut reader, target.offset).unwrap_or(false))
}

fn range_content_reader_into_read(reader: RangeContentReader) -> Box<dyn Read> {
    match reader {
        RangeContentReader::Seekable(reader) => reader as Box<dyn Read>,
        RangeContentReader::Streaming(reader) => reader,
    }
}
