//! Partition candidate discovery for E01 and RAW image previews.

use crate::file_service::FileServiceError;
use domain::FileEntry;
use evidence_core::RawImageReader;
use persistence_sqlite::repositories::partition_repo::PartitionRepo;
use rusqlite::Connection;
use std::path::Path;

pub(crate) fn e01_partition_candidates(
    conn: &Connection,
    entry: &FileEntry,
    expected_partition_index: Option<usize>,
) -> Result<Vec<crate::file_service::viewer::PreviewPartitionCandidate>, FileServiceError> {
    let part_repo = PartitionRepo::new(conn);
    let partitions = part_repo
        .find_by_data_source(&entry.data_source_id.0)
        .map_err(|e| FileServiceError::other(format!("Failed to query partitions: {e}")))?;

    if partitions.is_empty() {
        return Err(FileServiceError::other(
            "No partition metadata found for this data source. Re-import the E01 image.",
        ));
    }

    if !entry.path.contains('/') && !entry.path.contains('\\') {
        return Err(FileServiceError::other(format!(
            "Cannot preview '{}': path reconstruction did not resolve the parent directory. Re-import.",
            entry.path
        )));
    }

    let candidates: Vec<crate::file_service::viewer::PreviewPartitionCandidate> =
        match expected_partition_index {
            Some(expected) => partitions
                .iter()
                .filter(|partition| {
                    partition.partition_index as usize == expected
                        && partition.status != "EncryptedBitLocker"
                })
                .map(
                    |partition| crate::file_service::viewer::PreviewPartitionCandidate {
                        partition_index: partition.partition_index as usize,
                        filesystem_kind: partition
                            .filesystem
                            .as_deref()
                            .unwrap_or(&partition.kind_label)
                            .to_string(),
                        offset: partition.offset,
                    },
                )
                .collect(),
            None => partitions
                .iter()
                .filter(|partition| partition.status != "EncryptedBitLocker")
                .map(
                    |partition| crate::file_service::viewer::PreviewPartitionCandidate {
                        partition_index: partition.partition_index as usize,
                        filesystem_kind: partition
                            .filesystem
                            .as_deref()
                            .unwrap_or(&partition.kind_label)
                            .to_string(),
                        offset: partition.offset,
                    },
                )
                .collect(),
        };

    if candidates.is_empty() {
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

    Ok(candidates)
}

pub(crate) fn raw_partition_candidates(
    source_path: &str,
    expected_partition_index: Option<usize>,
) -> Result<Vec<crate::file_service::viewer::PreviewPartitionCandidate>, FileServiceError> {
    let mut reader = RawImageReader::open(Path::new(source_path))?;
    let probe = crate::datasource_service::detect_image_filesystem(&mut reader)
        .map_err(|e| FileServiceError::other(format!("Failed to detect RAW filesystem: {e}")))?;
    if probe.candidates.is_empty() {
        if let Some(candidate) = direct_exfat_raw_partition_candidate(source_path)? {
            if expected_partition_index.is_none_or(|expected| expected == candidate.partition_index)
            {
                return Ok(vec![candidate]);
            }
        }

        return Err(FileServiceError::other(
            "No supported filesystem detected in RAW image",
        ));
    }

    let index_map =
        crate::datasource_service::assign_effective_partition_indices(&probe.candidates);
    let mut candidates = Vec::new();
    for (candidate_pos, candidate) in probe.candidates.iter().enumerate() {
        let partition_index = crate::datasource_service::effective_partition_index(
            candidate,
            candidate_pos,
            &index_map,
        );
        if expected_partition_index.is_some_and(|expected| partition_index != expected) {
            continue;
        }

        let filesystem_kind = match candidate.kind {
            crate::datasource_service::ImageFilesystemKind::Ntfs => "NTFS",
            crate::datasource_service::ImageFilesystemKind::Fat => "FAT",
            crate::datasource_service::ImageFilesystemKind::BitLocker => continue,
        };
        candidates.push(crate::file_service::viewer::PreviewPartitionCandidate {
            partition_index,
            filesystem_kind: filesystem_kind.to_string(),
            offset: candidate.offset,
        });
    }

    let mut exfat_reader = RawImageReader::open(Path::new(source_path))?;
    for partition in &probe.partitions {
        if expected_partition_index.is_some_and(|expected| partition.index != expected) {
            continue;
        }
        if candidates
            .iter()
            .any(|candidate| candidate.partition_index == partition.index)
        {
            continue;
        }
        if !crate::file_service::viewer::looks_like_exfat_boot_sector(
            &mut exfat_reader,
            partition.offset,
        )? {
            continue;
        }

        candidates.push(crate::file_service::viewer::PreviewPartitionCandidate {
            partition_index: partition.index,
            filesystem_kind: "EXFAT".to_string(),
            offset: partition.offset,
        });
    }

    if candidates.is_empty() {
        return Err(FileServiceError::other(match expected_partition_index {
            Some(expected) => {
                format!("Partition index {expected} not found or is unsupported.")
            }
            None => "No supported filesystem detected in RAW image".to_string(),
        }));
    }

    Ok(candidates)
}

pub(crate) fn direct_exfat_raw_partition_candidate(
    source_path: &str,
) -> Result<Option<crate::file_service::viewer::PreviewPartitionCandidate>, FileServiceError> {
    let mut reader = RawImageReader::open(Path::new(source_path))?;
    if !crate::file_service::viewer::looks_like_exfat_boot_sector(&mut reader, 0)? {
        return Ok(None);
    }

    Ok(Some(
        crate::file_service::viewer::PreviewPartitionCandidate {
            partition_index: 0,
            filesystem_kind: "EXFAT".to_string(),
            offset: 0,
        },
    ))
}
