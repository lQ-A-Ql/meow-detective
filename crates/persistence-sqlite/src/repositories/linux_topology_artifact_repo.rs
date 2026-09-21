use crate::connection::{DbError, DbResult};
use rusqlite::{params, Connection};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinuxTopologyArtifactRecord {
    pub scope_id: String,
    pub data_source_id: String,
    pub file_id: Option<String>,
    pub layer: String,
    pub artifact_kind: String,
    pub parser: String,
    pub status: String,
    pub diagnostics_json: String,
    pub content_digest: Option<String>,
}

pub struct LinuxTopologyArtifactRepo<'a> {
    conn: &'a Connection,
}

impl<'a> LinuxTopologyArtifactRepo<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    pub fn insert(&self, record: &LinuxTopologyArtifactRecord) -> DbResult<()> {
        let same_case: bool = self.conn.query_row(
            "SELECT COUNT(*) = 1
             FROM linux_topology_scopes AS scope
             JOIN data_sources AS source ON source.id = ?2
             WHERE scope.id = ?1 AND scope.case_id = source.case_id",
            params![record.scope_id, record.data_source_id],
            |row| row.get(0),
        )?;
        if !same_case {
            return Err(DbError::System(
                "topology artifact source and scope must belong to the same case".to_string(),
            ));
        }
        self.conn.execute(
            "INSERT OR REPLACE INTO linux_topology_artifacts (
                scope_id, data_source_id, file_id, layer, artifact_kind, parser,
                status, diagnostics_json, content_digest
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                record.scope_id,
                record.data_source_id,
                record.file_id,
                record.layer,
                record.artifact_kind,
                record.parser,
                record.status,
                record.diagnostics_json,
                record.content_digest,
            ],
        )?;
        Ok(())
    }
}
