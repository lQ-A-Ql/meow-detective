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
    persist_membership_provenance(conn, case_id, import_set_id)?;
    persist_edge_provenance(conn, case_id, import_set_id)?;
    persist_lineage_provenance(conn, case_id, import_set_id)?;
    Ok(())
}

fn persist_membership_provenance(
    conn: &Connection,
    case_id: &CaseId,
    import_set_id: &str,
) -> Result<()> {
    let repo = ProvenanceAssertionRepo::new(conn);
    let mut membership_query = conn.prepare(
        "SELECT membership.scope_id, membership.data_source_id, membership.role,
                membership.confidence, membership.provenance_json
         FROM linux_topology_memberships AS membership
         JOIN linux_topology_scopes AS scope ON scope.id = membership.scope_id
         WHERE scope.case_id = ?1
           AND substr(scope.id, 1, 6) = 'scope:'
           AND (
               substr(scope.id, 7 + instr(substr(scope.id, 7), ':')) = ?2
               OR substr(scope.id, 7 + instr(substr(scope.id, 7), ':'), length(?2) + 1) = ?2 || ':'
           )",
    )?;
    let rows = membership_query.query_map(rusqlite::params![case_id.0, import_set_id], |row| {
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

fn persist_edge_provenance(conn: &Connection, case_id: &CaseId, import_set_id: &str) -> Result<()> {
    let repo = ProvenanceAssertionRepo::new(conn);
    let mut edge_query = conn.prepare(
        "SELECT edge.source_scope_id, edge.target_scope_id, edge.edge_kind,
                edge.confidence, edge.provenance_json
         FROM linux_topology_edges AS edge
         JOIN linux_topology_scopes AS source ON source.id = edge.source_scope_id
         WHERE source.case_id = ?1
           AND substr(source.id, 1, 6) = 'scope:'
           AND (
               substr(source.id, 7 + instr(substr(source.id, 7), ':')) = ?2
               OR substr(source.id, 7 + instr(substr(source.id, 7), ':'), length(?2) + 1) = ?2 || ':'
           )",
    )?;
    let rows = edge_query.query_map(rusqlite::params![case_id.0, import_set_id], |row| {
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

fn persist_lineage_provenance(
    conn: &Connection,
    case_id: &CaseId,
    import_set_id: &str,
) -> Result<()> {
    let repo = ProvenanceAssertionRepo::new(conn);
    let mut lineage_query = conn.prepare(
        "SELECT lineage.derived_data_source_id, lineage.parent_ceph_scope_id, 'ceph_rbd'
         FROM ceph_rbd_derived_lineage AS lineage
         JOIN data_sources AS source ON source.id = lineage.derived_data_source_id
         WHERE source.case_id = ?1 AND lineage.parent_ceph_scope_id = ?2
         UNION ALL
         SELECT lineage.derived_data_source_id, lineage.parent_ceph_scope_id, 'ceph_fs'
         FROM ceph_fs_derived_lineage AS lineage
         JOIN data_sources AS source ON source.id = lineage.derived_data_source_id
         WHERE source.case_id = ?1 AND lineage.parent_ceph_scope_id = ?2",
    )?;
    let scope_id = format!("scope:ceph:{import_set_id}");
    let lineages = lineage_query.query_map(rusqlite::params![case_id.0, scope_id], |row| {
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
    member_sources: &[(u32, String, String)],
) -> Result<()> {
    let mut artifact_insert = case_connection.prepare_cached(
        "INSERT OR IGNORE INTO analysis_object_links
         (analysis_object_id, analysis_kind, environment_object_id,
          storage_object_id, evidence_source_id, relation_kind, provenance_assertion_id)
         VALUES (?1, 'artifact', ?2, NULL, ?3, 'observed_on', NULL)",
    )?;
    let mut timeline_insert = case_connection.prepare_cached(
        "INSERT OR IGNORE INTO analysis_object_links
         (analysis_object_id, analysis_kind, environment_object_id,
          storage_object_id, evidence_source_id, relation_kind, provenance_assertion_id)
         VALUES (?1, 'timeline_event', ?2, NULL, ?3, 'observed_on', NULL)",
    )?;
    for (_, source_id, _) in member_sources {
        let case_id =
            persistence_sqlite::repositories::datasource_repo::DataSourceRepo::new(case_connection)
                .case_id(&domain::DataSourceId(source_id.clone()))?;
        let reference =
            persistence_sqlite::repositories::source_revision_repo::SourceRevisionRepo::new(
                case_connection,
            )
            .current(&case_id.0, source_id)?
            .0;
        let environment_object_id = format!("source:{source_id}@{}", reference.revision);
        let fjall = source_kernel::source_db::open_registered_fjall_source_read_only(
            case_root,
            &domain::DataSourceId(source_id.clone()),
        )
        .map_err(|error| persistence_sqlite::DbError::System(error.to_string()))?;
        let snapshot = fjall.snapshot();
        let mut cursor = None;
        loop {
            let page = fjall
                .artifact_list_by_source(
                    &snapshot,
                    source_id,
                    cursor.as_deref(),
                    persistence_source::fjall::ScanDirection::Forward,
                    512,
                )
                .map_err(|error| persistence_sqlite::DbError::System(error.to_string()))?;
            for record in page.records {
                artifact_insert.execute(rusqlite::params![
                    record.id,
                    environment_object_id,
                    source_id,
                ])?;
            }
            cursor = page.next_cursor;
            if cursor.is_none() {
                break;
            }
        }
        let mut cursor = None;
        loop {
            let page = fjall
                .timeline_list_by_time(
                    &snapshot,
                    source_id,
                    cursor.as_deref(),
                    persistence_source::fjall::ScanDirection::Forward,
                    512,
                )
                .map_err(|error| persistence_sqlite::DbError::System(error.to_string()))?;
            for record in page.records {
                timeline_insert.execute(rusqlite::params![
                    record.id,
                    environment_object_id,
                    source_id,
                ])?;
            }
            cursor = page.next_cursor;
            if cursor.is_none() {
                break;
            }
        }
    }
    Ok(())
}
