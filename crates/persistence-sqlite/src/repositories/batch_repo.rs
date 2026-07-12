use crate::connection::DbResult;
use rusqlite::{params, Connection};

/// Persisted row matching the batch_jobs table.
#[derive(Debug, Clone)]
pub struct BatchJobRow {
    pub id: String,
    pub case_id: String,
    pub label: String,
    pub plan_json: String,
    pub status: String,
    pub created_at: String,
    pub started_at: Option<String>,
    pub completed_at: Option<String>,
}

/// Persisted row matching the batch_phases table.
#[derive(Debug, Clone)]
pub struct BatchPhaseRow {
    pub id: i64,
    pub batch_id: String,
    pub kind: String,
    pub state: String,
    pub progress: f64,
    pub started_at: Option<String>,
    pub completed_at: Option<String>,
    pub error_count: u32,
    pub warnings_json: String,
}

pub struct BatchRepo<'a> {
    conn: &'a Connection,
}

impl<'a> BatchRepo<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    pub fn create_job(
        &self,
        id: &str,
        case_id: &str,
        label: &str,
        plan_json: &str,
    ) -> DbResult<()> {
        self.conn.execute(
            "INSERT INTO batch_jobs (id, case_id, label, plan_json, status)
             VALUES (?1, ?2, ?3, ?4, 'queued')",
            params![id, case_id, label, plan_json],
        )?;
        Ok(())
    }

    pub fn get_job(&self, id: &str) -> DbResult<Option<BatchJobRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, case_id, label, plan_json, status, created_at, started_at, completed_at
             FROM batch_jobs WHERE id = ?1",
        )?;
        let mut rows = stmt.query_map(params![id], |row| {
            Ok(BatchJobRow {
                id: row.get(0)?,
                case_id: row.get(1)?,
                label: row.get(2)?,
                plan_json: row.get(3)?,
                status: row.get(4)?,
                created_at: row.get(5)?,
                started_at: row.get(6)?,
                completed_at: row.get(7)?,
            })
        })?;
        match rows.next() {
            Some(row) => Ok(Some(row?)),
            None => Ok(None),
        }
    }

    pub fn list_jobs(&self, case_id: &str) -> DbResult<Vec<BatchJobRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, case_id, label, plan_json, status, created_at, started_at, completed_at
             FROM batch_jobs
             WHERE case_id = ?1
             ORDER BY created_at DESC",
        )?;
        let rows = stmt.query_map(params![case_id], |row| {
            Ok(BatchJobRow {
                id: row.get(0)?,
                case_id: row.get(1)?,
                label: row.get(2)?,
                plan_json: row.get(3)?,
                status: row.get(4)?,
                created_at: row.get(5)?,
                started_at: row.get(6)?,
                completed_at: row.get(7)?,
            })
        })?;
        let mut result = Vec::new();
        for r in rows {
            result.push(r?);
        }
        Ok(result)
    }

    pub fn update_job_status(&self, id: &str, status: &str) -> DbResult<()> {
        let now = match status {
            "running" => ", started_at = CASE WHEN started_at IS NULL THEN datetime('now') ELSE started_at END",
            "completed" | "failed" | "cancelled" => ", completed_at = datetime('now')",
            _ => "",
        };
        self.conn.execute(
            &format!("UPDATE batch_jobs SET status = ?1{} WHERE id = ?2", now),
            params![status, id],
        )?;
        Ok(())
    }

    pub fn upsert_phase(
        &self,
        batch_id: &str,
        kind: &str,
        state: &str,
        progress: f64,
        error_count: u32,
        warnings_json: &str,
    ) -> DbResult<()> {
        let now = match state {
            "running" => ", started_at = CASE WHEN started_at IS NULL THEN datetime('now') ELSE started_at END",
            "completed" | "failed" => ", completed_at = datetime('now')",
            _ => "",
        };
        self.conn.execute(
            &format!(
                "INSERT INTO batch_phases (batch_id, kind, state, progress, error_count, warnings_json)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                 ON CONFLICT(batch_id, kind) DO UPDATE SET
                     state = excluded.state,
                     progress = excluded.progress,
                     error_count = excluded.error_count,
                     warnings_json = excluded.warnings_json{}",
                now
            ),
            params![batch_id, kind, state, progress, error_count, warnings_json],
        )?;
        Ok(())
    }

    pub fn get_phases(&self, batch_id: &str) -> DbResult<Vec<BatchPhaseRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, batch_id, kind, state, progress, started_at, completed_at, error_count, warnings_json
             FROM batch_phases
             WHERE batch_id = ?1
             ORDER BY id",
        )?;
        let rows = stmt.query_map(params![batch_id], |row| {
            Ok(BatchPhaseRow {
                id: row.get(0)?,
                batch_id: row.get(1)?,
                kind: row.get(2)?,
                state: row.get(3)?,
                progress: row.get(4)?,
                started_at: row.get(5)?,
                completed_at: row.get(6)?,
                error_count: row.get(7)?,
                warnings_json: row.get(8)?,
            })
        })?;
        let mut result = Vec::new();
        for r in rows {
            result.push(r?);
        }
        Ok(result)
    }

    pub fn write_checkpoint(
        &self,
        batch_id: &str,
        phase_kind: &str,
        key: &str,
        value_json: &str,
    ) -> DbResult<()> {
        self.conn.execute(
            "INSERT INTO batch_checkpoints (batch_id, phase_kind, key, value_json)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(batch_id, phase_kind, key) DO UPDATE SET
                 value_json = excluded.value_json,
                 saved_at = datetime('now')",
            params![batch_id, phase_kind, key, value_json],
        )?;
        Ok(())
    }

    pub fn read_checkpoint(
        &self,
        batch_id: &str,
        phase_kind: &str,
        key: &str,
    ) -> DbResult<Option<String>> {
        let mut stmt = self.conn.prepare(
            "SELECT value_json FROM batch_checkpoints
             WHERE batch_id = ?1 AND phase_kind = ?2 AND key = ?3",
        )?;
        let mut rows = stmt.query_map(params![batch_id, phase_kind, key], |row| {
            row.get::<_, String>(0)
        })?;
        match rows.next() {
            Some(row) => Ok(Some(row?)),
            None => Ok(None),
        }
    }
}

#[cfg(test)]
#[path = "../../tests/unit/repositories/batch_repo.rs"]
mod tests;
