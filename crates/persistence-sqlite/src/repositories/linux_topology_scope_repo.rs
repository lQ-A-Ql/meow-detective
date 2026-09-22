use crate::connection::DbResult;
use rusqlite::{params, Connection, OptionalExtension};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinuxTopologyScopeRecord {
    pub id: String,
    pub case_id: String,
    pub scope_kind: String,
    pub name: String,
    pub identity_state: String,
    pub identity_fingerprint: Option<String>,
    pub status: String,
    pub evidence_completeness: String,
    pub diagnostics_json: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinuxTopologyScopeSummary {
    pub id: String,
    pub case_id: String,
    pub scope_kind: String,
    pub name: String,
    pub status: String,
    pub evidence_completeness: String,
    pub member_count: u32,
}

pub struct LinuxTopologyScopeRepo<'a> {
    conn: &'a Connection,
}

impl<'a> LinuxTopologyScopeRepo<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    pub fn insert(&self, record: &LinuxTopologyScopeRecord) -> DbResult<()> {
        self.conn.execute(
            "INSERT INTO linux_topology_scopes (
                id, case_id, scope_kind, name, identity_state, identity_fingerprint,
                status, evidence_completeness, diagnostics_json
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                record.id,
                record.case_id,
                record.scope_kind,
                record.name,
                record.identity_state,
                record.identity_fingerprint,
                record.status,
                record.evidence_completeness,
                record.diagnostics_json,
            ],
        )?;
        Ok(())
    }

    pub fn delete(&self, scope_id: &str) -> DbResult<()> {
        self.conn.execute(
            "DELETE FROM linux_topology_scopes WHERE id = ?1",
            [scope_id],
        )?;
        Ok(())
    }

    pub fn find_kind(&self, scope_id: &str) -> DbResult<Option<String>> {
        self.conn
            .query_row(
                "SELECT scope_kind FROM linux_topology_scopes WHERE id = ?1",
                [scope_id],
                |row| row.get(0),
            )
            .optional()
            .map_err(Into::into)
    }

    pub fn find_summary(
        &self,
        case_id: &str,
        scope_id: &str,
    ) -> DbResult<Option<LinuxTopologyScopeSummary>> {
        self.conn
            .query_row(
                "SELECT scope.id, scope.case_id, scope.scope_kind, scope.name,
                    scope.status, scope.evidence_completeness,
                    COUNT(membership.data_source_id)
             FROM linux_topology_scopes AS scope
             LEFT JOIN linux_topology_memberships AS membership
               ON membership.scope_id = scope.id
             WHERE scope.id = ?1 AND scope.case_id = ?2
             GROUP BY scope.id, scope.case_id, scope.scope_kind, scope.name,
                      scope.status, scope.evidence_completeness",
                params![scope_id, case_id],
                |row| {
                    Ok(LinuxTopologyScopeSummary {
                        id: row.get(0)?,
                        case_id: row.get(1)?,
                        scope_kind: row.get(2)?,
                        name: row.get(3)?,
                        status: row.get(4)?,
                        evidence_completeness: row.get(5)?,
                        member_count: row.get::<_, i64>(6)?.max(0) as u32,
                    })
                },
            )
            .optional()
            .map_err(Into::into)
    }

    pub fn find_for_source(
        &self,
        case_id: &str,
        data_source_id: &str,
        scope_kind: &str,
    ) -> DbResult<Option<LinuxTopologyScopeSummary>> {
        self.conn
            .query_row(
                "SELECT scope.id, scope.case_id, scope.scope_kind, scope.name,
                        scope.status, scope.evidence_completeness,
                        COUNT(membership.data_source_id)
                 FROM linux_topology_scopes AS scope
                 JOIN linux_topology_memberships AS membership
                   ON membership.scope_id = scope.id
                 WHERE scope.case_id = ?1
                   AND scope.scope_kind = ?2
                   AND membership.data_source_id = ?3
                 GROUP BY scope.id, scope.case_id, scope.scope_kind, scope.name,
                          scope.status, scope.evidence_completeness
                 ORDER BY scope.id ASC
                 LIMIT 1",
                params![case_id, scope_kind, data_source_id],
                |row| {
                    Ok(LinuxTopologyScopeSummary {
                        id: row.get(0)?,
                        case_id: row.get(1)?,
                        scope_kind: row.get(2)?,
                        name: row.get(3)?,
                        status: row.get(4)?,
                        evidence_completeness: row.get(5)?,
                        member_count: row.get::<_, i64>(6)?.max(0) as u32,
                    })
                },
            )
            .optional()
            .map_err(Into::into)
    }
}

#[cfg(test)]
#[path = "../../tests/unit/repositories/linux_topology_repo.rs"]
mod tests;
