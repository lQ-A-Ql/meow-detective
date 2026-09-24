use domain::CaseId;
use persistence_sqlite::repositories::provenance_assertion_repo::{
    ProvenanceAssertionRecord, ProvenanceAssertionRepo,
};
use rusqlite::Connection;

use super::Result;

pub(super) fn persist_topology_provenance(
    conn: &Connection,
    case_id: &CaseId,
    import_set_id: &str,
) -> Result<()> {
    let pattern = format!("scope:%:{import_set_id}%");
    persist_membership_provenance(conn, case_id, &pattern)?;
    persist_edge_provenance(conn, case_id, &pattern)?;
    persist_lineage_provenance(conn, case_id)?;
    Ok(())
}

fn persist_membership_provenance(conn: &Connection, case_id: &CaseId, pattern: &str) -> Result<()> {
    let repo = ProvenanceAssertionRepo::new(conn);
    let mut membership_query = conn.prepare(
        "SELECT membership.scope_id, membership.data_source_id, membership.role,
                membership.confidence, membership.provenance_json
         FROM linux_topology_memberships AS membership
         JOIN linux_topology_scopes AS scope ON scope.id = membership.scope_id
         WHERE scope.case_id = ?1 AND scope.id LIKE ?2",
    )?;
    let rows = membership_query.query_map(rusqlite::params![case_id.0, pattern], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, String>(4)?,
        ))
    })?;
    for row in rows {
        let (scope_id, source_id, role, confidence, provenance) = row?;
        repo.insert(&ProvenanceAssertionRecord {
            id: format!("topology-membership:{scope_id}:{source_id}:{role}"),
            case_id: case_id.0.clone(),
            subject_domain: "environment".to_string(),
            subject_id: scope_id.clone(),
            relation_kind: "member_of".to_string(),
            source_data_source_id: Some(source_id),
            source_file_id: None,
            confidence,
            basis: provenance,
            parser: "linux.topology_projection".to_string(),
            parser_version: "1".to_string(),
            content_digest: None,
            details_json: serde_json::json!({ "scopeId": scope_id, "role": role }).to_string(),
        })?;
    }
    Ok(())
}

fn persist_edge_provenance(conn: &Connection, case_id: &CaseId, pattern: &str) -> Result<()> {
    let repo = ProvenanceAssertionRepo::new(conn);
    let mut edge_query = conn.prepare(
        "SELECT edge.source_scope_id, edge.target_scope_id, edge.edge_kind,
                edge.confidence, edge.provenance_json
         FROM linux_topology_edges AS edge
         JOIN linux_topology_scopes AS source ON source.id = edge.source_scope_id
         WHERE source.case_id = ?1 AND source.id LIKE ?2",
    )?;
    let rows = edge_query.query_map(rusqlite::params![case_id.0, pattern], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, String>(4)?,
        ))
    })?;
    for row in rows {
        let (source_scope_id, target_scope_id, edge_kind, confidence, provenance) = row?;
        repo.insert(&ProvenanceAssertionRecord {
            id: format!("topology-edge:{source_scope_id}:{target_scope_id}:{edge_kind}"),
            case_id: case_id.0.clone(),
            subject_domain: "environment".to_string(),
            subject_id: source_scope_id.clone(),
            relation_kind: edge_kind.clone(),
            source_data_source_id: None,
            source_file_id: None,
            confidence,
            basis: provenance,
            parser: "linux.topology_projection".to_string(),
            parser_version: "1".to_string(),
            content_digest: None,
            details_json: serde_json::json!({ "targetScopeId": target_scope_id }).to_string(),
        })?;
    }
    Ok(())
}

fn persist_lineage_provenance(conn: &Connection, case_id: &CaseId) -> Result<()> {
    let repo = ProvenanceAssertionRepo::new(conn);
    let mut lineage_query = conn.prepare(
        "SELECT derived_data_source_id, parent_ceph_scope_id, 'ceph_rbd'
         FROM ceph_rbd_derived_lineage
         UNION ALL
         SELECT derived_data_source_id, parent_ceph_scope_id, 'ceph_fs'
         FROM ceph_fs_derived_lineage",
    )?;
    let lineages = lineage_query.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
        ))
    })?;
    for lineage in lineages {
        let (derived_id, parent_scope_id, kind) = lineage?;
        repo.insert(&ProvenanceAssertionRecord {
            id: format!("derived-lineage:{derived_id}"),
            case_id: case_id.0.clone(),
            subject_domain: "storage".to_string(),
            subject_id: derived_id,
            relation_kind: "derived_from".to_string(),
            source_data_source_id: None,
            source_file_id: None,
            confidence: "candidate".to_string(),
            basis: "persisted derived-source lineage".to_string(),
            parser: "cluster.derived_lineage".to_string(),
            parser_version: "1".to_string(),
            content_digest: None,
            details_json: serde_json::json!({
                "parentScopeId": parent_scope_id,
                "kind": kind
            })
            .to_string(),
        })?;
    }
    Ok(())
}

pub(super) fn persist_analysis_links(
    case_connection: &Connection,
    case_root: &std::path::Path,
    case_id: &CaseId,
    import_set_id: &str,
    member_sources: &[(u32, String, String)],
) -> Result<()> {
    for (member_index, source_id, _) in member_sources {
        let source_connection = crate::source_db::open_registered_source_db_read_only(
            case_connection,
            case_root,
            &domain::DataSourceId(source_id.clone()),
        )?;
        let environment_object_id = format!("env:os:{import_set_id}:{member_index}");
        let mut statement = source_connection.prepare("SELECT id FROM artifacts ORDER BY id")?;
        let artifact_ids = statement
            .query_map([], |row| row.get::<_, String>(0))?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        for artifact_id in artifact_ids {
            case_connection.execute(
                "INSERT OR IGNORE INTO analysis_object_links
                 (analysis_object_id, analysis_kind, environment_object_id,
                  storage_object_id, evidence_source_id, relation_kind, provenance_assertion_id)
                 VALUES (?1, 'artifact', ?2, NULL, ?3, 'observed_on', NULL)",
                rusqlite::params![artifact_id, environment_object_id, source_id],
            )?;
        }
        let mut statement =
            source_connection.prepare("SELECT id FROM timeline_events ORDER BY id")?;
        let timeline_ids = statement
            .query_map([], |row| row.get::<_, String>(0))?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        for timeline_id in timeline_ids {
            case_connection.execute(
                "INSERT OR IGNORE INTO analysis_object_links
                 (analysis_object_id, analysis_kind, environment_object_id,
                  storage_object_id, evidence_source_id, relation_kind, provenance_assertion_id)
                 VALUES (?1, 'timeline_event', ?2, NULL, ?3, 'observed_on', NULL)",
                rusqlite::params![timeline_id, environment_object_id, source_id],
            )?;
        }
    }
    let _ = case_id;
    Ok(())
}
