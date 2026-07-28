use std::path::Path;
use std::sync::Arc;

use domain::{CaseId, DataSourceId};
use rusqlite::Connection;

use crate::bitlocker_runtime::BitLockerUnlockRegistry;
use transport::dto::{
    DeletedRecoveryContentRangeDto, DeletedRecoveryExportDto, DeletedRecoveryRunDto,
};

use super::DeletedRecoveryError;

/// Source-bound capabilities shared by deleted-recovery operations.
///
/// BitLocker keys remain in the process registry. This context only retains a
/// reference-counted capability and never persists credentials or plaintext.
pub struct DeletedRecoveryContext<'a> {
    pub(super) case_conn: &'a Connection,
    pub(super) case_root: &'a Path,
    pub(super) case_id: &'a CaseId,
    pub(super) data_source_id: &'a DataSourceId,
    pub(super) bitlocker_runtime: Option<Arc<BitLockerUnlockRegistry>>,
}

impl<'a> DeletedRecoveryContext<'a> {
    #[must_use]
    pub fn new(
        case_conn: &'a Connection,
        case_root: &'a Path,
        case_id: &'a CaseId,
        data_source_id: &'a DataSourceId,
    ) -> Self {
        Self {
            case_conn,
            case_root,
            case_id,
            data_source_id,
            bitlocker_runtime: None,
        }
    }

    #[must_use]
    pub fn with_bitlocker_runtime(mut self, runtime: Arc<BitLockerUnlockRegistry>) -> Self {
        self.bitlocker_runtime = Some(runtime);
        self
    }

    pub fn run(
        &self,
        partition_index: Option<u32>,
    ) -> Result<DeletedRecoveryRunDto, DeletedRecoveryError> {
        super::scan::run_deleted_recovery_in_context(self, partition_index)
    }

    pub fn read_range(
        &self,
        recovery_id: &str,
        offset: u64,
        length: u32,
    ) -> Result<DeletedRecoveryContentRangeDto, DeletedRecoveryError> {
        super::content::read_deleted_recovery_range_in_context(self, recovery_id, offset, length)
    }

    pub fn export(
        &self,
        recovery_id: &str,
        destination_path: &Path,
        overwrite: bool,
    ) -> Result<DeletedRecoveryExportDto, DeletedRecoveryError> {
        super::export::export_deleted_recovery_in_context(
            self,
            recovery_id,
            destination_path,
            overwrite,
        )
    }
}
