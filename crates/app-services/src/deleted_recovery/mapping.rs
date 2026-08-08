use persistence_sqlite::repositories::deleted_recovery_repo::{
    DeletedRecoveryPageRecord, DeletedRecoveryRecord, RecoveryIssueRecord, RecoveryRangeRecord,
    RecoveryScanRecord,
};
use transport::dto::{
    DeletedFileRecoveryDto, DeletedRecoveryPageDto, DeletedRecoveryScanDto,
    RecoveryAllocationStateDto, RecoveryCompletenessDto, RecoveryIssueDto,
    RecoveryIssueSeverityDto, RecoveryProvenanceRangeDto, RecoveryScanStateDto,
};

use super::DeletedRecoveryError;

pub(super) fn page_to_dto(
    page: DeletedRecoveryPageRecord,
) -> Result<DeletedRecoveryPageDto, DeletedRecoveryError> {
    let data_source_id = page.scan.data_source_id.clone();
    let partition_index = page.scan.partition_index;
    let filesystem_type = page.scan.filesystem_type.clone();
    let filesystem_uuid = page.scan.filesystem_uuid.clone();
    let recoveries = page
        .recoveries
        .into_iter()
        .map(|record| {
            recovery_to_dto(
                record,
                &data_source_id,
                partition_index,
                &filesystem_type,
                filesystem_uuid.as_deref(),
            )
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(DeletedRecoveryPageDto {
        scan: scan_to_dto(page.scan, page.issues)?,
        recoveries,
        offset: page.offset,
        limit: page.limit,
        total: page.total,
    })
}

pub(super) fn scan_to_dto(
    scan: RecoveryScanRecord,
    issues: Vec<RecoveryIssueRecord>,
) -> Result<DeletedRecoveryScanDto, DeletedRecoveryError> {
    Ok(DeletedRecoveryScanDto {
        id: scan.id,
        data_source_id: scan.data_source_id,
        partition_index: scan.partition_index,
        filesystem_type: scan.filesystem_type,
        filesystem_uuid: scan.filesystem_uuid,
        parser_version: scan.parser_version,
        log_kind: scan.log_kind,
        snapshot_identity_sha256: scan.snapshot_identity_sha256,
        state: scan_state(&scan.state)?,
        transaction_count: scan.transaction_count,
        candidate_count: scan.candidate_count,
        warnings: scan.warnings,
        started_at: scan.started_at,
        completed_at: scan.completed_at,
        issues: issues
            .into_iter()
            .map(issue_to_dto)
            .collect::<Result<Vec<_>, _>>()?,
    })
}

pub(super) fn recovery_to_dto(
    recovery: DeletedRecoveryRecord,
    data_source_id: &str,
    partition_index: u32,
    filesystem_type: &str,
    filesystem_uuid: Option<&str>,
) -> Result<DeletedFileRecoveryDto, DeletedRecoveryError> {
    Ok(DeletedFileRecoveryDto {
        id: recovery.id,
        data_source_id: data_source_id.to_string(),
        partition_index,
        filesystem_type: filesystem_type.to_string(),
        filesystem_uuid: filesystem_uuid.map(str::to_string),
        inode: recovery.inode,
        original_path: recovery.original_path,
        entry_type: recovery.entry_type,
        mode: recovery.mode,
        mft_sequence: recovery.mft_sequence,
        deleted_at_unix: recovery.deleted_at_unix,
        declared_size: recovery.declared_size,
        recoverable_bytes: recovery.recoverable_bytes,
        completeness: completeness(&recovery.completeness)?,
        allocation_state: allocation_state(&recovery.allocation_state)?,
        recovery_method: recovery.recovery_method,
        confidence: recovery.confidence,
        transaction_id: recovery.transaction_id,
        log_sequence: recovery.log_sequence,
        log_cycle: recovery.log_cycle,
        content_md5: recovery.content_md5,
        content_sha1: recovery.content_sha1,
        content_sha256: recovery.content_sha256,
        provenance_ranges: recovery
            .ranges
            .into_iter()
            .map(range_to_dto)
            .collect::<Result<Vec<_>, _>>()?,
        warnings: recovery.warnings,
    })
}

fn range_to_dto(
    range: RecoveryRangeRecord,
) -> Result<RecoveryProvenanceRangeDto, DeletedRecoveryError> {
    Ok(RecoveryProvenanceRangeDto {
        ordinal: range.ordinal,
        range_role: range.range_role,
        source_kind: range.source_kind,
        logical_offset: range.logical_offset,
        source_offset: range.source_offset,
        physical_offset: range.physical_offset,
        length: range.length,
        allocation_state: allocation_state(&range.allocation_state)?,
        sha256: range.sha256,
    })
}

fn issue_to_dto(issue: RecoveryIssueRecord) -> Result<RecoveryIssueDto, DeletedRecoveryError> {
    Ok(RecoveryIssueDto {
        ordinal: issue.ordinal,
        severity: match issue.severity.as_str() {
            "info" => RecoveryIssueSeverityDto::Info,
            "warning" => RecoveryIssueSeverityDto::Warning,
            "error" => RecoveryIssueSeverityDto::Error,
            value => return invalid_state("issue severity", value),
        },
        code: issue.code,
        message: issue.message,
        log_offset: issue.log_offset,
        sequence: issue.sequence,
    })
}

fn scan_state(value: &str) -> Result<RecoveryScanStateDto, DeletedRecoveryError> {
    match value {
        "complete" => Ok(RecoveryScanStateDto::Complete),
        "partial" => Ok(RecoveryScanStateDto::Partial),
        "failed" => Ok(RecoveryScanStateDto::Failed),
        value => invalid_state("scan state", value),
    }
}

fn completeness(value: &str) -> Result<RecoveryCompletenessDto, DeletedRecoveryError> {
    match value {
        "metadata_only" => Ok(RecoveryCompletenessDto::MetadataOnly),
        "partial" => Ok(RecoveryCompletenessDto::Partial),
        "complete" => Ok(RecoveryCompletenessDto::Complete),
        value => invalid_state("completeness", value),
    }
}

fn allocation_state(value: &str) -> Result<RecoveryAllocationStateDto, DeletedRecoveryError> {
    match value {
        "unverified" => Ok(RecoveryAllocationStateDto::Unverified),
        "free" => Ok(RecoveryAllocationStateDto::Free),
        "allocated" => Ok(RecoveryAllocationStateDto::Allocated),
        "partially_overwritten" => Ok(RecoveryAllocationStateDto::PartiallyOverwritten),
        value => invalid_state("allocation state", value),
    }
}

fn invalid_state<T>(label: &str, value: &str) -> Result<T, DeletedRecoveryError> {
    Err(DeletedRecoveryError::InvalidState(format!(
        "stored {label} '{value}' is invalid"
    )))
}
