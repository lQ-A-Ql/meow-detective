use crate::connection::DbResult;
use rusqlite::{params, Connection};

pub struct ReportRecord {
    pub id: String,
    pub case_id: String,
    pub template_id: String,
    pub file_name: String,
    pub created_by: String,
    pub status: String,
    pub progress: Option<u32>,
    pub created_at: String,
}

pub struct ReportRepo<'a> {
    conn: &'a Connection,
}

impl<'a> ReportRepo<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    pub fn insert(&self, record: &ReportRecord) -> DbResult<()> {
        self.conn.execute(
            "INSERT INTO reports (id, case_id, template_id, file_name, created_by, status, progress, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                record.id,
                record.case_id,
                record.template_id,
                record.file_name,
                record.created_by,
                record.status,
                record.progress,
                record.created_at,
            ],
        )?;
        Ok(())
    }

    pub fn list_by_case(&self, case_id: &str) -> DbResult<Vec<ReportRecord>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, case_id, template_id, file_name, created_by, status, progress, created_at
             FROM reports WHERE case_id = ?1 ORDER BY created_at DESC",
        )?;
        let rows = stmt.query_map(params![case_id], |row| {
            Ok(ReportRecord {
                id: row.get(0)?,
                case_id: row.get(1)?,
                template_id: row.get(2)?,
                file_name: row.get(3)?,
                created_by: row.get(4)?,
                status: row.get(5)?,
                progress: row.get(6)?,
                created_at: row.get(7)?,
            })
        })?;
        let mut records = Vec::new();
        for row in rows {
            records.push(row?);
        }
        Ok(records)
    }

    pub fn update_status(&self, id: &str, status: &str, progress: Option<u32>) -> DbResult<()> {
        self.conn.execute(
            "UPDATE reports SET status = ?1, progress = ?2 WHERE id = ?3",
            params![status, progress, id],
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup_db() -> Connection {
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
            CREATE TABLE reports (
                id TEXT PRIMARY KEY NOT NULL,
                case_id TEXT NOT NULL REFERENCES cases(id),
                template_id TEXT NOT NULL,
                file_name TEXT NOT NULL,
                created_by TEXT NOT NULL DEFAULT '',
                status TEXT NOT NULL DEFAULT 'running',
                progress INTEGER,
                created_at TEXT NOT NULL DEFAULT (datetime('now'))
            );",
        )
        .unwrap();
        conn
    }

    fn insert_case(conn: &Connection, case_id: &str) {
        conn.execute(
            "INSERT INTO cases (id, name, created_at, updated_at) VALUES (?1, ?2, datetime('now'), datetime('now'))",
            params![case_id, "Test Case"],
        ).unwrap();
    }

    fn make_report(id: &str, case_id: &str, status: &str) -> ReportRecord {
        ReportRecord {
            id: id.to_string(),
            case_id: case_id.to_string(),
            template_id: "tpl-1".to_string(),
            file_name: "report.html".to_string(),
            created_by: "tester".to_string(),
            status: status.to_string(),
            progress: Some(0),
            created_at: "2025-01-01T00:00:00Z".to_string(),
        }
    }

    #[test]
    fn insert_then_list_by_case_returns_record() {
        let conn = setup_db();
        insert_case(&conn, "case-1");
        let repo = ReportRepo::new(&conn);
        repo.insert(&make_report("r1", "case-1", "running"))
            .unwrap();

        let results = repo.list_by_case("case-1").unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "r1");
        assert_eq!(results[0].status, "running");
    }

    #[test]
    fn update_status_changes_field() {
        let conn = setup_db();
        insert_case(&conn, "case-1");
        let repo = ReportRepo::new(&conn);
        repo.insert(&make_report("r1", "case-1", "running"))
            .unwrap();

        repo.update_status("r1", "completed", Some(100)).unwrap();

        let results = repo.list_by_case("case-1").unwrap();
        assert_eq!(results[0].status, "completed");
        assert_eq!(results[0].progress, Some(100));
    }

    #[test]
    fn list_by_case_wrong_id_returns_empty() {
        let conn = setup_db();
        insert_case(&conn, "case-1");
        let repo = ReportRepo::new(&conn);
        repo.insert(&make_report("r1", "case-1", "running"))
            .unwrap();

        let results = repo.list_by_case("case-999").unwrap();
        assert!(results.is_empty());
    }
}
