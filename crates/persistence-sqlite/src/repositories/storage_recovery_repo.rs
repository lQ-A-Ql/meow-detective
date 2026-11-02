use super::storage_control_repo::StorageControlRepo;
use crate::DbResult;
use rusqlite::params;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StorageRecoveryStatusRecord {
    pub case_id: String,
    pub scope_key: String,
    pub source_id: Option<String>,
    pub operation_id: Option<String>,
    pub disposition: String,
    pub storage_state: Option<String>,
    pub detail: String,
    pub scanned_at: String,
}

impl<'a> StorageControlRepo<'a> {
    pub fn upsert_recovery_status(&self, record: &StorageRecoveryStatusRecord) -> DbResult<()> {
        self.connection.execute(
            "INSERT INTO storage_recovery_status (
                case_id, scope_key, source_id, operation_id, disposition,
                storage_state, detail, scanned_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
             ON CONFLICT(case_id, scope_key) DO UPDATE SET
                source_id = excluded.source_id,
                operation_id = excluded.operation_id,
                disposition = excluded.disposition,
                storage_state = excluded.storage_state,
                detail = excluded.detail,
                scanned_at = excluded.scanned_at",
            params![
                record.case_id,
                record.scope_key,
                record.source_id,
                record.operation_id,
                record.disposition,
                record.storage_state,
                record.detail,
                record.scanned_at,
            ],
        )?;
        Ok(())
    }

    pub fn clear_recovery_status(&self, case_id: &str) -> DbResult<()> {
        self.connection.execute(
            "DELETE FROM storage_recovery_status WHERE case_id = ?1",
            [case_id],
        )?;
        Ok(())
    }

    pub fn list_recovery_status(
        &self,
        case_id: &str,
    ) -> DbResult<Vec<StorageRecoveryStatusRecord>> {
        let mut statement = self.connection.prepare(
            "SELECT case_id, scope_key, source_id, operation_id, disposition,
                    storage_state, detail, scanned_at
             FROM storage_recovery_status
             WHERE case_id = ?1
             ORDER BY scope_key ASC",
        )?;
        let rows = statement.query_map([case_id], |row| {
            Ok(StorageRecoveryStatusRecord {
                case_id: row.get(0)?,
                scope_key: row.get(1)?,
                source_id: row.get(2)?,
                operation_id: row.get(3)?,
                disposition: row.get(4)?,
                storage_state: row.get(5)?,
                detail: row.get(6)?,
                scanned_at: row.get(7)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }
}
