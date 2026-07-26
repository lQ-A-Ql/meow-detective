use persistence_sqlite::repositories::deleted_recovery_repo::RecoveryRangeRecord;
use sha2::{Digest, Sha256};

use super::super::identity::sha256_hex;
use super::super::DeletedRecoveryError;

const HASH_BUFFER_BYTES: usize = 1024 * 1024;
pub(super) const MAX_RECOVERY_RANGE_BYTES: u64 = 8 * 1024 * 1024;

pub(super) struct NtfsContentAccumulator {
    pub(super) ranges: Vec<RecoveryRangeRecord>,
    full_hasher: Sha256,
    full_expected_offset: u64,
    full_coverage: bool,
}

impl NtfsContentAccumulator {
    pub(super) fn new() -> Self {
        Self {
            ranges: Vec::new(),
            full_hasher: Sha256::new(),
            full_expected_offset: 0,
            full_coverage: true,
        }
    }
}

pub(super) fn classify_candidate_content(
    filesystem: &fs_ntfs::NtfsReader,
    bitmap: Option<&[u8]>,
    candidate: &fs_ntfs::NtfsDeletedFileRecord,
    content: &mut NtfsContentAccumulator,
    warnings: &mut Vec<String>,
) -> (&'static str, &'static str, Option<String>) {
    if candidate.is_dir {
        warnings.push(
            "Deleted directory metadata is retained; directory export is unsupported".to_string(),
        );
        return ("unverified", "metadata_only", None);
    }
    if let Some(classification) = classify_file_level_efs(candidate, warnings) {
        return classification;
    }
    if candidate.size == 0 {
        return ("free", "complete", Some(sha256_hex(&[])));
    }
    if candidate.has_attribute_list {
        warnings.push(
            "$ATTRIBUTE_LIST is present; external data extents were not reconstructed".to_string(),
        );
        return ("unverified", "metadata_only", None);
    }
    if let Err(error) = build_content_ranges(filesystem, bitmap, candidate, content, warnings) {
        warnings.push(format!(
            "Content verification failed and no bytes were claimed: {error}"
        ));
        content.ranges.clear();
        return ("unverified", "metadata_only", None);
    }
    content_claim(
        candidate.size,
        content.full_expected_offset,
        content.full_coverage,
        content.full_hasher.clone(),
        &content.ranges,
    )
}

pub(super) fn classify_file_level_efs(
    candidate: &fs_ntfs::NtfsDeletedFileRecord,
    warnings: &mut Vec<String>,
) -> Option<(&'static str, &'static str, Option<String>)> {
    if !candidate.encrypted {
        return None;
    }
    warnings.push(
        "NTFS EFS-encrypted deleted content is metadata-only without a decryption key".to_string(),
    );
    Some(("unverified", "metadata_only", None))
}

fn build_content_ranges(
    filesystem: &fs_ntfs::NtfsReader,
    bitmap: Option<&[u8]>,
    candidate: &fs_ntfs::NtfsDeletedFileRecord,
    content: &mut NtfsContentAccumulator,
    warnings: &mut Vec<String>,
) -> Result<(), DeletedRecoveryError> {
    let mut ordinal = 0u32;
    for extent in &candidate.extents {
        if extent.resident_source_offset.is_some() {
            warnings.push(
                "Resident NTFS content is stored inside an MFT record protected by update-sequence fixups; raw physical bytes were not claimed"
                    .to_string(),
            );
            content.full_coverage = false;
            continue;
        }
        if extent.compressed || extent.encrypted || extent.sparse {
            warnings.push("Compressed, encrypted, or sparse NTFS data runs were not claimed as complete content".to_string());
            content.full_coverage = false;
            continue;
        }
        let Some(bitmap) = bitmap else {
            content.full_coverage = false;
            continue;
        };
        append_extent_ranges(
            filesystem,
            bitmap,
            candidate,
            extent,
            &mut ordinal,
            content,
            warnings,
        )?;
    }
    Ok(())
}

fn append_extent_ranges(
    filesystem: &fs_ntfs::NtfsReader,
    bitmap: &[u8],
    candidate: &fs_ntfs::NtfsDeletedFileRecord,
    extent: &fs_ntfs::NtfsDataExtent,
    ordinal: &mut u32,
    content: &mut NtfsContentAccumulator,
    warnings: &mut Vec<String>,
) -> Result<(), DeletedRecoveryError> {
    let mut logical_offset = extent.logical_offset;
    for run in &extent.runs {
        let run_length = run
            .cluster_count
            .checked_mul(filesystem.cluster_size())
            .ok_or_else(|| {
                DeletedRecoveryError::Parser("NTFS data run length overflows".to_string())
            })?;
        let length = run_length.min(candidate.size.saturating_sub(logical_offset));
        if length == 0 {
            break;
        }
        if filesystem.classify_data_run(bitmap, run)? == fs_ntfs::NtfsAllocationState::Free {
            append_verified_run_ranges(
                filesystem,
                logical_offset,
                filesystem.data_run_source_offset(run)?,
                length,
                ordinal,
                content,
            )?;
        } else {
            warnings.push(format!(
                "NTFS data run at logical offset {logical_offset} is not fully free"
            ));
            content.full_coverage = false;
        }
        logical_offset = logical_offset.checked_add(run_length).ok_or_else(|| {
            DeletedRecoveryError::Parser("NTFS logical run offset overflows".to_string())
        })?;
    }
    Ok(())
}

fn append_verified_run_ranges(
    filesystem: &fs_ntfs::NtfsReader,
    logical_offset: u64,
    source_offset: u64,
    length: u64,
    ordinal: &mut u32,
    content: &mut NtfsContentAccumulator,
) -> Result<(), DeletedRecoveryError> {
    for (relative_offset, chunk_length) in bounded_range_chunks(length) {
        let chunk_logical_offset =
            logical_offset.checked_add(relative_offset).ok_or_else(|| {
                DeletedRecoveryError::Parser("NTFS logical chunk offset overflows".to_string())
            })?;
        let chunk_source_offset = source_offset.checked_add(relative_offset).ok_or_else(|| {
            DeletedRecoveryError::Parser("NTFS source chunk offset overflows".to_string())
        })?;
        let include_in_full =
            content.full_coverage && content.full_expected_offset == chunk_logical_offset;
        let sha256 = hash_source_range(
            filesystem,
            chunk_source_offset,
            chunk_length,
            include_in_full.then_some(&mut content.full_hasher),
        )?;
        if include_in_full {
            content.full_expected_offset = content
                .full_expected_offset
                .checked_add(chunk_length)
                .ok_or_else(|| {
                DeletedRecoveryError::Parser("NTFS complete-content offset overflows".to_string())
            })?;
        } else {
            content.full_coverage = false;
        }
        content.ranges.push(content_range(
            *ordinal,
            chunk_logical_offset,
            chunk_source_offset,
            chunk_length,
            sha256,
        ));
        *ordinal = ordinal.checked_add(1).ok_or_else(|| {
            DeletedRecoveryError::Parser("NTFS recovery range ordinal overflows".to_string())
        })?;
    }
    Ok(())
}

pub(super) fn bounded_range_chunks(length: u64) -> Vec<(u64, u64)> {
    let mut chunks = Vec::new();
    let mut offset = 0u64;
    while offset < length {
        let chunk_length = (length - offset).min(MAX_RECOVERY_RANGE_BYTES);
        chunks.push((offset, chunk_length));
        offset += chunk_length;
    }
    chunks
}

fn content_range(
    ordinal: u32,
    logical_offset: u64,
    source_offset: u64,
    length: u64,
    sha256: String,
) -> RecoveryRangeRecord {
    RecoveryRangeRecord {
        ordinal,
        range_role: "content".to_string(),
        source_kind: "filesystem".to_string(),
        logical_offset,
        source_offset,
        physical_offset: Some(source_offset),
        length,
        allocation_state: "free".to_string(),
        sha256: Some(sha256),
    }
}

fn hash_source_range(
    filesystem: &fs_ntfs::NtfsReader,
    source_offset: u64,
    length: u64,
    mut full_hasher: Option<&mut Sha256>,
) -> Result<String, DeletedRecoveryError> {
    let mut range_hasher = Sha256::new();
    let mut remaining = length;
    let mut offset = source_offset;
    let mut buffer = vec![0u8; HASH_BUFFER_BYTES];
    while remaining > 0 {
        let chunk_len = usize::try_from(remaining.min(HASH_BUFFER_BYTES as u64)).map_err(|_| {
            DeletedRecoveryError::Parser("NTFS hash chunk exceeds platform limits".to_string())
        })?;
        filesystem.read_source_range(offset, &mut buffer[..chunk_len])?;
        range_hasher.update(&buffer[..chunk_len]);
        if let Some(hasher) = full_hasher.as_deref_mut() {
            hasher.update(&buffer[..chunk_len]);
        }
        offset = offset.saturating_add(chunk_len as u64);
        remaining -= chunk_len as u64;
    }
    Ok(hex::encode(range_hasher.finalize()))
}

pub(super) fn content_claim(
    declared_size: u64,
    covered_size: u64,
    full_coverage: bool,
    full_hasher: Sha256,
    ranges: &[RecoveryRangeRecord],
) -> (&'static str, &'static str, Option<String>) {
    if declared_size > 0 && ranges.is_empty() {
        return ("unverified", "metadata_only", None);
    }
    let mut ordered = ranges
        .iter()
        .filter(|range| range.range_role == "content")
        .collect::<Vec<_>>();
    ordered.sort_by_key(|range| (range.logical_offset, range.ordinal));
    let mut expected = 0u64;
    for range in ordered {
        if range.logical_offset != expected {
            return ("partially_overwritten", "partial", None);
        }
        expected = match expected.checked_add(range.length) {
            Some(value) => value,
            None => return ("partially_overwritten", "partial", None),
        };
    }
    if expected != declared_size || covered_size != declared_size || !full_coverage {
        return ("partially_overwritten", "partial", None);
    }
    (
        "free",
        "complete",
        Some(hex::encode(full_hasher.finalize())),
    )
}
