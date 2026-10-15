use super::StagingRepo;
use crate::connection::DbResult;
use rusqlite::Connection;

impl StagingRepo {
    /// Discard one incomplete enumeration atomically before starting fallback.
    pub fn reset_enum_for_fallback(conn: &Connection) -> DbResult<()> {
        let transaction = conn.unchecked_transaction()?;
        transaction.execute_batch(
            "DELETE FROM file_entries;
             DELETE FROM staging_meta
              WHERE key IN ('enum_strategy', 'mft_records', 'mft_fallback_warning',
                            'file_count', 'dir_count');",
        )?;
        transaction.commit()?;
        Ok(())
    }
}
