//! Repository for entity extraction and resolution tables.
//!
//! Covers:
//! - `entity_index` — normalized+hashed entity values for deduplication.
//! - `entity_merge_log` — audit trail of entity merge operations.
//! - `resolved_entities` — denormalised resolved entity lookup.
//! - `entity_relationships` — inferred semantic relationships between entities.
//! - Entity-related graph operations (delete entity nodes, edge repointing).

use crate::connection::{DbError, DbResult};
use rusqlite::{params, Connection, OptionalExtension};

// ── entity_index ──────────────────────────────────────────────────────

/// A row from the entity_index table.
#[derive(Debug, Clone)]
pub struct EntityIndexEntry {
    pub value_hash: String,
    pub entity_type: String,
    pub value_normalized: String,
    pub source_artifact_ids: String, // JSON array of artifact IDs
}

/// Check whether an entity_index row exists for a (hash, entity_type) pair and
/// return the serialized source_artifact_ids JSON if present.
pub fn get_entity_index_row(
    conn: &Connection,
    value_hash: &str,
    entity_type: &str,
) -> DbResult<Option<String>> {
    conn.query_row(
        "SELECT source_artifact_ids FROM entity_index
         WHERE value_hash = ?1 AND entity_type = ?2",
        params![value_hash, entity_type],
        |row| row.get(0),
    )
    .optional()
    .map_err(DbError::from)
}

/// Upsert a row into entity_index.
pub fn upsert_entity_index(
    conn: &Connection,
    value_hash: &str,
    entity_type: &str,
    value_normalized: &str,
    source_artifact_ids_json: &str,
    created_at: &str,
    updated_at: &str,
) -> DbResult<()> {
    conn.execute(
        "INSERT INTO entity_index
         (value_hash, entity_type, value_normalized, source_artifact_ids, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)
         ON CONFLICT(value_hash, entity_type) DO UPDATE SET
            value_normalized = excluded.value_normalized,
            source_artifact_ids = excluded.source_artifact_ids,
            updated_at = excluded.updated_at",
        params![
            value_hash,
            entity_type,
            value_normalized,
            source_artifact_ids_json,
            created_at,
            updated_at
        ],
    )?;
    Ok(())
}

/// Update the source_artifact_ids for an existing entity_index row.
pub fn update_entity_index_source_ids(
    conn: &Connection,
    value_hash: &str,
    entity_type: &str,
    merged_json: &str,
    updated_at: &str,
) -> DbResult<()> {
    conn.execute(
        "UPDATE entity_index
         SET source_artifact_ids = ?1, updated_at = ?2
         WHERE value_hash = ?3 AND entity_type = ?4",
        params![merged_json, updated_at, value_hash, entity_type],
    )?;
    Ok(())
}

/// Look up an entity by its normalized hash and type, returning the
/// source_artifact_ids JSON string.
pub fn find_entity_index_row(
    conn: &Connection,
    value_hash: &str,
    entity_type: &str,
) -> DbResult<Option<String>> {
    get_entity_index_row(conn, value_hash, entity_type)
}

/// Load all entity_index rows.
pub fn list_all_entity_index_rows(conn: &Connection) -> DbResult<Vec<EntityIndexEntry>> {
    let mut stmt = conn.prepare(
        "SELECT value_hash, entity_type, value_normalized, source_artifact_ids FROM entity_index",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok(EntityIndexEntry {
            value_hash: row.get(0)?,
            entity_type: row.get(1)?,
            value_normalized: row.get(2)?,
            source_artifact_ids: row.get(3)?,
        })
    })?;
    let mut entries = Vec::new();
    for row in rows {
        entries.push(row?);
    }
    Ok(entries)
}

// ── Entity graph operations ───────────────────────────────────────────

/// Delete all entity-type graph nodes for a case (used before re-extraction).
pub fn delete_entity_nodes(conn: &Connection, case_id: &str) -> DbResult<()> {
    conn.execute(
        "DELETE FROM graph_nodes WHERE case_id = ?1 AND node_type = 'entity'",
        params![case_id],
    )?;
    Ok(())
}

/// Get IDs of existing artifact graph nodes for a case.
pub fn get_existing_artifact_node_ids(conn: &Connection, case_id: &str) -> DbResult<Vec<String>> {
    let mut stmt =
        conn.prepare("SELECT id FROM graph_nodes WHERE case_id = ?1 AND node_type = 'artifact'")?;
    let ids: Vec<String> = stmt
        .query_map(params![case_id], |row| row.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(ids)
}

// ── Entity merge operations ───────────────────────────────────────────

/// Repoint outgoing edges from one node to another within a case.
pub fn repoint_outgoing_edges(
    conn: &Connection,
    kept_id: &str,
    merged_id: &str,
    case_id: &str,
) -> DbResult<()> {
    conn.execute(
        "UPDATE graph_edges SET source_id = ?1
         WHERE source_id = ?2 AND case_id = ?3",
        params![kept_id, merged_id, case_id],
    )?;
    Ok(())
}

/// Repoint incoming edges from one node to another within a case.
pub fn repoint_incoming_edges(
    conn: &Connection,
    kept_id: &str,
    merged_id: &str,
    case_id: &str,
) -> DbResult<()> {
    conn.execute(
        "UPDATE graph_edges SET target_id = ?1
         WHERE target_id = ?2 AND case_id = ?3",
        params![kept_id, merged_id, case_id],
    )?;
    Ok(())
}

/// Delete a single graph node by id and case_id.
pub fn delete_graph_node(conn: &Connection, node_id: &str, case_id: &str) -> DbResult<()> {
    conn.execute(
        "DELETE FROM graph_nodes WHERE id = ?1 AND case_id = ?2",
        params![node_id, case_id],
    )?;
    Ok(())
}

/// Insert a merge log entry.
pub fn insert_merge_log(
    conn: &Connection,
    merge_id: &str,
    case_id: &str,
    kept_entity_id: &str,
    merged_entity_id: &str,
    confidence: f64,
    merged_at: &str,
) -> DbResult<()> {
    conn.execute(
        "INSERT INTO entity_merge_log
         (merge_id, case_id, kept_entity_id, merged_entity_id,
          confidence, merged_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![
            merge_id,
            case_id,
            kept_entity_id,
            merged_entity_id,
            confidence,
            merged_at
        ],
    )?;
    Ok(())
}

// ── Resolved entities ───────────────────────────────────────────────

/// Upsert a resolved entity into the denormalized lookup table.
#[allow(clippy::too_many_arguments)]
pub fn upsert_resolved_entity(
    conn: &Connection,
    id: &str,
    case_id: &str,
    entity_type: &str,
    canonical_value: &str,
    source_count: u64,
    confidence: f64,
    attributes_json: &str,
) -> DbResult<()> {
    conn.execute(
        "INSERT OR REPLACE INTO resolved_entities
         (id, case_id, entity_type, canonical_value, source_count,
          confidence, attributes_json)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            id,
            case_id,
            entity_type,
            canonical_value,
            source_count as i64,
            confidence,
            attributes_json
        ],
    )?;
    Ok(())
}

// ── Entity relationships ─────────────────────────────────────────────

/// Insert or replace an entity relationship row.
#[allow(clippy::too_many_arguments)]
pub fn upsert_entity_relationship(
    conn: &Connection,
    id: &str,
    case_id: &str,
    source_entity_id: &str,
    target_entity_id: &str,
    relationship_type: &str,
    confidence: f64,
    evidence_edge_ids_json: &str,
    created_at: &str,
) -> DbResult<()> {
    conn.execute(
        "INSERT OR REPLACE INTO entity_relationships
         (id, case_id, source_entity_id, target_entity_id,
          relationship_type, confidence, evidence_edge_ids, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![
            id,
            case_id,
            source_entity_id,
            target_entity_id,
            relationship_type,
            confidence,
            evidence_edge_ids_json,
            created_at,
        ],
    )?;
    Ok(())
}

/// Query entity nodes for a case (id, label, tags).
pub fn query_entity_nodes(
    conn: &Connection,
    case_id: &str,
) -> DbResult<Vec<(String, String, String)>> {
    let mut stmt = conn.prepare(
        "SELECT id, label, tags FROM graph_nodes
         WHERE case_id = ?1 AND node_type = 'entity'",
    )?;
    let rows = stmt
        .query_map(params![case_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// Get artifact ids for a case.
pub fn get_artifact_ids_for_case(conn: &Connection, case_id: &str) -> DbResult<Vec<String>> {
    let mut stmt = conn.prepare("SELECT id FROM artifacts WHERE case_id = ?1")?;
    let ids: Vec<String> = stmt
        .query_map(params![case_id], |row| row.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(ids)
}

/// Get artifact rows (id, title, summary, attrs) for a case.
pub fn get_artifact_rows_for_case(
    conn: &Connection,
    case_id: &str,
) -> DbResult<Vec<(String, String, String, String)>> {
    let mut stmt =
        conn.prepare("SELECT id, title, summary, attrs FROM artifacts WHERE case_id = ?1")?;
    let rows: Vec<(String, String, String, String)> = stmt
        .query_map(params![case_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

// ── Tests ─────────────────────────────────────────────────────────────

#[cfg(test)]
#[path = "../../tests/unit/repositories/entity_repo.rs"]
mod tests;
