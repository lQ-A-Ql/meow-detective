use crate::connection::DbResult;
use rusqlite::{params, Connection};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProvenanceAssertionRecord {
    pub id: String,
    pub case_id: String,
    pub subject_domain: String,
    pub subject_id: String,
    pub relation_kind: String,
    pub source_data_source_id: Option<String>,
    pub source_file_id: Option<String>,
    pub confidence: String,
    pub basis: String,
    pub parser: String,
    pub parser_version: String,
    pub content_digest: Option<String>,
    pub details_json: String,
}

pub struct ProvenanceAssertionRepo<'a> {
    conn: &'a Connection,
}

impl<'a> ProvenanceAssertionRepo<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }
    pub fn insert(&self, record: &ProvenanceAssertionRecord) -> DbResult<()> {
        self.conn.execute(
            "INSERT OR REPLACE INTO provenance_assertions (id, case_id, subject_domain, subject_id, relation_kind, source_data_source_id, source_file_id, confidence, basis, parser, parser_version, content_digest, details_json) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
            params![record.id, record.case_id, record.subject_domain, record.subject_id, record.relation_kind, record.source_data_source_id, record.source_file_id, record.confidence, record.basis, record.parser, record.parser_version, record.content_digest, record.details_json],
        )?;
        Ok(())
    }
}
