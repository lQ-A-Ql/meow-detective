use crate::connection::DbResult;
use rusqlite::{params, Connection};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvidenceDigestRecord {
    pub case_id: String,
    pub scope: String,
    pub algorithm: String,
    pub data_source_id: Option<String>,
    pub file_id: Option<String>,
    pub partition_index: Option<u32>,
    pub value: String,
    pub byte_length: u64,
    pub status: String,
    pub source_revision: String,
}

pub struct EvidenceDigestRepo<'a> {
    conn: &'a Connection,
}

impl<'a> EvidenceDigestRepo<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    pub fn upsert(&self, record: &EvidenceDigestRecord) -> DbResult<()> {
        self.conn.execute(
            "INSERT INTO evidence_digests(case_id,scope,algorithm,data_source_id,file_id,partition_index,value,byte_length,status,source_revision,computed_at)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,datetime('now'))
             ON CONFLICT(case_id,scope,algorithm,data_source_id,file_id,partition_index) DO UPDATE SET value=excluded.value,byte_length=excluded.byte_length,status=excluded.status,source_revision=excluded.source_revision,computed_at=excluded.computed_at",
            params![record.case_id, record.scope, record.algorithm, record.data_source_id.clone().unwrap_or_default(), record.file_id.clone().unwrap_or_default(), record.partition_index.map(i64::from).unwrap_or(-1), record.value, i64::try_from(record.byte_length).map_err(|_| crate::connection::DbError::System("digest length exceeds SQLite range".to_string()))?, record.status, record.source_revision],
        )?;
        Ok(())
    }
}
