use super::StagingRepo;
use crate::connection::DbResult;
use rusqlite::Connection;

impl StagingRepo {
    /// Publish done only when both enumeration counters have persisted.
    pub fn complete_enum(conn: &Connection, file_count: u64, dir_count: u64) -> DbResult<()> {
        let transaction = conn.unchecked_transaction()?;
        Self::set_staging_meta(&transaction, "file_count", &file_count.to_string())?;
        Self::set_staging_meta(&transaction, "dir_count", &dir_count.to_string())?;
        Self::set_staging_meta(&transaction, "status", "done")?;
        transaction.commit()?;
        Ok(())
    }
}
