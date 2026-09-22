use crate::connection::DbResult;
use rusqlite::{params, Connection};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InfrastructureGraphNodeRecord {
    pub id: String,
    pub domain: String,
    pub kind: String,
    pub name: String,
    pub status: String,
    pub confidence: String,
    pub provenance_json: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InfrastructureGraphEdgeRecord {
    pub source_id: String,
    pub target_id: String,
    pub relation_kind: String,
    pub confidence: String,
    pub provenance_json: String,
}

pub struct InfrastructureGraphRepo<'a> {
    conn: &'a Connection,
}

impl<'a> InfrastructureGraphRepo<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    pub fn list_nodes(&self, case_id: &str) -> DbResult<Vec<InfrastructureGraphNodeRecord>> {
        let mut statement = self.conn.prepare(
            "SELECT id, domain, kind, name, status, confidence, provenance_json
             FROM (
                 SELECT id, 'environment' AS domain, object_kind AS kind, name, status,
                        identity_state AS confidence, provenance_json
                 FROM environment_objects WHERE case_id = ?1
                 UNION ALL
                 SELECT id, 'storage' AS domain, object_kind AS kind, name, status,
                        identity_state AS confidence, provenance_json
                 FROM storage_objects WHERE case_id = ?1
                 UNION ALL
                 SELECT 'analysis:' || scope_id || ':' || data_source_id || ':' || IFNULL(file_id, artifact_kind),
                        'analysis' AS domain, artifact_kind AS kind, artifact_kind AS name,
                        status, 'candidate' AS confidence,
                        json_object('scopeId', scope_id, 'dataSourceId', data_source_id,
                                    'fileId', file_id, 'parser', parser)
                 FROM linux_topology_artifacts
                 WHERE data_source_id IN (SELECT id FROM data_sources WHERE case_id = ?1)
             )
             ORDER BY domain, kind, id",
        )?;
        let rows = statement.query_map([case_id], |row| {
            Ok(InfrastructureGraphNodeRecord {
                id: row.get(0)?,
                domain: row.get(1)?,
                kind: row.get(2)?,
                name: row.get(3)?,
                status: row.get(4)?,
                confidence: row.get(5)?,
                provenance_json: row.get(6)?,
            })
        })?;
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(Into::into)
    }

    pub fn list_edges(&self, case_id: &str) -> DbResult<Vec<InfrastructureGraphEdgeRecord>> {
        let mut statement = self.conn.prepare(
            "SELECT source_object_id, target_object_id, relation_kind, confidence, provenance_json
             FROM environment_relations
             WHERE source_object_id IN (SELECT id FROM environment_objects WHERE case_id = ?1)
             UNION ALL
             SELECT source_object_id, target_object_id, relation_kind, confidence, provenance_json
             FROM storage_relations
             WHERE source_object_id IN (SELECT id FROM storage_objects WHERE case_id = ?1)
             UNION ALL
             SELECT source_scope_id, target_scope_id, edge_kind, confidence, provenance_json
             FROM linux_topology_edges AS edge
             JOIN linux_topology_scopes AS source ON source.id = edge.source_scope_id
             WHERE source.case_id = ?1
             UNION ALL
             SELECT 'analysis:' || artifact.scope_id || ':' || artifact.data_source_id || ':' || IFNULL(artifact.file_id, artifact.artifact_kind),
                    REPLACE(artifact.scope_id, 'scope:kubernetes:', 'env:kubernetes:'),
                    'derived_from', 'candidate', artifact.diagnostics_json
             FROM linux_topology_artifacts AS artifact
             WHERE artifact.layer = 'orchestration'
               AND artifact.data_source_id IN (SELECT id FROM data_sources WHERE case_id = ?1)
             ORDER BY source_object_id, target_object_id, relation_kind",
        )?;
        let rows = statement.query_map(params![case_id], |row| {
            Ok(InfrastructureGraphEdgeRecord {
                source_id: row.get(0)?,
                target_id: row.get(1)?,
                relation_kind: row.get(2)?,
                confidence: row.get(3)?,
                provenance_json: row.get(4)?,
            })
        })?;
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(Into::into)
    }
}
