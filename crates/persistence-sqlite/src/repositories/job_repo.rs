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

    pub fn mark_cancelling(&self, id: &JobId, detail: &str) -> DbResult<()> {
        self.conn.execute(
            "UPDATE jobs SET status = 'cancelling', detail = ?1 WHERE id = ?2",
            params![detail, id.0],
        )?;
        Ok(())
    }

    pub fn cancel(&self, id: &JobId, detail: &str) -> DbResult<()> {
        self.conn.execute(
            "UPDATE jobs SET status = 'cancelled', detail = ?1, finished_at = datetime('now') WHERE id = ?2",
            params![detail, id.0],
        )?;
        Ok(())
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
                    current_partition, completed_partitions, total_partitions, partition_progress
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
mod tests {
    use super::*;

    fn setup_db() -> rusqlite::Connection {
        let conn = crate::connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE cases (
                id TEXT PRIMARY KEY NOT NULL,
                name TEXT NOT NULL,
                number TEXT,
                examiner TEXT,
                notes TEXT,
                created_at TEXT NOT NULL DEFAULT (datetime('now')),
                updated_at TEXT NOT NULL DEFAULT (datetime('now'))
            );
            CREATE TABLE jobs (
                id TEXT PRIMARY KEY NOT NULL,
                case_id TEXT NOT NULL REFERENCES cases(id),
                kind TEXT NOT NULL,
                status TEXT NOT NULL DEFAULT 'pending',
                progress INTEGER NOT NULL DEFAULT 0,
                detail TEXT NOT NULL DEFAULT '',
                created_at TEXT NOT NULL DEFAULT (datetime('now')),
                started_at TEXT,
                finished_at TEXT,
                current_partition TEXT DEFAULT NULL,
                completed_partitions INTEGER DEFAULT 0,
                total_partitions INTEGER DEFAULT 0,
                partition_progress INTEGER DEFAULT 0,
                warning_count INTEGER NOT NULL DEFAULT 0,
                skipped_count INTEGER NOT NULL DEFAULT 0,
                failed_count INTEGER NOT NULL DEFAULT 0,
                partial INTEGER NOT NULL DEFAULT 0
            );",
        )
        .unwrap();
        conn.execute(
            "INSERT INTO cases (id, name, created_at, updated_at) VALUES (?1, ?2, datetime('now'), datetime('now'))",
            params!["case-1", "Test Case"],
        ).unwrap();
        conn
    }

    #[test]
    fn create_returns_job_id() {
        let conn = setup_db();
        let repo = JobRepo::new(&conn);
        let id = repo.create("case-1", "ingest").unwrap();
        assert!(!id.0.is_empty());
    }

    #[test]
    fn update_progress_changes_progress() {
        let conn = setup_db();
        let repo = JobRepo::new(&conn);
        let id = repo.create("case-1", "ingest").unwrap();

        repo.update_progress(&id, 50, "halfway").unwrap();

        let jobs = repo.list_recent(10).unwrap();
        assert_eq!(jobs.len(), 1);
        assert_eq!(jobs[0].progress, 50);
        assert_eq!(jobs[0].detail, "halfway");
    }

    #[test]
    fn complete_sets_status_to_completed() {
        let conn = setup_db();
        let repo = JobRepo::new(&conn);
        let id = repo.create("case-1", "ingest").unwrap();

        repo.complete(&id, "done").unwrap();

        let jobs = repo.list_recent(10).unwrap();
        assert_eq!(jobs[0].status, "completed");
        assert_eq!(jobs[0].progress, 100);
    }

    #[test]
    fn fail_sets_status_to_failed() {
        let conn = setup_db();
        let repo = JobRepo::new(&conn);
        let id = repo.create("case-1", "ingest").unwrap();

        repo.fail(&id, "error occurred").unwrap();

        let jobs = repo.list_recent(10).unwrap();
        assert_eq!(jobs[0].status, "failed");
        assert_eq!(jobs[0].detail, "error occurred");
    }

    #[test]
    fn cancellation_methods_update_status_without_schema_changes() {
        let conn = setup_db();
        let repo = JobRepo::new(&conn);
        let id = repo.create("case-1", "ingest").unwrap();

        repo.mark_cancelling(&id, "Cancel requested").unwrap();
        let jobs = repo.list_recent(10).unwrap();
        assert_eq!(jobs[0].status, "cancelling");
        assert_eq!(jobs[0].detail, "Cancel requested");

        repo.cancel(&id, "Import cancelled by user").unwrap();
        let jobs = repo.list_recent(10).unwrap();
        assert_eq!(jobs[0].status, "cancelled");
        assert_eq!(jobs[0].detail, "Import cancelled by user");
    }

    #[test]
    fn list_recent_returns_jobs_ordered() {
        let conn = setup_db();
        let repo = JobRepo::new(&conn);

        let id1 = repo.create("case-1", "ingest").unwrap();
        let _id2 = repo.create("case-1", "search").unwrap();
        repo.complete(&id1, "done").unwrap();

        let jobs = repo.list_recent(10).unwrap();
        assert_eq!(jobs.len(), 2);
        // Running/pending jobs come before completed ones
        assert_eq!(jobs[0].status, "running");
        assert_eq!(jobs[1].status, "completed");
    }

    #[test]
    fn find_interrupted_returns_only_running_and_cancelling() {
        let conn = setup_db();
        let repo = JobRepo::new(&conn);

        let running = repo.create("case-1", "import").unwrap();
        let cancelling = repo.create("case-1", "import").unwrap();
        repo.mark_cancelling(&cancelling, "test").unwrap();

        let completed = repo.create("case-1", "import").unwrap();
        repo.complete(&completed, "done").unwrap();

        let failed = repo.create("case-1", "import").unwrap();
        repo.fail(&failed, "err").unwrap();

        let interrupted = repo.find_interrupted().unwrap();
        let ids: Vec<&str> = interrupted.iter().map(|id| id.0.as_str()).collect();
        assert_eq!(interrupted.len(), 2);
        assert!(ids.contains(&running.0.as_str()));
        assert!(ids.contains(&cancelling.0.as_str()));
    }
}
