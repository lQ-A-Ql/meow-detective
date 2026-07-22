use persistence_sqlite::repositories::deleted_recovery_repo::{
    DeletedRecoveryAggregate, DeletedRecoveryRecord, RecoveryIssueRecord, RecoveryRangeRecord,
    RecoveryScanRecord,
};

use super::identity::{sha256_hex, stable_id};
use super::source::RecoveryTarget;
use super::DeletedRecoveryError;

const PARSER_VERSION: &str = "xfs-log-v2";

pub(super) fn scan_xfs(
    data_source_id: &str,
    target: &RecoveryTarget,
    reader: Box<dyn evidence_core::EvidenceReader>,
    filesystem_offset: u64,
    started_at: String,
) -> Result<DeletedRecoveryAggregate, DeletedRecoveryError> {
    let filesystem = fs_xfs::XfsReader::open(reader, filesystem_offset)?;
    let snapshot = filesystem
        .read_internal_log_snapshot(fs_xfs::log::XFS_LOG_MAX_SNAPSHOT_BYTES)
        .map_err(map_xfs_error)?;
    let snapshot_identity = sha256_hex(&snapshot.bytes);
    let analysis =
        fs_xfs::log::analyze_log_snapshot(&snapshot, fs_xfs::log::XfsLogParseLimits::default())
            .map_err(map_xfs_error)?;
    let mut warnings = Vec::new();
    if !snapshot.complete {
        warnings.push(format!(
            "XFS log snapshot reached the {} byte safety limit; later records were not scanned",
            snapshot.byte_limit
        ));
    }
    let issues = analysis
        .issues
        .iter()
        .enumerate()
        .map(|(ordinal, issue)| issue_record(ordinal, issue))
        .collect::<Vec<_>>();
    if !issues.is_empty() {
        warnings.push(format!(
            "{} XFS log record or operation issue(s) were retained for review",
            issues.len()
        ));
    }
    let transaction_count = analysis.transactions.len() as u64;
    let recoveries = analysis
        .deleted_file_candidates
        .into_iter()
        .filter_map(|candidate| {
            deleted_candidate_record(
                data_source_id,
                target.partition.partition_index,
                &snapshot_identity,
                &snapshot.bytes,
                candidate,
            )
        })
        .collect::<Vec<_>>();
    if recoveries.is_empty() {
        warnings.push(
            "No XFS log item proved both inode identity and deletion state; ordinary metadata updates were not treated as deleted files"
                .to_string(),
        );
    }
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
            filesystem_type: "xfs".to_string(),
            filesystem_uuid: Some(hex::encode(snapshot.geometry.fs_uuid)),
            parser_version: PARSER_VERSION.to_string(),
            log_kind: "internal_log".to_string(),
            snapshot_identity_sha256: snapshot_identity,
            state: if snapshot.complete && issues.is_empty() {
                "complete".to_string()
            } else {
                "partial".to_string()
            },
            transaction_count,
            candidate_count: recoveries.len() as u64,
            warnings,
            started_at,
            completed_at: chrono::Utc::now().to_rfc3339(),
        },
        recoveries,
        issues,
    })
}

fn deleted_candidate_record(
    data_source_id: &str,
    partition_index: u32,
    snapshot_identity: &str,
    snapshot: &[u8],
    candidate: fs_xfs::log::XfsDeletedFileCandidate,
) -> Option<DeletedRecoveryRecord> {
    let inode = candidate.inode;
    let cycle = candidate.record_lsn >> 32;
    let inode_string = inode.to_string();
    let recovery_id = stable_id(
        "recovery",
        &[
            data_source_id,
            &partition_index.to_string(),
            snapshot_identity,
            &inode_string,
            &candidate.record_lsn.to_string(),
            &candidate.operation_index.to_string(),
        ],
    );
    let (recovery_method, proof_warning) = match candidate.proof {
        fs_xfs::log::XfsDeletionProof::InodeCoreNlinkZero => (
            "xfs_logged_inode_nlink_zero",
            "The XFS log proves nlink=0 for this inode, but retained file extents remain unverified",
        ),
    };
    Some(DeletedRecoveryRecord {
        id: recovery_id,
        inode: inode_string,
        original_path: None,
        entry_type: None,
        mode: None,
        mft_sequence: None,
        deleted_at_unix: None,
        declared_size: 0,
        recoverable_bytes: 0,
        completeness: "metadata_only".to_string(),
        recovery_method: recovery_method.to_string(),
        confidence: 0.85,
        allocation_state: "unverified".to_string(),
        transaction_id: None,
        log_sequence: None,
        log_cycle: Some(cycle),
        content_sha256: None,
        warnings: vec![proof_warning.to_string()],
        ranges: provenance_ranges(snapshot, &candidate.provenance)?,
    })
}

fn provenance_ranges(
    snapshot: &[u8],
    spans: &[fs_xfs::log::XfsLogSourceSpan],
) -> Option<Vec<RecoveryRangeRecord>> {
    spans
        .iter()
        .enumerate()
        .map(|(ordinal, span)| {
            let start = usize::try_from(span.snapshot_offset).ok()?;
            let length = usize::try_from(span.length).ok()?;
            let end = start.checked_add(length)?;
            let bytes = snapshot.get(start..end)?;
            Some(RecoveryRangeRecord {
                ordinal: u32::try_from(ordinal).ok()?,
                range_role: "metadata".to_string(),
                source_kind: "filesystem".to_string(),
                logical_offset: span.snapshot_offset,
                source_offset: span.source_offset,
                physical_offset: None,
                length: span.length,
                allocation_state: "unverified".to_string(),
                sha256: Some(sha256_hex(bytes)),
            })
        })
        .collect()
}

fn issue_record(ordinal: usize, issue: &fs_xfs::log::XfsLogIssue) -> RecoveryIssueRecord {
    let (severity, code) = match issue.kind {
        fs_xfs::log::XfsLogIssueKind::ExternalLogUnsupported => {
            ("warning", "xfs.external_log_unsupported")
        }
        fs_xfs::log::XfsLogIssueKind::InvalidGeometry => ("error", "xfs.invalid_geometry"),
        fs_xfs::log::XfsLogIssueKind::InvalidRecord => ("error", "xfs.invalid_record"),
        fs_xfs::log::XfsLogIssueKind::TruncatedRecord => ("error", "xfs.truncated_record"),
        fs_xfs::log::XfsLogIssueKind::CycleMismatch => ("warning", "xfs.cycle_mismatch"),
        fs_xfs::log::XfsLogIssueKind::ChecksumMismatch => ("error", "xfs.checksum_mismatch"),
        fs_xfs::log::XfsLogIssueKind::InvalidOperation => ("error", "xfs.invalid_operation"),
        fs_xfs::log::XfsLogIssueKind::DeletionEvidenceUnavailable => {
            ("warning", "xfs.deletion_evidence_unavailable")
        }
        fs_xfs::log::XfsLogIssueKind::LimitReached => ("warning", "xfs.limit_reached"),
    };
    RecoveryIssueRecord {
        ordinal: ordinal as u32,
        severity: severity.to_string(),
        code: code.to_string(),
        message: issue.message.clone(),
        log_offset: issue
            .log_block
            .map(|block| block.saturating_mul(fs_xfs::log::XLOG_BASIC_BLOCK_SIZE as u64)),
        sequence: None,
    }
}

fn map_xfs_error(error: fs_xfs::log::XfsLogError) -> DeletedRecoveryError {
    match error {
        fs_xfs::log::XfsLogError::Unsupported(issue) => {
            DeletedRecoveryError::Unsupported(issue.to_string())
        }
        fs_xfs::log::XfsLogError::Io(error) => DeletedRecoveryError::Io(error),
        other => DeletedRecoveryError::Parser(other.to_string()),
    }
}

#[cfg(test)]
#[path = "../../tests/unit/deleted_recovery/xfs.rs"]
mod tests;
