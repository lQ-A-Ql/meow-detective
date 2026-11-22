use crate::DbResult;
use rusqlite::{params, Connection};

pub struct SourceCapabilityRepo<'a> {
    conn: &'a Connection,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceCapabilityRow {
    pub case_id: String,
    pub data_source_id: String,
    pub capability: String,
    pub state: String,
    pub revision: i64,
    pub progress_done: i64,
    pub progress_total: Option<i64>,
    pub phase: Option<String>,
    pub detail: Option<String>,
    pub error_code: Option<String>,
    pub derived: i64,
    pub updated_at: String,
}

impl<'a> SourceCapabilityRepo<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    pub fn upsert(&self, row: &SourceCapabilityRow) -> DbResult<()> {
        self.conn.execute(
            "INSERT INTO source_capability (
                case_id, data_source_id, capability, state, revision,
                progress_done, progress_total, phase, detail, error_code,
                derived, updated_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
             ON CONFLICT(case_id, data_source_id, capability) DO UPDATE SET
                state = excluded.state,
                revision = excluded.revision,
                progress_done = excluded.progress_done,
                progress_total = excluded.progress_total,
                phase = excluded.phase,
                detail = excluded.detail,
                error_code = excluded.error_code,
                derived = excluded.derived,
                updated_at = excluded.updated_at",
            params![
                row.case_id,
                row.data_source_id,
                row.capability,
                row.state,
                row.revision,
                row.progress_done,
                row.progress_total,
                row.phase,
                row.detail,
                row.error_code,
                row.derived,
                row.updated_at,
            ],
        )?;
        Ok(())
    }

    /// Backfill helper: insert the row only when the key is absent; never
    /// overwrites a task-confirmed or existing derived row.
    pub fn insert_if_missing(&self, row: &SourceCapabilityRow) -> DbResult<()> {
        self.conn.execute(
            "INSERT OR IGNORE INTO source_capability (
                case_id, data_source_id, capability, state, revision,
                progress_done, progress_total, phase, detail, error_code,
                derived, updated_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
            params![
                row.case_id,
                row.data_source_id,
                row.capability,
                row.state,
                row.revision,
                row.progress_done,
                row.progress_total,
                row.phase,
                row.detail,
                row.error_code,
                row.derived,
                row.updated_at,
            ],
        )?;
        Ok(())
    }

    pub fn get(
        &self,
        case_id: &str,
        data_source_id: &str,
        capability: &str,
    ) -> DbResult<Option<SourceCapabilityRow>> {
        let result = self.conn.query_row(
            "SELECT case_id, data_source_id, capability, state, revision,
                    progress_done, progress_total, phase, detail, error_code,
                    derived, updated_at
             FROM source_capability
             WHERE case_id = ?1 AND data_source_id = ?2 AND capability = ?3",
            params![case_id, data_source_id, capability],
            read_row,
        );
        match result {
            Ok(row) => Ok(Some(row)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(error) => Err(error.into()),
        }
    }

    pub fn list_for_case(&self, case_id: &str) -> DbResult<Vec<SourceCapabilityRow>> {
        let mut statement = self.conn.prepare(
            "SELECT case_id, data_source_id, capability, state, revision,
                    progress_done, progress_total, phase, detail, error_code,
                    derived, updated_at
             FROM source_capability
             WHERE case_id = ?1
             ORDER BY data_source_id ASC, capability ASC",
        )?;
        let rows = statement.query_map([case_id], read_row)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    /// Restart reconciliation: rows left `running` by a dead process fall back to
    /// `queued`. No attempts column exists, so only the state reverts. Returns
    /// the number of rows recovered.
    pub fn requeue_running(&self, case_id: &str) -> DbResult<u64> {
        let changed = self.conn.execute(
            "UPDATE source_capability SET
                state = 'queued', updated_at = datetime('now')
             WHERE case_id = ?1 AND state = 'running'",
            params![case_id],
        )?;
        Ok(u64::try_from(changed).unwrap_or(0))
    }

    /// Rows eligible for shell-side re-enqueue after case open: `queued` or
    /// `pending` for this case only.
    pub fn list_pending(&self, case_id: &str) -> DbResult<Vec<SourceCapabilityRow>> {
        let mut statement = self.conn.prepare(
            "SELECT case_id, data_source_id, capability, state, revision,
                    progress_done, progress_total, phase, detail, error_code,
                    derived, updated_at
             FROM source_capability
             WHERE case_id = ?1 AND state IN ('queued', 'pending')
             ORDER BY data_source_id ASC, capability ASC",
        )?;
        let rows = statement.query_map([case_id], read_row)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }
}

fn read_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<SourceCapabilityRow> {
    Ok(SourceCapabilityRow {
        case_id: row.get(0)?,
        data_source_id: row.get(1)?,
        capability: row.get(2)?,
        state: row.get(3)?,
        revision: row.get(4)?,
        progress_done: row.get(5)?,
        progress_total: row.get(6)?,
        phase: row.get(7)?,
        detail: row.get(8)?,
        error_code: row.get(9)?,
        derived: row.get(10)?,
        updated_at: row.get(11)?,
    })
}
