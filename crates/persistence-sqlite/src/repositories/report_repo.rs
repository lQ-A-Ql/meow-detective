use crate::connection::DbResult;
use crate::repositories::fingerprint_repo::ForensicFingerprintRepo;
use domain::ForensicFingerprint;
use rusqlite::{params, Connection, OptionalExtension};

pub struct ReportRecord {
    pub id: String,
    pub case_id: String,
    pub template_id: String,
    pub file_name: String,
    pub created_by: String,
    pub status: String,
    pub progress: Option<u32>,
    pub created_at: String,
    pub request_json: String,
    pub error_code: Option<String>,
    pub error_message: Option<String>,
    pub file_size: Option<u64>,
    pub sha256: Option<String>,
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
            "INSERT INTO reports (id, case_id, template_id, file_name, created_by, status, progress, created_at, request_json)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                record.id,
                record.case_id,
                record.template_id,
                record.file_name,
                record.created_by,
                record.status,
                record.progress,
                record.created_at,
                record.request_json,
            ],
        )?;
        ForensicFingerprintRepo::new(self.conn).upsert_if_available(
            &ForensicFingerprint::for_report(
                &record.id,
                &record.case_id,
                &record.file_name,
                &record.template_id,
            ),
        )?;
        Ok(())
    }

    pub fn list_by_case(&self, case_id: &str) -> DbResult<Vec<ReportRecord>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, case_id, template_id, file_name, created_by, status, progress, created_at, request_json, error_code, error_message, file_size, sha256
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
                request_json: row.get(8)?,
                error_code: row.get(9)?,
                error_message: row.get(10)?,
                file_size: row.get(11)?,
                sha256: row.get(12)?,
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

    pub fn complete(&self, id: &str, size: u64, sha256: &str) -> DbResult<()> {
        self.conn.execute("UPDATE reports SET status = 'completed', progress = 100, file_size = ?2, sha256 = ?3, error_code = NULL, error_message = NULL WHERE id = ?1 AND status = 'running'", params![id, size, sha256])?;
        Ok(())
    }

    pub fn fail(&self, id: &str, status: &str, code: &str, message: &str) -> DbResult<()> {
        self.conn.execute("UPDATE reports SET status = ?2, error_code = ?3, error_message = ?4 WHERE id = ?1 AND status = 'running'", params![id, status, code, message])?;
        Ok(())
    }

    pub fn find_by_id(&self, case_id: &str, id: &str) -> DbResult<Option<ReportRecord>> {
        Ok(self.conn.query_row(
            "SELECT id, case_id, template_id, file_name, created_by, status, progress, created_at, request_json, error_code, error_message, file_size, sha256 FROM reports WHERE case_id = ?1 AND id = ?2",
            params![case_id, id], |row| Ok(ReportRecord {
                id: row.get(0)?, case_id: row.get(1)?, template_id: row.get(2)?, file_name: row.get(3)?,
                created_by: row.get(4)?, status: row.get(5)?, progress: row.get(6)?, created_at: row.get(7)?,
                request_json: row.get(8)?, error_code: row.get(9)?, error_message: row.get(10)?, file_size: row.get(11)?, sha256: row.get(12)?,
            }),
        ).optional()?)
    }
}

#[cfg(test)]
#[path = "../../tests/unit/repositories/report_repo.rs"]
mod tests;
