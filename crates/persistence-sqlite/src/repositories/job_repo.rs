use crate::connection::DbResult;
use domain::JobId;
use rusqlite::{params, Connection};
use uuid::Uuid;

pub struct JobRepo<'a> {
    conn: &'a Connection,
}

impl<'a> JobRepo<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    pub fn create(&self, case_id: &str, kind: &str) -> DbResult<JobId> {
        let id = JobId(Uuid::new_v4().to_string());
        self.conn.execute(
            "INSERT INTO jobs (id, case_id, kind, status, progress, detail) VALUES (?1, ?2, ?3, 'running', 0, '')",
            params![id.0, case_id, kind],
        )?;
        Ok(id)
    }

    pub fn update_progress(&self, id: &JobId, progress: u32, detail: &str) -> DbResult<()> {
        self.conn.execute(
            "UPDATE jobs SET progress = ?1, detail = ?2 WHERE id = ?3",
            params![progress, detail, id.0],
        )?;
        Ok(())
    }

    pub fn complete(&self, id: &JobId, detail: &str) -> DbResult<()> {
        self.conn.execute(
            "UPDATE jobs SET status = 'completed', progress = 100, detail = ?1, finished_at = datetime('now') WHERE id = ?2",
            params![detail, id.0],
        )?;
        Ok(())
    }

    pub fn fail(&self, id: &JobId, detail: &str) -> DbResult<()> {
        self.conn.execute(
            "UPDATE jobs SET status = 'failed', detail = ?1, finished_at = datetime('now') WHERE id = ?2",
            params![detail, id.0],
        )?;
        Ok(())
    }

    pub fn list_active(&self) -> DbResult<Vec<(JobId, String, String, u32)>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, kind, status, progress FROM jobs WHERE status IN ('running','pending') ORDER BY created_at DESC",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok((JobId(row.get(0)?), row.get(1)?, row.get(2)?, row.get(3)?))
        })?;
        let mut result = Vec::new();
        for r in rows {
            result.push(r?);
        }
        Ok(result)
    }
}
