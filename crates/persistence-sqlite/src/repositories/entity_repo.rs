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

/// Query graph nodes by type for a case (id, label, summary).
pub fn query_nodes_by_type(
    conn: &Connection,
    case_id: &str,
    node_type: &str,
) -> DbResult<Vec<(String, String, String)>> {
    let mut stmt = conn.prepare(
        "SELECT id, label, summary FROM graph_nodes WHERE case_id = ?1 AND node_type = ?2",
    )?;
    let rows = stmt
        .query_map(params![case_id, node_type], |row| {
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
mod tests {
    use super::*;
    use crate::connection::open_in_memory;

    fn setup_db() -> Connection {
        let conn = open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE entity_index (
                value_hash TEXT NOT NULL,
                entity_type TEXT NOT NULL,
                value_normalized TEXT NOT NULL,
                source_artifact_ids TEXT NOT NULL DEFAULT '[]',
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                PRIMARY KEY (value_hash, entity_type)
            );
            CREATE TABLE graph_nodes (
                id TEXT PRIMARY KEY NOT NULL,
                case_id TEXT NOT NULL,
                node_type TEXT NOT NULL,
                label TEXT NOT NULL,
                summary TEXT NOT NULL DEFAULT '',
                tags TEXT NOT NULL DEFAULT '[]',
                created_at TEXT NOT NULL
            );
            CREATE TABLE graph_edges (
                id TEXT PRIMARY KEY NOT NULL,
                case_id TEXT NOT NULL,
                source_id TEXT NOT NULL,
                target_id TEXT NOT NULL,
                edge_type TEXT NOT NULL,
                confidence REAL,
                provenance TEXT,
                created_at TEXT NOT NULL
            );
            CREATE TABLE entity_merge_log (
                merge_id TEXT PRIMARY KEY NOT NULL,
                case_id TEXT NOT NULL,
                kept_entity_id TEXT NOT NULL,
                merged_entity_id TEXT NOT NULL,
                confidence REAL NOT NULL,
                merged_at TEXT NOT NULL
            );
            CREATE TABLE resolved_entities (
                id TEXT PRIMARY KEY NOT NULL,
                case_id TEXT NOT NULL,
                entity_type TEXT NOT NULL,
                canonical_value TEXT NOT NULL,
                source_count INTEGER NOT NULL DEFAULT 0,
                confidence REAL NOT NULL DEFAULT 0.0,
                attributes_json TEXT NOT NULL DEFAULT '[]'
            );
            CREATE TABLE entity_relationships (
                id TEXT PRIMARY KEY NOT NULL,
                case_id TEXT NOT NULL,
                source_entity_id TEXT NOT NULL,
                target_entity_id TEXT NOT NULL,
                relationship_type TEXT NOT NULL,
                confidence REAL NOT NULL DEFAULT 0.0,
                evidence_edge_ids TEXT NOT NULL DEFAULT '[]',
                created_at TEXT NOT NULL
            );
            CREATE TABLE artifacts (
                id TEXT PRIMARY KEY NOT NULL,
                case_id TEXT NOT NULL,
                title TEXT NOT NULL,
                summary TEXT NOT NULL DEFAULT '',
                attrs TEXT NOT NULL DEFAULT '{}'
            );",
        )
        .unwrap();
        conn
    }

    #[test]
    fn upsert_and_find_entity_index() {
        let conn = setup_db();
        let hash = "abc123def4567890";
        let entity_type = "person";
        let now = "2026-01-01T00:00:00Z";

        upsert_entity_index(
            &conn,
            hash,
            entity_type,
            "alice@example.com",
            r#"["a1"]"#,
            now,
            now,
        )
        .unwrap();

        let row = find_entity_index_row(&conn, hash, entity_type)
            .unwrap()
            .expect("row should exist");
        assert_eq!(row, r#"["a1"]"#);

        // Non-existent entry returns None
        assert!(find_entity_index_row(&conn, "no-such-hash", "person")
            .unwrap()
            .is_none());
    }

    #[test]
    fn upsert_entity_index_overwrites_on_conflict() {
        let conn = setup_db();
        let hash = "abc123def4567890";
        let now = "2026-01-01T00:00:00Z";
        let later = "2026-01-02T00:00:00Z";

        // First insert
        upsert_entity_index(
            &conn,
            hash,
            "person",
            "alice@example.com",
            r#"["a1"]"#,
            now,
            now,
        )
        .unwrap();

        // Second insert with same (hash, type) overwrites
        upsert_entity_index(
            &conn,
            hash,
            "person",
            "alice@example.com",
            r#"["a1","a2"]"#,
            later,
            later,
        )
        .unwrap();

        let row = find_entity_index_row(&conn, hash, "person")
            .unwrap()
            .unwrap();
        assert_eq!(row, r#"["a1","a2"]"#);
    }

    #[test]
    fn delete_entity_nodes_removes_only_entity_type() {
        let conn = setup_db();
        conn.execute(
            "INSERT INTO graph_nodes (id, case_id, node_type, label, created_at)
             VALUES ('n1', 'case-1', 'entity', 'alice@example.com', '2026-01-01T00:00:00Z')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO graph_nodes (id, case_id, node_type, label, created_at)
             VALUES ('n2', 'case-1', 'artifact', 'artifact-1', '2026-01-01T00:00:00Z')",
            [],
        )
        .unwrap();

        delete_entity_nodes(&conn, "case-1").unwrap();

        // Entity node should be deleted
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM graph_nodes WHERE id = 'n1'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 0);

        // Artifact node should remain
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM graph_nodes WHERE id = 'n2'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn repoint_edges_and_delete_node() {
        let conn = setup_db();
        // Insert nodes
        conn.execute(
            "INSERT INTO graph_nodes (id, case_id, node_type, label, created_at)
             VALUES ('kept', 'case-1', 'file', 'kept', '2026-01-01T00:00:00Z'),
                    ('merged', 'case-1', 'file', 'merged', '2026-01-01T00:00:00Z'),
                    ('target', 'case-1', 'file', 'target', '2026-01-01T00:00:00Z')",
            [],
        )
        .unwrap();
        // Insert edges
        conn.execute(
            "INSERT INTO graph_edges (id, case_id, source_id, target_id, edge_type, created_at)
             VALUES ('e-out', 'case-1', 'merged', 'target', 'references', '2026-01-01T00:00:00Z'),
                    ('e-in', 'case-1', 'target', 'merged', 'references', '2026-01-01T00:00:00Z')",
            [],
        )
        .unwrap();

        repoint_outgoing_edges(&conn, "kept", "merged", "case-1").unwrap();
        repoint_incoming_edges(&conn, "kept", "merged", "case-1").unwrap();
        delete_graph_node(&conn, "merged", "case-1").unwrap();

        // Outgoing edge should now point to kept
        let source: String = conn
            .query_row(
                "SELECT source_id FROM graph_edges WHERE id = 'e-out'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(source, "kept");

        // Incoming edge should now point to kept
        let target: String = conn
            .query_row(
                "SELECT target_id FROM graph_edges WHERE id = 'e-in'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(target, "kept");
    }

    #[test]
    fn merge_log_roundtrip() {
        let conn = setup_db();
        insert_merge_log(
            &conn,
            "merge-1",
            "case-1",
            "kept-entity",
            "merged-entity",
            0.95,
            "2026-01-01T00:00:00Z",
        )
        .unwrap();

        let (kept, merged, conf): (String, String, f64) = conn
            .query_row(
                "SELECT kept_entity_id, merged_entity_id, confidence FROM entity_merge_log WHERE merge_id = 'merge-1'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        assert_eq!(kept, "kept-entity");
        assert_eq!(merged, "merged-entity");
        assert!((conf - 0.95).abs() < f64::EPSILON);
    }

    #[test]
    fn resolved_entity_upsert() {
        let conn = setup_db();
        upsert_resolved_entity(
            &conn,
            "resolved-1",
            "case-1",
            "person",
            "alice@example.com",
            2,
            0.85,
            r#"["alice@example.com","Alice@Example.COM"]"#,
        )
        .unwrap();

        let (canonical, count, conf): (String, i64, f64) = conn
            .query_row(
                "SELECT canonical_value, source_count, confidence FROM resolved_entities WHERE id = 'resolved-1'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        assert_eq!(canonical, "alice@example.com");
        assert_eq!(count, 2);
        assert!((conf - 0.85).abs() < f64::EPSILON);
    }

    #[test]
    fn entity_relationship_upsert() {
        let conn = setup_db();
        upsert_entity_relationship(
            &conn,
            "rel-1",
            "case-1",
            "entity-alice",
            "entity-bob",
            "communicates_with",
            0.85,
            r#"["edge-1","edge-2"]"#,
            "2026-01-01T00:00:00Z",
        )
        .unwrap();

        let (source, target, rel_type): (String, String, String) = conn
            .query_row(
                "SELECT source_entity_id, target_entity_id, relationship_type
                 FROM entity_relationships WHERE id = 'rel-1'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        assert_eq!(source, "entity-alice");
        assert_eq!(target, "entity-bob");
        assert_eq!(rel_type, "communicates_with");
    }

    #[test]
    fn get_artifact_ids_for_case_returns_correct_ids() {
        let conn = setup_db();
        conn.execute(
            "INSERT INTO artifacts (id, case_id, title) VALUES ('a1', 'case-1', 'Artifact 1')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO artifacts (id, case_id, title) VALUES ('a2', 'case-2', 'Artifact 2')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO artifacts (id, case_id, title) VALUES ('a3', 'case-1', 'Artifact 3')",
            [],
        )
        .unwrap();

        let ids = get_artifact_ids_for_case(&conn, "case-1").unwrap();
        assert_eq!(ids.len(), 2);
        assert!(ids.contains(&"a1".to_string()));
        assert!(ids.contains(&"a3".to_string()));
    }

    #[test]
    fn get_artifact_rows_for_case_includes_attrs() {
        let conn = setup_db();
        conn.execute(
            "INSERT INTO artifacts (id, case_id, title, summary, attrs)
             VALUES ('a1', 'case-1', 'Title', 'Summary', '{\"hostname\":\"PC1\"}')",
            [],
        )
        .unwrap();

        let rows = get_artifact_rows_for_case(&conn, "case-1").unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].0, "a1");
        assert_eq!(rows[0].1, "Title");
        assert_eq!(rows[0].2, "Summary");
        assert!(rows[0].3.contains("PC1"));
    }
}
