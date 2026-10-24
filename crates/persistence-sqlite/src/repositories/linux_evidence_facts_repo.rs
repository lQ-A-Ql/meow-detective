use crate::connection::{DbError, DbResult};
use domain::DataSourceId;
use rusqlite::Connection;

pub struct LinuxEvidenceFactsRepo<'a> {
    conn: &'a Connection,
}

impl<'a> LinuxEvidenceFactsRepo<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    pub fn find_json(&self, data_source_id: &DataSourceId) -> DbResult<Option<String>> {
        let result = self.conn.query_row(
            "SELECT facts_json
             FROM linux_evidence_facts
             WHERE data_source_id = ?1 AND schema_version = 1",
            [data_source_id.0.as_str()],
            |row| row.get::<_, String>(0),
        );
        match result {
            Ok(value) => Ok(Some(value)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(error) => Err(DbError::from(error)),
        }
    }

    pub fn replace_json(&self, data_source_id: &DataSourceId, facts_json: &str) -> DbResult<()> {
        self.conn.execute(
            "INSERT INTO linux_evidence_facts
                 (data_source_id, schema_version, facts_json)
             VALUES (?1, 1, ?2)
             ON CONFLICT(data_source_id) DO UPDATE SET
                 facts_json = excluded.facts_json,
                 updated_at = datetime('now')",
            rusqlite::params![data_source_id.0, facts_json],
        )?;
        Ok(())
    }
}
