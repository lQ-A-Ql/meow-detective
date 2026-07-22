mod query;
mod records;
mod validation;
mod write;

pub use records::{
    DeletedRecoveryAggregate, DeletedRecoveryPageRecord, DeletedRecoveryRecord,
    RecoveryIssueRecord, RecoveryRangeRecord, RecoveryScanRecord,
};

use rusqlite::Connection;

pub struct DeletedRecoveryRepo<'a> {
    pub(super) conn: &'a Connection,
}

impl<'a> DeletedRecoveryRepo<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }
}
