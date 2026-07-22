use persistence_sqlite::repositories::deleted_recovery_repo::{
    DeletedRecoveryAggregate, DeletedRecoveryRecord, RecoveryRangeRecord, RecoveryScanRecord,
};
use sha2::{Digest, Sha256};

use super::identity::{sha256_hex, stable_id};
use super::source::RecoveryTarget;
use super::DeletedRecoveryError;

const PARSER_VERSION: &str = "ntfs-mft-v1";
const MAX_BITMAP_BYTES: usize = 256 * 1024 * 1024;
const HASH_BUFFER_BYTES: usize = 1024 * 1024;

pub(super) fn scan_ntfs(
    data_source_id: &str,
    target: &RecoveryTarget,
    reader: Box<dyn evidence_core::EvidenceReader>,
    filesystem_offset: u64,
    started_at: String,
) -> Result<DeletedRecoveryAggregate, DeletedRecoveryError> {
    let filesystem = fs_ntfs::NtfsReader::open(reader, filesystem_offset)?;
    let candidates = filesystem.scan_deleted_file_records()?;
    let bitmap = filesystem.read_volume_bitmap(MAX_BITMAP_BYTES).ok();
    let mut warnings =
        vec!["NTFS $LogFile, $UsnJrnl and MFT slack are not replayed by this scanner".to_string()];
    if bitmap.is_none() {
        warnings.push(
            "NTFS $Bitmap could not be loaded within the recovery safety limit; content candidates were not claimed"
                .to_string(),
        );
    }
    let mut recoveries = Vec::with_capacity(candidates.len());
    for candidate in &candidates {
        match candidate_record(
            &filesystem,
            bitmap.as_deref(),
            data_source_id,
            target,
            candidate,
        ) {
            Ok(recovery) => recoveries.push(recovery),
            Err(error) => {
                warnings.push(format!(
                    "MFT record {} was retained as metadata-only after content verification failed: {error}",
                    candidate.record_number
                ));
                recoveries.push(metadata_only_record(
                    data_source_id,
                    target,
                    candidate,
                    error.to_string(),
                ));
            }
        }
    }
    let snapshot_identity = snapshot_identity(&filesystem, &recoveries);
    let scan_id = stable_id(
        "recovery-scan",
        &[
            data_source_id,
            &target.partition.partition_index.to_string(),
            PARSER_VERSION,
            &snapshot_identity,
        ],
    );
    Ok(DeletedRecoveryAggregate {
        scan: RecoveryScanRecord {
            id: scan_id,
            data_source_id: data_source_id.to_string(),
            partition_index: target.partition.partition_index,
            filesystem_type: "ntfs".to_string(),
            filesystem_uuid: Some(format!("{:016x}", filesystem.volume_serial())),
            parser_version: PARSER_VERSION.to_string(),
            log_kind: "internal_log".to_string(),
            snapshot_identity_sha256: snapshot_identity,
            state: "complete".to_string(),
            transaction_count: 0,
            candidate_count: recoveries.len() as u64,
            warnings,
            started_at,
            completed_at: chrono::Utc::now().to_rfc3339(),
        },
        recoveries,
        issues: Vec::new(),
    })
}

fn metadata_only_record(
    data_source_id: &str,
    target: &RecoveryTarget,
    candidate: &fs_ntfs::NtfsDeletedFileRecord,
    failure: String,
) -> DeletedRecoveryRecord {
    let inode = candidate.record_number.to_string();
    let recovery_id = stable_id(
        "recovery",
        &[
            data_source_id,
            &target.partition.partition_index.to_string(),
            &inode,
            &candidate.sequence_number.to_string(),
            PARSER_VERSION,
        ],
    );
    DeletedRecoveryRecord {
        id: recovery_id,
        inode,
        original_path: Some(candidate.name.clone()),
        entry_type: Some(
            if candidate.is_dir {
                "directory"
            } else {
                "file"
            }
            .to_string(),
        ),
        mode: None,
        mft_sequence: Some(candidate.sequence_number),
        deleted_at_unix: None,
        declared_size: candidate.size,
        recoverable_bytes: 0,
        completeness: "metadata_only".to_string(),
        recovery_method: "ntfs_mft_metadata".to_string(),
        confidence: 0.5,
        allocation_state: "unverified".to_string(),
        transaction_id: None,
        log_sequence: None,
        log_cycle: None,
        content_sha256: None,
        warnings: vec![
            format!(
                "Original parent MFT reference {} was not reconstructed into a full path",
                candidate.parent_ref
            ),
            format!("Content verification failed: {failure}"),
            "Only deleted MFT metadata is available; the candidate cannot be exported".to_string(),
        ],
        ranges: Vec::new(),
    }
}

fn candidate_record(
    filesystem: &fs_ntfs::NtfsReader,
    bitmap: Option<&[u8]>,
    data_source_id: &str,
    target: &RecoveryTarget,
    candidate: &fs_ntfs::NtfsDeletedFileRecord,
) -> Result<DeletedRecoveryRecord, DeletedRecoveryError> {
    let mut warnings = vec![format!(
        "Original parent MFT reference {} was not reconstructed into a full path",
        candidate.parent_ref
    )];
    let mut content = NtfsContentAccumulator::new();
    let (allocation_state, completeness, content_sha256) =
        classify_candidate_content(filesystem, bitmap, candidate, &mut content, &mut warnings);

    if completeness == "metadata_only" {
        warnings.push(
            "Only deleted MFT metadata is available; the candidate cannot be exported".to_string(),
        );
    } else if completeness == "partial" {
        warnings.push(
            "Only allocation-verified free ranges are readable; complete export is disabled"
                .to_string(),
        );
    }
    let inode = candidate.record_number.to_string();
    let recovery_id = stable_id(
        "recovery",
        &[
            data_source_id,
            &target.partition.partition_index.to_string(),
            &inode,
            &candidate.sequence_number.to_string(),
            PARSER_VERSION,
        ],
    );
    Ok(DeletedRecoveryRecord {
        id: recovery_id,
        inode,
        original_path: Some(candidate.name.clone()),
        entry_type: Some(
            if candidate.is_dir {
                "directory"
            } else {
                "file"
            }
            .to_string(),
        ),
        mode: None,
        mft_sequence: Some(candidate.sequence_number),
        deleted_at_unix: None,
        declared_size: candidate.size,
        recoverable_bytes: content
            .ranges
            .iter()
            .filter(|range| range.range_role == "content")
            .map(|range| range.length)
            .try_fold(0u64, |total, length| total.checked_add(length))
            .ok_or_else(|| {
                DeletedRecoveryError::Parser("NTFS recoverable byte count overflows".to_string())
            })?,
        completeness: completeness.to_string(),
        recovery_method: if content.ranges.is_empty() {
            "ntfs_mft_metadata".to_string()
        } else {
            "ntfs_mft_data_runs_bitmap".to_string()
        },
        confidence: match completeness {
            "complete" => 0.92,
            "partial" => 0.68,
            _ => 0.5,
        },
        allocation_state: allocation_state.to_string(),
        transaction_id: None,
        log_sequence: None,
        log_cycle: None,
        content_sha256,
        warnings,
        ranges: content.ranges,
    })
}

fn classify_candidate_content(
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

struct NtfsContentAccumulator {
    ranges: Vec<RecoveryRangeRecord>,
    full_hasher: Sha256,
    full_expected_offset: u64,
    full_coverage: bool,
}

impl NtfsContentAccumulator {
    fn new() -> Self {
        Self {
            ranges: Vec::new(),
            full_hasher: Sha256::new(),
            full_expected_offset: 0,
            full_coverage: true,
        }
    }
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
            let state = filesystem.classify_data_run(bitmap, run)?;
            if state == fs_ntfs::NtfsAllocationState::Free {
                let source_offset = filesystem.data_run_source_offset(run)?;
                let include_in_full =
                    content.full_coverage && content.full_expected_offset == logical_offset;
                let sha256 = hash_source_range(
                    filesystem,
                    source_offset,
                    length,
                    include_in_full.then_some(&mut content.full_hasher),
                )?;
                if include_in_full {
                    content.full_expected_offset =
                        content.full_expected_offset.saturating_add(length);
                } else {
                    content.full_coverage = false;
                }
                content.ranges.push(content_range(
                    ordinal,
                    logical_offset,
                    source_offset,
                    length,
                    sha256,
                ));
                ordinal = ordinal.saturating_add(1);
            } else {
                warnings.push(format!(
                    "NTFS data run at logical offset {logical_offset} is not fully free"
                ));
                content.full_coverage = false;
            }
            logical_offset = logical_offset.saturating_add(run_length);
        }
    }
    Ok(())
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

fn content_claim(
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

fn snapshot_identity(
    filesystem: &fs_ntfs::NtfsReader,
    recoveries: &[DeletedRecoveryRecord],
) -> String {
    let mut hasher = Sha256::new();
    hasher.update(filesystem.volume_serial().to_le_bytes());
    hasher.update((recoveries.len() as u64).to_le_bytes());
    for recovery in recoveries {
        hasher.update(recovery.inode.as_bytes());
        hasher.update(recovery.mft_sequence.unwrap_or_default().to_le_bytes());
        hasher.update(recovery.declared_size.to_le_bytes());
        if let Some(hash) = &recovery.content_sha256 {
            hasher.update(hash.as_bytes());
        }
    }
    hex::encode(hasher.finalize())
}

#[cfg(test)]
#[path = "../../tests/unit/deleted_recovery/ntfs.rs"]
mod tests;
