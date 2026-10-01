use super::super::BitLockerRuntimeContext;
use domain::{CaseId, DataSourceId};
use rusqlite::Connection;
use std::path::Path;
use volume_bitlocker::{
    unlock_volume_with_password as unlock_password,
    unlock_volume_with_recovery_password as unlock_recovery, Passphrase, VerifiedUnlock,
};
pub(in crate::bitlocker_service) struct UnlockContext<'a> {
    pub(in crate::bitlocker_service) case_conn: &'a Connection,
    pub(in crate::bitlocker_service) case_root: &'a Path,
    pub(in crate::bitlocker_service) case_id: &'a CaseId,
    pub(in crate::bitlocker_service) data_source_id: &'a DataSourceId,
    pub(in crate::bitlocker_service) partition_index: u32,
    pub(in crate::bitlocker_service) runtimes: BitLockerRuntimeContext<'a>,
}

#[derive(Clone, Copy)]
pub(in crate::bitlocker_service) enum UnlockMethod {
    Password,
    RecoveryPassword,
    MemoryImage,
}

impl UnlockMethod {
    pub(in crate::bitlocker_service) fn apply(
        self,
        window: &mut evidence_core::PartitionWindowReader,
        credential: &Passphrase,
    ) -> volume_bitlocker::Result<VerifiedUnlock> {
        match self {
            Self::Password => unlock_password(window, credential),
            Self::RecoveryPassword => unlock_recovery(window, credential),
            Self::MemoryImage => unreachable!("memory recovery does not use a passphrase"),
        }
    }

    pub(in crate::bitlocker_service) fn audit_name(self) -> &'static str {
        match self {
            Self::Password => "password",
            Self::RecoveryPassword => "recoveryPassword",
            Self::MemoryImage => "memoryImage",
        }
    }
}
