use super::super::{BitLockerRuntimeContext, BitLockerServiceError};
use super::unlock::unlock_with;
use super::{UnlockContext, UnlockMethod};
use domain::{CaseId, DataSourceId};
use rusqlite::Connection;
use std::path::Path;
use transport::dto::BitLockerVolumeStatusDto;
use volume_bitlocker::Passphrase;
pub fn unlock_bitlocker_with_recovery_password(
    case_conn: &Connection,
    case_root: &Path,
    case_id: &CaseId,
    data_source_id: &DataSourceId,
    partition_index: u32,
    credential: Passphrase,
    runtimes: BitLockerRuntimeContext<'_>,
) -> Result<BitLockerVolumeStatusDto, BitLockerServiceError> {
    unlock_with(
        UnlockContext {
            case_conn,
            case_root,
            case_id,
            data_source_id,
            partition_index,
            runtimes,
        },
        credential,
        UnlockMethod::RecoveryPassword,
    )
}
