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
        let tx = self.conn.unchecked_transaction()?;
        tx.execute(
            "INSERT INTO evidence_digests(case_id,scope,algorithm,data_source_id,file_id,partition_index,value,byte_length,status,source_revision,computed_at)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,datetime('now'))
             ON CONFLICT(case_id,scope,algorithm,data_source_id,file_id,partition_index) DO UPDATE SET value=excluded.value,byte_length=excluded.byte_length,status=excluded.status,source_revision=excluded.source_revision,computed_at=excluded.computed_at",
            params![record.case_id, record.scope, record.algorithm, record.data_source_id.clone().unwrap_or_default(), record.file_id.clone().unwrap_or_default(), record.partition_index.map(i64::from).unwrap_or(-1), record.value, i64::try_from(record.byte_length).map_err(|_| crate::connection::DbError::System("digest length exceeds SQLite range".to_string()))?, record.status, record.source_revision],
        )?;
        crate::repositories::audit_repo::AuditRepo::new(&tx).log_in_transaction(
            &tx,
            Some(&record.case_id),
            "local",
            &crate::repositories::audit_repo::AuditAction::DigestCalculate,
            record.data_source_id.as_deref(),
            &format!(
                "scope={} algorithm={} revision={} bytes={} digest={}",
                record.scope,
                record.algorithm,
                record.source_revision,
                record.byte_length,
                record.value
            ),
        )?;
        tx.commit()?;
        Ok(())
    }
    pub fn list(
        &self,
        case: &str,
        source: &str,
        file: Option<&str>,
        partition: Option<u32>,
    ) -> DbResult<Vec<EvidenceDigestRecord>> {
        let mut statement = self.conn.prepare("SELECT case_id,scope,algorithm,data_source_id,file_id,partition_index,value,byte_length,status,source_revision FROM evidence_digests WHERE case_id=?1 AND data_source_id=?2 AND (file_id='' OR file_id=?3) AND (partition_index=-1 OR partition_index=?4) ORDER BY scope,algorithm LIMIT 24")?;
        let results = statement
            .query_map(
                params![
                    case,
                    source,
                    file.unwrap_or_default(),
                    partition.map(i64::from).unwrap_or(-1)
                ],
                |row| {
                    let file: String = row.get(4)?;
                    let index: i64 = row.get(5)?;
                    Ok(EvidenceDigestRecord {
                        case_id: row.get(0)?,
                        scope: row.get(1)?,
                        algorithm: row.get(2)?,
                        data_source_id: Some(row.get(3)?),
                        file_id: (!file.is_empty()).then_some(file),
                        partition_index: u32::try_from(index).ok(),
                        value: row.get(6)?,
                        byte_length: row.get(7)?,
                        status: row.get(8)?,
                        source_revision: row.get(9)?,
                    })
                },
            )?
            .collect::<Result<Vec<_>, _>>()
            .map_err(Into::into);
        results
    }
}
