use std::collections::HashSet;

use chrono::Utc;
use persistence_sqlite::repositories::entity_repo;
use rusqlite::Connection;

use super::normalization::{hash_entity_value, normalize_entity_value};
use super::scan::{scan_artifacts, EntityMap};
use super::EntityExtractionError;

/// Scan case artifacts and upsert their normalized entity index rows.
pub fn index_entities(conn: &Connection, case_id: &str) -> Result<u64, EntityExtractionError> {
    let entities = scan_artifacts(conn, case_id)?;
    if entities.is_empty() {
        return Ok(0);
    }

    let now = Utc::now().to_rfc3339();
    let mut changed_rows = 0;
    for ((value, entity_type), artifact_ids) in entities {
        changed_rows += upsert_index_row(conn, &value, &entity_type, &artifact_ids, &now)?;
    }
    Ok(changed_rows)
}

/// Look up source artifact IDs by raw entity value and entity type.
pub fn lookup_entity(conn: &Connection, value: &str, entity_type: &str) -> Option<Vec<String>> {
    let normalized = normalize_entity_value(value);
    let hash = hash_entity_value(&normalized);
    let ids_json = entity_repo::find_entity_index_row(conn, &hash, entity_type).ok()??;
    let mut ids: Vec<String> = serde_json::from_str(&ids_json).ok()?;
    ids.sort();
    ids.dedup();
    Some(ids)
}

pub(super) fn entity_map_from_index(
    conn: &Connection,
    artifact_ids: &[String],
) -> Result<EntityMap, EntityExtractionError> {
    let artifact_set: HashSet<&str> = artifact_ids.iter().map(String::as_str).collect();
    let entries =
        entity_repo::list_all_entity_index_rows(conn).map_err(EntityExtractionError::Db)?;
    let mut entities = EntityMap::new();

    for entry in entries {
        let ids: Vec<String> = serde_json::from_str(&entry.source_artifact_ids).unwrap_or_default();
        let mut matching: Vec<String> = ids
            .into_iter()
            .filter(|id| artifact_set.contains(id.as_str()))
            .collect();
        matching.sort();
        matching.dedup();
        if !matching.is_empty() {
            entities
                .entry((entry.value_normalized, entry.entity_type))
                .or_default()
                .extend(matching);
        }
    }

    for source_ids in entities.values_mut() {
        source_ids.sort();
        source_ids.dedup();
    }
    Ok(entities)
}

fn upsert_index_row(
    conn: &Connection,
    value: &str,
    entity_type: &str,
    artifact_ids: &[String],
    now: &str,
) -> Result<u64, EntityExtractionError> {
    let normalized = normalize_entity_value(value);
    let hash = hash_entity_value(&normalized);
    let existing = entity_repo::find_entity_index_row(conn, &hash, entity_type)
        .map_err(EntityExtractionError::Db)?;

    if let Some(existing_json) = existing {
        return update_existing_row(conn, &hash, entity_type, &existing_json, artifact_ids, now);
    }

    let ids_json = serde_json::to_string(artifact_ids)?;
    entity_repo::upsert_entity_index(conn, &hash, entity_type, &normalized, &ids_json, now, now)
        .map_err(EntityExtractionError::Db)?;
    Ok(1)
}

fn update_existing_row(
    conn: &Connection,
    hash: &str,
    entity_type: &str,
    existing_json: &str,
    artifact_ids: &[String],
    now: &str,
) -> Result<u64, EntityExtractionError> {
    let mut existing_ids: Vec<String> = serde_json::from_str(existing_json).unwrap_or_default();
    let original_len = existing_ids.len();
    existing_ids.extend(artifact_ids.iter().cloned());
    existing_ids.sort();
    existing_ids.dedup();
    if existing_ids.len() == original_len {
        return Ok(0);
    }

    let merged = serde_json::to_string(&existing_ids)?;
    entity_repo::update_entity_index_source_ids(conn, hash, entity_type, &merged, now)
        .map_err(EntityExtractionError::Db)?;
    Ok(1)
}
