use super::super::audit::BitLockerAudit;
use super::super::{audit, BitLockerRuntimeContext};
use super::{inspect_bitlocker_volume, BitLockerServiceError};
use std::time::Duration;
const LOCK_DRAIN_TIMEOUT: Duration = Duration::from_secs(30);
use domain::{CaseId, DataSourceId};
use rusqlite::Connection;
use std::path::Path;
use transport::dto::BitLockerVolumeStatusDto;

pub fn lock_bitlocker_volume(
    case_conn: &Connection,
    case_root: &Path,
    case_id: &CaseId,
    data_source_id: &DataSourceId,
    partition_index: u32,
    runtimes: BitLockerRuntimeContext<'_>,
) -> Result<BitLockerVolumeStatusDto, BitLockerServiceError> {
    let before = inspect_bitlocker_volume(
        case_conn,
        case_root,
        case_id,
        data_source_id,
        partition_index,
        runtimes,
    )?;
    let drained = runtimes.preview_runtime.retire_source_and_drain(
        &case_id.0,
        &data_source_id.0,
        LOCK_DRAIN_TIMEOUT,
    )?;
    if !drained {
        let _ = runtimes
            .preview_runtime
            .reactivate_source(&case_id.0, &data_source_id.0);
        audit_lock(
            case_conn,
            case_id,
            data_source_id,
            partition_index,
            &before,
            "timeout",
        );
        return Err(BitLockerServiceError::DrainTimeout);
    }
    let invalidated = runtimes.bitlocker_runtime.invalidate_partition(
        &case_id.0,
        &data_source_id.0,
        partition_index as usize,
    );
    let reactivated = runtimes
        .preview_runtime
        .reactivate_source(&case_id.0, &data_source_id.0);
    invalidated?;
    reactivated?;
    audit_lock(
        case_conn,
        case_id,
        data_source_id,
        partition_index,
        &before,
        "success",
    );
    Ok(BitLockerVolumeStatusDto {
        unlocked: false,
        plaintext_filesystem: None,
        ..before
    })
}

fn audit_lock(
    case_conn: &Connection,
    case_id: &CaseId,
    data_source_id: &DataSourceId,
    partition_index: u32,
    status: &BitLockerVolumeStatusDto,
    outcome: &str,
) {
    audit::record(
        case_conn,
        BitLockerAudit {
            case_id: &case_id.0,
            data_source_id: &data_source_id.0,
            partition_index,
            metadata_fingerprint: Some(&status.metadata_fingerprint),
            operation: "lock",
            outcome,
            error_code: (outcome == "timeout").then_some("BITLOCKER_LOCK_TIMEOUT"),
            extra_details: None,
        },
    );
}
