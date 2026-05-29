use crate::connection::DbResult;
use domain::JobId;
use rusqlite::{params, Connection};
use uuid::Uuid;

pub struct JobRepo<'a> {
    conn: &'a Connection,
}

#[derive(Debug, Clone)]
pub struct JobSummaryRow {
    pub id: JobId,
    pub kind: String,
    pub status: String,
    pub progress: u32,
    pub detail: String,
    pub current_partition: Option<String>,
    pub completed_partitions: u32,
    pub total_partitions: u32,
    pub partition_progress: u32,
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

    pub fn update_partition_progress(
        &self,
        id: &JobId,
        current_partition: &str,
        completed: u32,
        total: u32,
        partition_pct: u32,
    ) -> DbResult<()> {
        self.conn.execute(
            "UPDATE jobs SET current_partition = ?1, completed_partitions = ?2, total_partitions = ?3, partition_progress = ?4 WHERE id = ?5",
            params![current_partition, completed, total, partition_pct, id.0],
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

    pub fn list_recent(&self, limit: usize) -> DbResult<Vec<JobSummaryRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, kind, status, progress, detail,
                    current_partition, completed_partitions, total_partitions, partition_progress
             FROM jobs
             ORDER BY
                 CASE
                     WHEN status IN ('running', 'pending') THEN 0
                     WHEN status = 'failed' THEN 1
                     ELSE 2
                 END,
                 COALESCE(finished_at, created_at) DESC,
                 created_at DESC
             LIMIT ?1",
        )?;
        let rows = stmt.query_map(params![limit as i64], |row| {
            Ok(JobSummaryRow {
                id: JobId(row.get(0)?),
                kind: row.get(1)?,
                status: row.get(2)?,
                progress: row.get(3)?,
                detail: row.get(4)?,
                current_partition: row.get(5)?,
                completed_partitions: row.get::<_, Option<u32>>(6)?.unwrap_or(0),
                total_partitions: row.get::<_, Option<u32>>(7)?.unwrap_or(0),
                partition_progress: row.get::<_, Option<u32>>(8)?.unwrap_or(0),
            })
        })?;
        let mut result = Vec::new();
        for r in rows {
            result.push(r?);
        }
        Ok(result)
    }
}
