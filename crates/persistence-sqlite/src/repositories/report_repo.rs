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
#[path = "../../tests/unit/repositories/report_repo.rs"]
mod tests;
