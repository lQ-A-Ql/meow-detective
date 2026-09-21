use crate::connection::{DbError, DbResult};
use rusqlite::{params, Connection};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinuxTopologyMembershipRecord {
    pub scope_id: String,
    pub data_source_id: String,
    pub role: String,
    pub member_index: Option<u32>,
    pub confidence: String,
    pub provenance_json: String,
}

pub struct LinuxTopologyMembershipRepo<'a> {
    conn: &'a Connection,
}

impl<'a> LinuxTopologyMembershipRepo<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    pub fn insert(&self, record: &LinuxTopologyMembershipRecord) -> DbResult<()> {
        if !valid_role(&record.role) {
            return invalid("topology membership role is invalid");
        }
        let same_case: bool = self.conn.query_row(
            "SELECT COUNT(*) = 1
             FROM linux_topology_scopes AS scope
             JOIN data_sources AS source ON source.id = ?2
             WHERE scope.id = ?1 AND scope.case_id = source.case_id",
            params![record.scope_id, record.data_source_id],
            |row| row.get(0),
        )?;
        if !same_case {
            return invalid("topology membership source and scope must belong to the same case");
        }
        if requires_provenance(&record.confidence, &record.provenance_json) {
            return invalid("proven topology relationship requires provenance");
        }
        self.conn.execute(
            "INSERT INTO linux_topology_memberships (
                scope_id, data_source_id, role, member_index, confidence, provenance_json
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                record.scope_id,
                record.data_source_id,
                record.role,
                record.member_index.map(i64::from),
                record.confidence,
                record.provenance_json,
            ],
        )?;
        Ok(())
    }

    pub fn find_source_ids(&self, scope_id: &str) -> DbResult<Vec<String>> {
        let mut statement = self.conn.prepare(
            "SELECT data_source_id FROM linux_topology_memberships
             WHERE scope_id = ?1 ORDER BY member_index ASC, data_source_id ASC",
        )?;
        let rows = statement.query_map([scope_id], |row| row.get(0))?;
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(Into::into)
    }
}

fn valid_role(role: &str) -> bool {
    matches!(
        role,
        "host"
            | "storage_node"
            | "control_plane"
            | "worker"
            | "virtual_machine"
            | "os_root"
            | "unknown"
    )
}

fn requires_provenance(confidence: &str, provenance_json: &str) -> bool {
    confidence == "proven" && (provenance_json.trim().is_empty() || provenance_json == "{}")
}

fn invalid<T>(message: &str) -> DbResult<T> {
    Err(DbError::System(message.to_string()))
}
