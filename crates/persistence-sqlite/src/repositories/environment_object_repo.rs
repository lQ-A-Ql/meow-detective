use crate::connection::{DbError, DbResult};
use rusqlite::{params, Connection, OptionalExtension};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnvironmentObjectRecord {
    pub id: String,
    pub case_id: String,
    pub object_kind: String,
    pub name: String,
    pub identity_state: String,
    pub status: String,
    pub provenance_json: String,
}

pub struct EnvironmentObjectRepo<'a> {
    conn: &'a Connection,
}

impl<'a> EnvironmentObjectRepo<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }
    pub fn insert_if_absent(&self, record: &EnvironmentObjectRecord) -> DbResult<()> {
        self.conn.execute("INSERT OR IGNORE INTO environment_objects (id, case_id, object_kind, name, identity_state, status, provenance_json) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)", params![record.id, record.case_id, record.object_kind, record.name, record.identity_state, record.status, record.provenance_json])?;
        Ok(())
    }
    pub fn find(&self, object_id: &str) -> DbResult<Option<EnvironmentObjectRecord>> {
        self.conn.query_row("SELECT id, case_id, object_kind, name, identity_state, status, provenance_json FROM environment_objects WHERE id = ?1", [object_id], |row| Ok(EnvironmentObjectRecord { id: row.get(0)?, case_id: row.get(1)?, object_kind: row.get(2)?, name: row.get(3)?, identity_state: row.get(4)?, status: row.get(5)?, provenance_json: row.get(6)? })).optional().map_err(Into::into)
    }
    pub fn insert_relation(
        &self,
        source_object_id: &str,
        target_object_id: &str,
        relation_kind: &str,
        confidence: &str,
        provenance_json: &str,
    ) -> DbResult<()> {
        let same_case: bool = self.conn.query_row("SELECT COUNT(*) = 1 FROM environment_objects AS source JOIN environment_objects AS target ON target.id = ?2 WHERE source.id = ?1 AND source.case_id = target.case_id", params![source_object_id, target_object_id], |row| row.get(0))?;
        if !same_case {
            return Err(DbError::System(
                "environment relation objects must share a case".to_string(),
            ));
        }
        self.conn.execute("INSERT INTO environment_relations (source_object_id, target_object_id, relation_kind, confidence, provenance_json) VALUES (?1, ?2, ?3, ?4, ?5)", params![source_object_id, target_object_id, relation_kind, confidence, provenance_json])?;
        Ok(())
    }
}
