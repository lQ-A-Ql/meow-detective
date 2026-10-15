use crate::{DbError, DbResult};
use rusqlite::{params, Connection, OptionalExtension};

pub struct EvidenceTaskRepo<'a> {
    conn: &'a Connection,
}
#[derive(Debug, Clone)]
pub struct EvidenceTaskRecord {
    pub task_id: String,
    pub case_id: String,
    pub kind: String,
    pub state: String,
    pub request_json: String,
    pub fingerprint: String,
    pub source_revision: String,
    pub bytes_processed: u64,
    pub total_bytes: u64,
    pub result_json: Option<String>,
    pub error_code: Option<String>,
}
impl<'a> EvidenceTaskRepo<'a> {
    pub fn active_count(&self, case_id: &str) -> DbResult<u64> {
        self.conn.query_row("SELECT COUNT(*) FROM evidence_tasks WHERE case_id=?1 AND state IN ('queued','running','cancelling')",[case_id],|row|row.get(0)).map_err(Into::into)
    }
    pub fn recover_all(&self) -> DbResult<()> {
        self.conn.execute("UPDATE evidence_tasks SET state='failed',error_code='interrupted',updated_at=datetime('now') WHERE state IN ('queued','running','cancelling')",[])?;
        Ok(())
    }
    pub fn request_cancel(&self, case_id: &str, task_id: &str) -> DbResult<()> {
        self.conn.execute("UPDATE evidence_tasks SET state='cancelling',updated_at=datetime('now') WHERE case_id=?1 AND task_id=?2 AND state IN ('queued','running')",params![case_id,task_id])?;
        Ok(())
    }
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }
    pub fn insert(&self, record: &EvidenceTaskRecord) -> DbResult<()> {
        self.conn.execute("INSERT INTO evidence_tasks(task_id,case_id,kind,state,request_json,fingerprint,source_revision,updated_at) VALUES(?1,?2,?3,'queued',?4,?5,?6,datetime('now'))",params![record.task_id,record.case_id,record.kind,record.request_json,record.fingerprint,record.source_revision])?;
        Ok(())
    }
    pub fn find(&self, case_id: &str, task_id: &str) -> DbResult<Option<EvidenceTaskRecord>> {
        self.conn.query_row("SELECT task_id,case_id,kind,state,request_json,fingerprint,source_revision,bytes_processed,total_bytes,result_json,error_code FROM evidence_tasks WHERE case_id=?1 AND task_id=?2",params![case_id,task_id],read).optional().map_err(Into::into)
    }
    pub fn reusable(
        &self,
        case_id: &str,
        fingerprint: &str,
    ) -> DbResult<Option<EvidenceTaskRecord>> {
        self.conn.query_row("SELECT task_id,case_id,kind,state,request_json,fingerprint,source_revision,bytes_processed,total_bytes,result_json,error_code FROM evidence_tasks WHERE case_id=?1 AND fingerprint=?2 AND state IN ('queued','running','cancelling','completed') ORDER BY updated_at DESC LIMIT 1",params![case_id,fingerprint],read).optional().map_err(Into::into)
    }
    pub fn progress(&self, case_id: &str, task_id: &str, bytes: u64, total: u64) -> DbResult<()> {
        let bytes =
            i64::try_from(bytes).map_err(|_| DbError::System("task byte count overflow".into()))?;
        let total =
            i64::try_from(total).map_err(|_| DbError::System("task total overflow".into()))?;
        self.conn.execute("UPDATE evidence_tasks SET state=CASE WHEN state='cancelling' THEN state ELSE 'running' END,bytes_processed=?3,total_bytes=?4,updated_at=datetime('now') WHERE case_id=?1 AND task_id=?2 AND state IN ('queued','running','cancelling')",params![case_id,task_id,bytes,total])?;
        Ok(())
    }
    pub fn finish(
        &self,
        case_id: &str,
        task_id: &str,
        state: &str,
        result: Option<&str>,
        error: Option<&str>,
    ) -> DbResult<()> {
        self.conn.execute("UPDATE evidence_tasks SET state=?3,result_json=?4,error_code=?5,updated_at=datetime('now') WHERE case_id=?1 AND task_id=?2",params![case_id,task_id,state,result,error])?;
        Ok(())
    }
    pub fn recover(&self, case_id: &str) -> DbResult<()> {
        self.conn.execute("UPDATE evidence_tasks SET state='failed',error_code='interrupted',updated_at=datetime('now') WHERE case_id=?1 AND state IN ('queued','running','cancelling')",[case_id])?;
        Ok(())
    }
}
fn read(row: &rusqlite::Row<'_>) -> rusqlite::Result<EvidenceTaskRecord> {
    Ok(EvidenceTaskRecord {
        task_id: row.get(0)?,
        case_id: row.get(1)?,
        kind: row.get(2)?,
        state: row.get(3)?,
        request_json: row.get(4)?,
        fingerprint: row.get(5)?,
        source_revision: row.get(6)?,
        bytes_processed: row.get(7)?,
        total_bytes: row.get(8)?,
        result_json: row.get(9)?,
        error_code: row.get(10)?,
    })
}
