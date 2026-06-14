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
            CREATE TABLE batch_jobs (
                id TEXT PRIMARY KEY NOT NULL,
                case_id TEXT NOT NULL REFERENCES cases(id) ON DELETE CASCADE,
                label TEXT NOT NULL DEFAULT '',
                plan_json TEXT NOT NULL DEFAULT '{}',
                status TEXT NOT NULL DEFAULT 'queued',
                created_at TEXT NOT NULL DEFAULT (datetime('now')),
                started_at TEXT,
                completed_at TEXT
            );
            CREATE TABLE batch_phases (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                batch_id TEXT NOT NULL,
                kind TEXT NOT NULL,
                state TEXT NOT NULL DEFAULT 'queued',
                progress REAL NOT NULL DEFAULT 0.0,
                started_at TEXT,
                completed_at TEXT,
                error_count INTEGER NOT NULL DEFAULT 0,
                warnings_json TEXT NOT NULL DEFAULT '[]',
                UNIQUE(batch_id, kind),
                FOREIGN KEY (batch_id) REFERENCES batch_jobs(id) ON DELETE CASCADE
            );
            CREATE TABLE batch_checkpoints (
                batch_id TEXT NOT NULL,
                phase_kind TEXT NOT NULL,
                key TEXT NOT NULL,
                value_json TEXT NOT NULL DEFAULT '{}',
                saved_at TEXT NOT NULL DEFAULT (datetime('now')),
                PRIMARY KEY (batch_id, phase_kind, key),
                FOREIGN KEY (batch_id) REFERENCES batch_jobs(id) ON DELETE CASCADE
            );",
        )
        .unwrap();
        conn.execute(
            "INSERT INTO cases (id, name, created_at, updated_at) VALUES (?1, ?2, datetime('now'), datetime('now'))",
            params!["case-1", "Test Case"],
        )
        .unwrap();
        conn
    }

    #[test]
    fn create_and_get_batch_job() {
        let conn = setup_db();
        let repo = BatchRepo::new(&conn);

        repo.create_job(
            "batch-1",
            "case-1",
            "Test Batch",
            r#"{"phases":["Mount","Catalog"]}"#,
        )
        .unwrap();

        let job = repo.get_job("batch-1").unwrap().expect("should exist");
        assert_eq!(job.id, "batch-1");
        assert_eq!(job.case_id, "case-1");
        assert_eq!(job.label, "Test Batch");
        assert_eq!(job.status, "queued");
    }

    #[test]
    fn list_jobs_by_case() {
        let conn = setup_db();
        let repo = BatchRepo::new(&conn);

        repo.create_job("batch-1", "case-1", "First", "{}").unwrap();
        repo.create_job("batch-2", "case-1", "Second", "{}")
            .unwrap();

        let jobs = repo.list_jobs("case-1").unwrap();
        assert_eq!(jobs.len(), 2);
    }

    #[test]
    fn update_job_status() {
        let conn = setup_db();
        let repo = BatchRepo::new(&conn);

        repo.create_job("batch-1", "case-1", "Test", "{}").unwrap();
        repo.update_job_status("batch-1", "running").unwrap();

        let job = repo.get_job("batch-1").unwrap().unwrap();
        assert_eq!(job.status, "running");
        assert!(job.started_at.is_some());
    }

    #[test]
    fn upsert_and_get_phases() {
        let conn = setup_db();
        let repo = BatchRepo::new(&conn);

        repo.create_job("batch-1", "case-1", "Test", "{}").unwrap();
        repo.upsert_phase("batch-1", "Mount", "running", 0.5, 0, "[]")
            .unwrap();
        repo.upsert_phase("batch-1", "Mount", "completed", 1.0, 0, "[]")
            .unwrap();

        let phases = repo.get_phases("batch-1").unwrap();
        assert_eq!(phases.len(), 1);
        assert_eq!(phases[0].kind, "Mount");
        assert_eq!(phases[0].state, "completed");
        assert_eq!(phases[0].progress, 1.0);
    }

    #[test]
    fn checkpoint_read_write() {
        let conn = setup_db();
        let repo = BatchRepo::new(&conn);

        repo.create_job("batch-1", "case-1", "Test", "{}").unwrap();
        repo.write_checkpoint("batch-1", "Catalog", "last_offset", r#""12345""#)
            .unwrap();

        let val = repo
            .read_checkpoint("batch-1", "Catalog", "last_offset")
            .unwrap();
        assert_eq!(val.unwrap(), r#""12345""#);
    }

    #[test]
    fn checkpoint_missing_returns_none() {
        let conn = setup_db();
        let repo = BatchRepo::new(&conn);

        repo.create_job("batch-1", "case-1", "Test", "{}").unwrap();
        let val = repo
            .read_checkpoint("batch-1", "Catalog", "nonexistent")
            .unwrap();
        assert!(val.is_none());
    }
}
