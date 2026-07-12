use persistence_sqlite::repositories::entity_repo;
use rusqlite::Connection;

use super::model::EntityRelationship;
use crate::entity_resolution::EntityResolutionError;

pub(super) fn persist_relationships(
    conn: &Connection,
    case_id: &str,
    relationships: &[EntityRelationship],
) -> Result<u64, EntityResolutionError> {
    if relationships.is_empty() {
        return Ok(0);
    }
    let transaction = conn.unchecked_transaction().map_err(|error| {
        EntityResolutionError::Other(format!("failed to begin transaction: {error}"))
    })?;
    let mut count = 0;
    for relationship in relationships
        .iter()
        .filter(|relationship| relationship.case_id == case_id)
    {
        persist_relationship(&transaction, relationship)?;
        count += 1;
    }
    transaction.commit().map_err(|error| {
        EntityResolutionError::Other(format!("failed to commit relationships: {error}"))
    })?;
    Ok(count)
}

fn persist_relationship(
    conn: &Connection,
    relationship: &EntityRelationship,
) -> Result<(), EntityResolutionError> {
    let edge_json = serde_json::to_string(&relationship.evidence_edge_ids)?;
    entity_repo::upsert_entity_relationship(
        conn,
        &relationship.id,
        &relationship.case_id,
        &relationship.source_entity_id,
        &relationship.target_entity_id,
        relationship.relationship_type.as_db_str(),
        relationship.confidence,
        &edge_json,
        &relationship.created_at,
    )
    .map_err(|error| {
        EntityResolutionError::Other(format!(
            "failed to insert relationship {}: {error}",
            relationship.id
        ))
    })
}
