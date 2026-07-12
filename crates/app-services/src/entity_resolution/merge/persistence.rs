use persistence_sqlite::repositories::entity_repo;
use rusqlite::Connection;

use super::grouping::merge_entities;
use super::ResolvedEntity;
use crate::entity_resolution::EntityResolutionError;

pub(super) fn deduplicate_entity_nodes(
    conn: &Connection,
    case_id: &str,
) -> Result<u64, EntityResolutionError> {
    let resolved = merge_entities(conn, case_id)?;
    let now = chrono::Utc::now().to_rfc3339();
    let mut merged_count = 0;

    for entity in &resolved {
        merged_count += merge_duplicate_group(conn, case_id, entity, &now)?;
    }
    for entity in &resolved {
        persist_resolved_entity(conn, case_id, entity)?;
    }
    Ok(merged_count)
}

fn merge_duplicate_group(
    conn: &Connection,
    case_id: &str,
    entity: &ResolvedEntity,
    now: &str,
) -> Result<u64, EntityResolutionError> {
    let Some(kept_id) = entity.source_entities.first() else {
        return Ok(0);
    };
    let mut merged_count = 0;
    for merged_id in entity.source_entities.iter().skip(1) {
        entity_repo::repoint_outgoing_edges(conn, kept_id, merged_id, case_id)
            .map_err(EntityResolutionError::Db)?;
        entity_repo::repoint_incoming_edges(conn, kept_id, merged_id, case_id)
            .map_err(EntityResolutionError::Db)?;
        log_merge(conn, case_id, kept_id, merged_id, entity.confidence, now)?;
        entity_repo::delete_graph_node(conn, merged_id, case_id)
            .map_err(EntityResolutionError::Db)?;
        merged_count += 1;
    }
    Ok(merged_count)
}

fn log_merge(
    conn: &Connection,
    case_id: &str,
    kept_id: &str,
    merged_id: &str,
    confidence: f64,
    now: &str,
) -> Result<(), EntityResolutionError> {
    let merge_id = format!("merge:{}:{}", case_id, uuid::Uuid::new_v4().as_simple());
    entity_repo::insert_merge_log(
        conn, &merge_id, case_id, kept_id, merged_id, confidence, now,
    )
    .map_err(EntityResolutionError::Db)
}

fn persist_resolved_entity(
    conn: &Connection,
    case_id: &str,
    entity: &ResolvedEntity,
) -> Result<(), EntityResolutionError> {
    let attrs_json = serde_json::to_string(&entity.attributes)?;
    entity_repo::upsert_resolved_entity(
        conn,
        &entity.id,
        case_id,
        &entity.entity_type,
        &entity.canonical_value,
        entity.source_entities.len() as u64,
        entity.confidence,
        &attrs_json,
    )
    .map_err(EntityResolutionError::Db)
}
