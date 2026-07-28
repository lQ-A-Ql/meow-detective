use std::path::Path;

use domain::{CaseId, DataSourceId, DataSourcePlatform};
use persistence_sqlite::repositories::deleted_recovery_repo::DeletedRecoveryRepo;
use rusqlite::Connection;
use transport::dto::{DeletedRecoveryFailureDto, DeletedRecoveryRunDto, DeletedRecoveryScanDto};

use super::mapping::scan_to_dto;
use super::source::{load_source, load_targets, open_target_reader, RecoveryTarget};
use super::{DeletedRecoveryContext, DeletedRecoveryError};

pub fn run_deleted_recovery(
    case_conn: &Connection,
    case_root: &Path,
    case_id: &CaseId,
    data_source_id: &DataSourceId,
    partition_index: Option<u32>,
) -> Result<DeletedRecoveryRunDto, DeletedRecoveryError> {
    run_deleted_recovery_in_context(
        &DeletedRecoveryContext::new(case_conn, case_root, case_id, data_source_id),
        partition_index,
    )
}

pub(super) fn run_deleted_recovery_in_context(
    context: &DeletedRecoveryContext<'_>,
    partition_index: Option<u32>,
) -> Result<DeletedRecoveryRunDto, DeletedRecoveryError> {
    let ready = crate::source_db::open_ready_source_by_id(
        context.case_conn,
        context.case_root,
        context.case_id,
        context.data_source_id,
    )?;
    if !matches!(
        ready.platform,
        DataSourcePlatform::Windows | DataSourcePlatform::Linux
    ) {
        return Err(DeletedRecoveryError::Unsupported(
            "deleted recovery is available only for Windows and Linux data sources".to_string(),
        ));
    }
    let source = load_source(context.case_conn, context.data_source_id)?;
    let targets = load_targets(
        &ready.connection,
        context.data_source_id,
        ready.platform,
        partition_index,
    )?;
    let mut scans = Vec::<DeletedRecoveryScanDto>::new();
    let mut failures = Vec::<DeletedRecoveryFailureDto>::new();

    // Evidence access remains serial: E01 and LVM readers are I/O-bound and
    // parallel journal snapshots would amplify seeks and peak memory.
    for target in targets {
        let started_at = chrono::Utc::now().to_rfc3339();
        let result = open_target_reader(context, &source, &target).and_then(|(reader, offset)| {
            scan_target(
                &context.data_source_id.0,
                &target,
                reader,
                offset,
                started_at,
            )
        });
        match result {
            Ok(aggregate) => {
                DeletedRecoveryRepo::new(&ready.connection).replace_scan(&aggregate)?;
                scans.push(scan_to_dto(
                    aggregate.scan.clone(),
                    aggregate.issues.clone(),
                )?);
            }
            Err(error) => {
                tracing::warn!(
                    data_source_id = %context.data_source_id.0,
                    partition_index = target.partition.partition_index,
                    filesystem = target.filesystem_type,
                    error = %error,
                    "Filesystem deleted-recovery scan failed"
                );
                failures.push(failure(&target, &error));
            }
        }
    }
    Ok(DeletedRecoveryRunDto {
        data_source_id: context.data_source_id.0.clone(),
        scans,
        failures,
    })
}

fn scan_target(
    data_source_id: &str,
    target: &RecoveryTarget,
    reader: Box<dyn evidence_core::EvidenceReader>,
    filesystem_offset: u64,
    started_at: String,
) -> Result<
    persistence_sqlite::repositories::deleted_recovery_repo::DeletedRecoveryAggregate,
    DeletedRecoveryError,
> {
    match target.filesystem_type {
        "ext4" => super::ext4::scan_ext4(
            data_source_id,
            target,
            reader,
            filesystem_offset,
            started_at,
        ),
        "xfs" => super::xfs::scan_xfs(
            data_source_id,
            target,
            reader,
            filesystem_offset,
            started_at,
        ),
        "ntfs" => super::ntfs::scan_ntfs(
            data_source_id,
            target,
            reader,
            filesystem_offset,
            started_at,
        ),
        filesystem => Err(DeletedRecoveryError::Unsupported(format!(
            "filesystem '{filesystem}' has no journal recovery adapter"
        ))),
    }
}

fn failure(target: &RecoveryTarget, error: &DeletedRecoveryError) -> DeletedRecoveryFailureDto {
    let (code, message) = match error {
        DeletedRecoveryError::Unsupported(_) => (
            "RECOVERY_UNSUPPORTED",
            "The filesystem recovery layout is not supported by the read-only recovery adapter",
        ),
        DeletedRecoveryError::BitLockerLocked => (
            "RECOVERY_BITLOCKER_LOCKED",
            "Unlock the BitLocker volume before running deleted-file recovery",
        ),
        DeletedRecoveryError::Parser(_) => (
            "RECOVERY_PARSER_ERROR",
            "The filesystem journal could not be parsed reliably",
        ),
        DeletedRecoveryError::Io(_) => (
            "RECOVERY_IO_ERROR",
            "The filesystem journal bytes could not be read from the evidence source",
        ),
        DeletedRecoveryError::InvalidState(_) => (
            "RECOVERY_INVALID_STATE",
            "Stored partition or LVM routing metadata is invalid",
        ),
        DeletedRecoveryError::Database(_) | DeletedRecoveryError::Source(_) => (
            "RECOVERY_SOURCE_ERROR",
            "The source-local recovery store is unavailable",
        ),
        DeletedRecoveryError::NotFound { .. } => (
            "RECOVERY_SCAN_NOT_FOUND",
            "No recovery scan is available for this partition",
        ),
        DeletedRecoveryError::RecoveryNotFound { .. } => (
            "RECOVERY_NOT_FOUND",
            "The selected deleted-file recovery candidate no longer exists",
        ),
        DeletedRecoveryError::ContentUnavailable(_) => (
            "RECOVERY_CONTENT_UNAVAILABLE",
            "The recovery candidate does not contain verified file content",
        ),
        DeletedRecoveryError::InvalidRange(_) => (
            "RECOVERY_RANGE_INVALID",
            "The requested recovery range is outside the verified content",
        ),
        DeletedRecoveryError::Integrity(_) => (
            "RECOVERY_INTEGRITY_MISMATCH",
            "Recovered content no longer matches its persisted integrity metadata",
        ),
    };
    DeletedRecoveryFailureDto {
        partition_index: target.partition.partition_index,
        filesystem_type: target.filesystem_type.to_string(),
        code: code.to_string(),
        message: message.to_string(),
    }
}
