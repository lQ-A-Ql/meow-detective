use crate::connection::{DbError, DbResult};
use rusqlite::{params, Connection};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinuxTopologyEdgeRecord {
    pub source_scope_id: String,
    pub target_scope_id: String,
    pub edge_kind: String,
    pub confidence: String,
    pub provenance_json: String,
}

pub struct LinuxTopologyEdgeRepo<'a> {
    conn: &'a Connection,
}

impl<'a> LinuxTopologyEdgeRepo<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    pub fn insert(&self, record: &LinuxTopologyEdgeRecord) -> DbResult<()> {
        let same_case: bool = self.conn.query_row(
            "SELECT COUNT(*) = 1
             FROM linux_topology_scopes AS source
             JOIN linux_topology_scopes AS target ON target.id = ?2
             WHERE source.id = ?1 AND source.case_id = target.case_id",
            params![record.source_scope_id, record.target_scope_id],
            |row| row.get(0),
        )?;
        if !same_case {
            return Err(DbError::System(
                "topology edge scopes must belong to the same case".to_string(),
            ));
        }
        if record.confidence == "proven"
            && (record.provenance_json.trim().is_empty() || record.provenance_json == "{}")
        {
            return Err(DbError::System(
                "proven topology relationship requires provenance".to_string(),
            ));
        }
        self.conn.execute(
            "INSERT INTO linux_topology_edges (
                source_scope_id, target_scope_id, edge_kind, confidence, provenance_json
             ) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                record.source_scope_id,
                record.target_scope_id,
                record.edge_kind,
                record.confidence,
                record.provenance_json,
            ],
        )?;
        Ok(())
    }
}
