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
    pub warning_count: u32,
    pub skipped_count: u32,
    pub failed_count: u32,
    pub partial: bool,
    pub current_partition: Option<String>,
    pub completed_partitions: u32,
    pub total_partitions: u32,
    pub partition_progress: u32,
    pub created_at: String,
    pub finished_at: Option<String>,
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

    pub fn has_active_kind_with_detail_prefix(
        &self,
        kind: &str,
        detail_prefix: &str,
    ) -> DbResult<bool> {
        let exists = self.conn.query_row(
            "SELECT EXISTS(
                 SELECT 1 FROM jobs
                 WHERE kind = ?1
                   AND detail LIKE ?2 || '%'
                   AND status IN ('running', 'pending', 'cancelling')
             )",
            params![kind, detail_prefix],
            |row| row.get::<_, bool>(0),
        )?;
        Ok(exists)
    }

    pub fn update_outcome_counts(
        &self,
        id: &JobId,
        warning_count: u32,
        skipped_count: u32,
        failed_count: u32,
        partial: bool,
    ) -> DbResult<()> {
        self.conn.execute(
            "UPDATE jobs SET warning_count = ?1, skipped_count = ?2, failed_count = ?3, partial = ?4 WHERE id = ?5",
            params![
                warning_count,
                skipped_count,
                failed_count,
                if partial { 1 } else { 0 },
                id.0
            ],
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
        self.complete_if_active(id, detail)?;
        Ok(())
    }

    pub fn complete_if_active(&self, id: &JobId, detail: &str) -> DbResult<bool> {
        let updated = self.conn.execute(
            "UPDATE jobs
             SET status = 'completed', progress = 100, detail = ?1, finished_at = datetime('now')
             WHERE id = ?2 AND status IN ('running', 'pending')",
            params![detail, id.0],
        )?;
        Ok(updated > 0)
    }

    pub fn fail(&self, id: &JobId, detail: &str) -> DbResult<()> {
        self.conn.execute(
            "UPDATE jobs
             SET status = 'failed', detail = ?1, finished_at = datetime('now')
             WHERE id = ?2 AND status IN ('running', 'pending', 'cancelling')",
            params![detail, id.0],
        )?;
        Ok(())
    }

    pub fn mark_cancelling(&self, id: &JobId, detail: &str) -> DbResult<bool> {
        let updated = self.conn.execute(
            "UPDATE jobs
             SET status = 'cancelling', detail = ?1
             WHERE id = ?2 AND status IN ('running', 'pending')",
            params![detail, id.0],
        )?;
        Ok(updated > 0)
    }

    pub fn cancel(&self, id: &JobId, detail: &str) -> DbResult<bool> {
        let updated = self.conn.execute(
            "UPDATE jobs
             SET status = 'cancelled', detail = ?1, finished_at = datetime('now')
             WHERE id = ?2 AND status IN ('running', 'pending', 'cancelling')",
            params![detail, id.0],
        )?;
        Ok(updated > 0)
    }

    /// Find jobs left in a running or cancelling state (interrupted by crash/shutdown).
    pub fn find_interrupted(&self) -> DbResult<Vec<JobId>> {
        let mut stmt = self
            .conn
            .prepare("SELECT id FROM jobs WHERE status IN ('running', 'cancelling')")?;
        let rows = stmt.query_map([], |row| {
            let raw: String = row.get(0)?;
            Ok(JobId(raw))
        })?;
        let mut result = Vec::new();
        for r in rows {
            result.push(r?);
        }
        Ok(result)
    }

    pub fn list_recent(&self, limit: usize) -> DbResult<Vec<JobSummaryRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, kind, status, progress, detail,
                    warning_count, skipped_count, failed_count, partial,
                    current_partition, completed_partitions, total_partitions, partition_progress,
                    created_at, finished_at
             FROM jobs
             ORDER BY
                 CASE
                     WHEN status IN ('running', 'pending', 'cancelling') THEN 0
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
                warning_count: row.get::<_, Option<u32>>(5)?.unwrap_or(0),
                skipped_count: row.get::<_, Option<u32>>(6)?.unwrap_or(0),
                failed_count: row.get::<_, Option<u32>>(7)?.unwrap_or(0),
                partial: row.get::<_, Option<bool>>(8)?.unwrap_or(false),
                current_partition: row.get(9)?,
                completed_partitions: row.get::<_, Option<u32>>(10)?.unwrap_or(0),
                total_partitions: row.get::<_, Option<u32>>(11)?.unwrap_or(0),
                partition_progress: row.get::<_, Option<u32>>(12)?.unwrap_or(0),
                created_at: row.get(13)?,
                finished_at: row.get(14)?,
            })
        })?;
        let mut result = Vec::new();
        for r in rows {
            result.push(r?);
        }
        Ok(result)
    }
}

#[cfg(test)]
#[path = "../../tests/unit/repositories/job_repo.rs"]
mod tests;
