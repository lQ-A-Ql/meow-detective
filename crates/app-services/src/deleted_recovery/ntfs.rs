use persistence_sqlite::repositories::deleted_recovery_repo::{
    DeletedRecoveryAggregate, DeletedRecoveryRecord, RecoveryScanRecord,
};
use sha2::{Digest, Sha256};

mod content;

use super::identity::stable_id;
use super::source::RecoveryTarget;
use super::DeletedRecoveryError;
use content::{classify_candidate_content, NtfsContentAccumulator};

const PARSER_VERSION: &str = "ntfs-mft-v1";
const MAX_BITMAP_BYTES: usize = 256 * 1024 * 1024;

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
