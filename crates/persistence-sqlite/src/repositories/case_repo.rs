use crate::connection::DbResult;
use crate::util::parse_datetime;
use domain::{CaseId, CaseMeta};
use rusqlite::{params, Connection};

pub struct CaseMetrics {
    pub data_source_count: u64,
    pub indexed_file_count: u64,
    pub timeline_event_count: u64,
    pub artifact_count: u64,
}

pub struct CaseRepo<'a> {
    conn: &'a Connection,
}

impl<'a> CaseRepo<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    pub fn create(&self, case: &CaseMeta) -> DbResult<CaseId> {
        self.conn.execute(
            "INSERT INTO cases (id, name, number, examiner, notes, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                case.id.0,
                case.name,
                case.number,
                case.examiner,
                case.notes,
                case.created_at.to_rfc3339(),
                case.updated_at.to_rfc3339(),
            ],
        )?;
        Ok(case.id.clone())
    }

    pub fn find_by_id(&self, id: &CaseId) -> DbResult<Option<CaseMeta>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, name, number, examiner, notes, created_at, updated_at FROM cases WHERE id = ?1",
        )?;
        let result = stmt.query_row(params![id.0], |row| {
            Ok(CaseMeta {
                id: CaseId(row.get::<_, String>(0)?),
                name: row.get(1)?,
                number: row.get(2)?,
                examiner: row.get(3)?,
                notes: row.get(4)?,
                created_at: parse_datetime(&row.get::<_, String>(5)?),
                updated_at: parse_datetime(&row.get::<_, String>(6)?),
            })
        });
        match result {
            Ok(meta) => Ok(Some(meta)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    pub fn update(&self, case: &CaseMeta) -> DbResult<()> {
        self.conn.execute(
            "UPDATE cases SET name = ?1, number = ?2, examiner = ?3, notes = ?4, updated_at = ?5 WHERE id = ?6",
            params![
                case.name,
                case.number,
                case.examiner,
                case.notes,
                case.updated_at.to_rfc3339(),
                case.id.0,
            ],
        )?;
        Ok(())
    }

    pub fn list_all(&self) -> DbResult<Vec<CaseMeta>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, name, number, examiner, notes, created_at, updated_at FROM cases ORDER BY created_at DESC",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(CaseMeta {
                id: CaseId(row.get::<_, String>(0)?),
                name: row.get(1)?,
                number: row.get(2)?,
                examiner: row.get(3)?,
                notes: row.get(4)?,
                created_at: parse_datetime(&row.get::<_, String>(5)?),
                updated_at: parse_datetime(&row.get::<_, String>(6)?),
            })
        })?;
        let mut cases = Vec::new();
        for row in rows {
            cases.push(row?);
        }
        Ok(cases)
    }

    pub fn delete(&self, id: &CaseId) -> DbResult<()> {
        self.conn
            .execute("DELETE FROM cases WHERE id = ?1", params![id.0])?;
        Ok(())
    }

    pub fn get_metrics(&self) -> DbResult<CaseMetrics> {
        let file_count: i64 =
            self.conn
                .query_row("SELECT COUNT(*) FROM file_entries", [], |r| r.get(0))?;
        let artifact_count: i64 =
            self.conn
                .query_row("SELECT COUNT(*) FROM artifacts", [], |r| r.get(0))?;
        let timeline_count: i64 =
            self.conn
                .query_row("SELECT COUNT(*) FROM timeline_events", [], |r| r.get(0))?;
        let ds_count: i64 = self
            .conn
            .query_row("SELECT COUNT(*) FROM data_sources", [], |r| r.get(0))?;
        Ok(CaseMetrics {
            data_source_count: ds_count as u64,
            indexed_file_count: file_count as u64,
            timeline_event_count: timeline_count as u64,
            artifact_count: artifact_count as u64,
        })
    }

    pub fn delete_cascade(&self, id: &CaseId) -> DbResult<()> {
        let tx = self.conn.unchecked_transaction()?;
        // Delete artifacts whose source_object_id belongs to this case's files
        tx.execute(
            "DELETE FROM artifacts WHERE source_object_id IN (
                SELECT fe.id FROM file_entries fe
                JOIN data_sources ds ON fe.data_source_id = ds.id
                WHERE ds.case_id = ?1
            ) OR case_id = ?1",
            params![id.0],
        )?;
        // Delete timeline events for this case's files (including direct case_id match)
        tx.execute(
            "DELETE FROM timeline_events WHERE source_object_id IN (
                SELECT fe.id FROM file_entries fe
                JOIN data_sources ds ON fe.data_source_id = ds.id
                WHERE ds.case_id = ?1
            ) OR case_id = ?1",
            params![id.0],
        )?;
        // Delete file entries
        tx.execute(
            "DELETE FROM file_entries WHERE data_source_id IN (
                SELECT id FROM data_sources WHERE case_id = ?1
            )",
            params![id.0],
        )?;
        // Delete partitions (has ON DELETE CASCADE but be explicit)
        tx.execute(
            "DELETE FROM data_source_partitions WHERE data_source_id IN (
                SELECT id FROM data_sources WHERE case_id = ?1
            )",
            params![id.0],
        )?;
        // Delete data sources
        tx.execute("DELETE FROM data_sources WHERE case_id = ?1", params![id.0])?;
        // Delete tag bindings for this case's tags
        tx.execute(
            "DELETE FROM tag_bindings WHERE tag_id IN (
                SELECT id FROM tags WHERE case_id = ?1
            )",
            params![id.0],
        )?;
        tx.execute("DELETE FROM tags WHERE case_id = ?1", params![id.0])?;
        // Delete jobs
        tx.execute("DELETE FROM jobs WHERE case_id = ?1", params![id.0])?;
        // Delete reports
        tx.execute("DELETE FROM reports WHERE case_id = ?1", params![id.0])?;
        // Delete case
        tx.execute("DELETE FROM cases WHERE id = ?1", params![id.0])?;
        tx.commit()?;
        Ok(())
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
            );",
        )
        .unwrap();
        conn
    }

    fn make_case(id: &str, name: &str) -> CaseMeta {
        CaseMeta {
            id: CaseId(id.to_string()),
            name: name.to_string(),
            number: Some("2025-001".to_string()),
            examiner: Some("Tester".to_string()),
            notes: None,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        }
    }

    #[test]
    fn create_then_find_by_id_returns_it() {
        let conn = setup_db();
        let repo = CaseRepo::new(&conn);
        let case = make_case("c1", "Test Case");
        repo.create(&case).unwrap();

        let found = repo.find_by_id(&CaseId("c1".to_string())).unwrap();
        assert!(found.is_some());
        let found = found.unwrap();
        assert_eq!(found.name, "Test Case");
        assert_eq!(found.number, Some("2025-001".to_string()));
    }

    #[test]
    fn find_by_id_nonexistent_returns_none() {
        let conn = setup_db();
        let repo = CaseRepo::new(&conn);

        let found = repo.find_by_id(&CaseId("nope".to_string())).unwrap();
        assert!(found.is_none());
    }

    #[test]
    fn list_all_returns_all_cases() {
        let conn = setup_db();
        let repo = CaseRepo::new(&conn);
        repo.create(&make_case("c1", "Case A")).unwrap();
        repo.create(&make_case("c2", "Case B")).unwrap();

        let cases = repo.list_all().unwrap();
        assert_eq!(cases.len(), 2);
    }

    #[test]
    fn delete_removes_the_case() {
        let conn = setup_db();
        let repo = CaseRepo::new(&conn);
        repo.create(&make_case("c1", "Case A")).unwrap();

        repo.delete(&CaseId("c1".to_string())).unwrap();

        let found = repo.find_by_id(&CaseId("c1".to_string())).unwrap();
        assert!(found.is_none());
    }
}
