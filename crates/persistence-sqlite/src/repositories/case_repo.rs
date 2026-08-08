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
}

#[cfg(test)]
#[path = "../../tests/unit/repositories/case_repo.rs"]
mod tests;
