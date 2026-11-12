use persistence_sqlite::repositories::deleted_recovery_repo::{
    DeletedRecoveryAggregate, DeletedRecoveryRecord, RecoveryIssueRecord, RecoveryRangeRecord,
    RecoveryScanRecord,
};

use super::identity::{sha256_hex, stable_id};
use super::source::RecoveryTarget;
use super::DeletedRecoveryError;

const MAX_EXT4_JOURNAL_SNAPSHOT_BYTES: usize = 128 * 1024 * 1024;
const PARSER_VERSION: &str = "ext4-jbd2-v1";

pub(super) fn scan_ext4(
    data_source_id: &str,
    target: &RecoveryTarget,
    reader: Box<dyn evidence_core::EvidenceReader>,
    filesystem_offset: u64,
    started_at: String,
) -> Result<DeletedRecoveryAggregate, DeletedRecoveryError> {
    let filesystem = fs_ext4::Ext4Reader::open(reader, filesystem_offset)?;
    let journal = filesystem
        .read_internal_journal(MAX_EXT4_JOURNAL_SNAPSHOT_BYTES)
        .map_err(map_journal_error)?;
    let snapshot_identity = sha256_hex(&journal);
    let scan = fs_ext4::journal::parse_journal(&journal).map_err(map_journal_error)?;
    let candidates = fs_ext4::journal::recover_deleted_inodes(&filesystem, &journal)
        .map_err(map_journal_error)?;
    let mut warnings = Vec::new();
    let mut issues = Vec::new();
    if let Some(incomplete) = &scan.incomplete_transaction {
        warnings.push(format!(
            "JBD2 transaction {} is incomplete; candidates after the stop point were omitted",
            incomplete.sequence
        ));
        issues.push(RecoveryIssueRecord {
            ordinal: 0,
            severity: "warning".to_string(),
            code: "jbd2.incomplete_transaction".to_string(),
            message: incomplete.reason.clone(),
            log_offset: Some(
                u64::from(incomplete.stopped_at_journal_block)
                    .saturating_mul(u64::from(scan.superblock.block_size)),
            ),
            sequence: Some(u64::from(incomplete.sequence)),
        });
    }
    let recoveries = candidates
        .into_iter()
        .map(|candidate| {
            candidate_record(
                data_source_id,
                target.partition.partition_index,
                &snapshot_identity,
                &journal,
                candidate,
            )
        })
        .collect::<Result<Vec<_>, _>>()?;
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
            filesystem_type: "ext4".to_string(),
            filesystem_uuid: None,
            parser_version: PARSER_VERSION.to_string(),
            log_kind: "internal_journal".to_string(),
            snapshot_identity_sha256: snapshot_identity,
            state: if scan.incomplete_transaction.is_some() {
                "partial".to_string()
            } else {
                "complete".to_string()
            },
            transaction_count: scan.transactions.len() as u64,
            candidate_count: recoveries.len() as u64,
            warnings,
            started_at,
            completed_at: chrono::Utc::now().to_rfc3339(),
        },
        recoveries,
        issues,
    })
}

fn candidate_record(
    data_source_id: &str,
    partition_index: u32,
    snapshot_identity: &str,
    journal: &[u8],
    candidate: fs_ext4::journal::DeletedInodeCandidate,
) -> Result<DeletedRecoveryRecord, DeletedRecoveryError> {
    let inode = candidate.inode.to_string();
    let sequence = candidate.transaction_sequence.to_string();
    let payload_block = candidate.payload_journal_block.to_string();
    let recovery_id = stable_id(
        "recovery",
        &[
            data_source_id,
            &partition_index.to_string(),
            snapshot_identity,
            &inode,
            &sequence,
            &payload_block,
        ],
    );
    let mut warnings = candidate_warnings(&candidate);
    let (ranges, persisted_recoverable_bytes) =
        recovery_ranges(journal, &candidate, &mut warnings)?;
    let (completeness, allocation_state, content_hashes) = recovery_claim(
        candidate.declared_size,
        candidate.recoverable_bytes,
        persisted_recoverable_bytes,
        candidate.content_mapping.content_md5.clone(),
        candidate.content_mapping.content_sha1.clone(),
        candidate.content_mapping.content_sha256.clone(),
        candidate.content_mapping.data_allocation_state,
    );
    match completeness.as_str() {
        "metadata_only" => warnings.push(
            "File content was not recovered; only deleted inode metadata is available".to_string(),
        ),
        "partial" => warnings.push(
            "Only allocation-verified free ranges were recovered; the logical file remains incomplete"
                .to_string(),
        ),
        "complete" => {}
        _ => unreachable!("recovery_claim returns a closed completeness set"),
    }
    Ok(DeletedRecoveryRecord {
        id: recovery_id,
        inode,
        original_path: None,
        entry_type: Some(
            match candidate.kind {
                fs_ext4::journal::DeletedInodeKind::RegularFile => "file",
                fs_ext4::journal::DeletedInodeKind::Directory => "directory",
                fs_ext4::journal::DeletedInodeKind::SymbolicLink => "symlink",
            }
            .to_string(),
        ),
        mode: Some(candidate.mode),
        mft_sequence: None,
        deleted_at_unix: Some(u64::from(candidate.deletion_time)),
        declared_size: candidate.declared_size,
        recoverable_bytes: persisted_recoverable_bytes,
        completeness,
        recovery_method: candidate.recovery_method,
        confidence: candidate.confidence,
        allocation_state,
        transaction_id: Some(sequence),
        log_sequence: Some(u64::from(candidate.transaction_sequence)),
        log_cycle: None,
        content_md5: content_hashes.as_ref().map(|hashes| hashes.0.clone()),
        content_sha1: content_hashes.as_ref().map(|hashes| hashes.1.clone()),
        content_sha256: content_hashes.map(|hashes| hashes.2),
        warnings,
        ranges,
    })
}

fn candidate_warnings(candidate: &fs_ext4::journal::DeletedInodeCandidate) -> Vec<String> {
    let mut warnings = Vec::new();
    if !candidate.journal_checksum_verified {
        warnings.push("The journal mapping was not protected by JBD2 v2/v3 checksums".to_string());
    }
    if !candidate.inode_checksum_verified {
        warnings.push("The recovered inode checksum has not been verified".to_string());
    }
    if let Some(issue) = candidate.content_mapping.issue.as_deref() {
        warnings.push(issue.to_string());
    }
    warnings
}

fn recovery_ranges(
    journal: &[u8],
    candidate: &fs_ext4::journal::DeletedInodeCandidate,
    warnings: &mut Vec<String>,
) -> Result<(Vec<RecoveryRangeRecord>, u64), DeletedRecoveryError> {
    let source_offset = candidate.journal_source_offset;
    let source_length = u64::from(candidate.journal_source_length);
    let start = usize::try_from(source_offset)
        .map_err(|_| DeletedRecoveryError::Parser("JBD2 provenance offset is too large".into()))?;
    let length = usize::try_from(source_length)
        .map_err(|_| DeletedRecoveryError::Parser("JBD2 provenance length is too large".into()))?;
    let end = start
        .checked_add(length)
        .filter(|end| *end <= journal.len())
        .ok_or_else(|| DeletedRecoveryError::Parser("JBD2 provenance range is invalid".into()))?;
    append_content_range_warnings(&candidate.content_mapping.ranges, warnings);
    let mut ranges = vec![RecoveryRangeRecord {
        ordinal: 0,
        range_role: "metadata".to_string(),
        source_kind: "journal".to_string(),
        logical_offset: 0,
        source_offset,
        physical_offset: None,
        length: source_length,
        allocation_state: "unverified".to_string(),
        sha256: Some(sha256_hex(&journal[start..end])),
    }];
    for range in &candidate.content_mapping.ranges {
        append_content_range(&mut ranges, range, warnings)?;
    }
    let recoverable_bytes = ranges
        .iter()
        .filter(|range| range.range_role == "content")
        .try_fold(0u64, |total, range| total.checked_add(range.length))
        .ok_or_else(|| DeletedRecoveryError::Parser("recoverable byte count overflows".into()))?;
    Ok((ranges, recoverable_bytes))
}

fn append_content_range(
    ranges: &mut Vec<RecoveryRangeRecord>,
    range: &fs_ext4::journal::DeletedContentRange,
    warnings: &mut Vec<String>,
) -> Result<(), DeletedRecoveryError> {
    if range.kind != fs_ext4::journal::DeletedContentRangeKind::RecoverableData {
        return Ok(());
    }
    let Some(source_offset) = range.filesystem_source_offset else {
        warnings.push("A recoverable ext4 range was missing its source offset".to_string());
        return Ok(());
    };
    let Some(hash) = range.sha256.clone() else {
        warnings.push("A recoverable ext4 range was missing its SHA-256 digest".to_string());
        return Ok(());
    };
    let ordinal = u32::try_from(ranges.len())
        .map_err(|_| DeletedRecoveryError::Parser("content range ordinal overflows".into()))?;
    ranges.push(RecoveryRangeRecord {
        ordinal,
        range_role: "content".to_string(),
        source_kind: "filesystem".to_string(),
        logical_offset: range.logical_offset,
        source_offset,
        physical_offset: None,
        length: range.length,
        allocation_state: "free".to_string(),
        sha256: Some(hash),
    });
    Ok(())
}

fn recovery_claim(
    declared_size: u64,
    mapped_recoverable_bytes: u64,
    persisted_recoverable_bytes: u64,
    content_md5: Option<String>,
    content_sha1: Option<String>,
    content_sha256: Option<String>,
    data_allocation_state: fs_ext4::journal::RecoveryAllocationState,
) -> (String, String, Option<(String, String, String)>) {
    let allocation_state = match data_allocation_state {
        fs_ext4::journal::RecoveryAllocationState::Unverified => "unverified",
        fs_ext4::journal::RecoveryAllocationState::Free => "free",
        fs_ext4::journal::RecoveryAllocationState::Allocated => "allocated",
        fs_ext4::journal::RecoveryAllocationState::Mixed => "partially_overwritten",
    }
    .to_string();
    if mapped_recoverable_bytes != persisted_recoverable_bytes {
        return ("metadata_only".to_string(), allocation_state, None);
    }
    let content_hashes = content_md5
        .zip(content_sha1)
        .zip(content_sha256)
        .map(|((md5, sha1), sha256)| (md5, sha1, sha256));
    if persisted_recoverable_bytes == declared_size && content_hashes.is_some() {
        return ("complete".to_string(), "free".to_string(), content_hashes);
    }
    if persisted_recoverable_bytes > 0 && persisted_recoverable_bytes < declared_size {
        return ("partial".to_string(), allocation_state, None);
    }
    ("metadata_only".to_string(), allocation_state, None)
}

fn append_content_range_warnings(
    ranges: &[fs_ext4::journal::DeletedContentRange],
    warnings: &mut Vec<String>,
) {
    for (kind, label) in [
        (
            fs_ext4::journal::DeletedContentRangeKind::AllocatedData,
            "currently allocated",
        ),
        (
            fs_ext4::journal::DeletedContentRangeKind::UnreadableData,
            "unreadable",
        ),
        (fs_ext4::journal::DeletedContentRangeKind::Sparse, "sparse"),
        (
            fs_ext4::journal::DeletedContentRangeKind::Unwritten,
            "unwritten",
        ),
    ] {
        let bytes = ranges
            .iter()
            .filter(|range| range.kind == kind)
            .fold(0u64, |total, range| total.saturating_add(range.length));
        if bytes > 0 {
            warnings.push(format!(
                "{bytes} byte(s) are {label} and were not claimed as recovered content"
            ));
        }
    }
}

fn map_journal_error(error: fs_ext4::journal::JournalError) -> DeletedRecoveryError {
    match error {
        fs_ext4::journal::JournalError::Unsupported(message) => {
            DeletedRecoveryError::Unsupported(message)
        }
        fs_ext4::journal::JournalError::Io(error) => DeletedRecoveryError::Io(error),
        other => DeletedRecoveryError::Parser(other.to_string()),
    }
}

#[cfg(test)]
#[path = "../../tests/unit/deleted_recovery/ext4.rs"]
mod tests;
