//! Entity extraction from artifacts — v1.
//!
//! Scans artifact title / summary / attrs for email addresses, Windows SIDs,
//! and hostname / computer_name fields, creates Entity graph nodes, and wires
//! them to their source artifact nodes via DerivesFrom edges.
//!
//! Entities are deduplicated by (entity_value, entity_type) within a case so
//! that the same email appearing in multiple artifacts produces a single node
//! with multiple edges.
//!
//! ## Entity pre-normalization index
//!
//! The `entity_index` table stores normalized + hashed entity values for
//! persistent deduplication and fast lookup without re-scanning all artifacts.
//! Functions:
//! - `index_entities` — scan artifacts, normalize, upsert into `entity_index`.
//! - `lookup_entity` — fast index lookup returning source artifact IDs.
//! - `extract_entities_from_artifacts` — uses the index when already populated,
//!   falling back to a full regex scan otherwise.

use chrono::Utc;
use domain::{EdgeType, GraphEdge, GraphNode, NodeType};
use persistence_sqlite::repositories::graph_repo::GraphRepo;
use regex::Regex;
use rusqlite::Connection;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use unicode_normalization::UnicodeNormalization;

// ── Public API: normalization & hashing ───────────────────────────

/// Normalize an entity value for consistent deduplication.
///
/// Applies lowercase, trim, and NFKD Unicode normalization so that
/// semantically identical values (e.g. "Alice@Example.COM" and
/// "alice@example.com") produce the same lookup key.
pub fn normalize_entity_value(value: &str) -> String {
    value.trim().to_lowercase().nfkd().collect()
}

/// Hash a normalized entity value with SHA-256, returning the first
/// 16 hex characters (64 bits). This compact hash is used as the
/// primary key in `entity_index` for fast equality checks.
pub fn hash_entity_value(normalized: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(normalized.as_bytes());
    let result = hasher.finalize();
    hex::encode(&result[..8]) // first 8 bytes → 16 hex chars
}

// ── Public API: entity index operations ───────────────────────────

/// Scan all artifacts in `case_id`, extract entities via regex, normalize
/// values, and upsert into the `entity_index` table for persistent
/// deduplication and fast lookup.
///
/// Returns the number of unique entity-index rows created or updated.
pub fn index_entities(conn: &Connection, case_id: &str) -> Result<u64, String> {
    let entity_map = regex_scan_artifacts(conn, case_id)?;

    if entity_map.is_empty() {
        return Ok(0);
    }

    let now = Utc::now().to_rfc3339();
    let mut count = 0u64;

    for ((value, entity_type), artifact_ids) in &entity_map {
        let normalized = normalize_entity_value(value);
        let hash = hash_entity_value(&normalized);
        let new_ids_json = serde_json::to_string(artifact_ids).unwrap_or_default();

        // Check whether a row already exists for this (hash, entity_type).
        let existing_json: Option<String> = conn
            .query_row(
                "SELECT source_artifact_ids FROM entity_index
                 WHERE value_hash = ?1 AND entity_type = ?2",
                rusqlite::params![hash, entity_type],
                |row| row.get(0),
            )
            .ok();

        if let Some(existing_json) = existing_json {
            // Merge artifact IDs — preserve existing, add only new ones.
            let mut existing_ids: Vec<String> =
                serde_json::from_str(&existing_json).unwrap_or_default();
            let mut changed = false;
            for id in artifact_ids {
                if !existing_ids.contains(id) {
                    existing_ids.push(id.clone());
                    changed = true;
                }
            }
            if changed {
                let merged = serde_json::to_string(&existing_ids).unwrap_or_default();
                conn.execute(
                    "UPDATE entity_index
                     SET source_artifact_ids = ?1, updated_at = ?2
                     WHERE value_hash = ?3 AND entity_type = ?4",
                    rusqlite::params![merged, now, hash, entity_type],
                )
                .map_err(|e| e.to_string())?;
                count += 1;
            }
        } else {
            conn.execute(
                "INSERT INTO entity_index
                 (value_hash, entity_type, value_normalized, source_artifact_ids, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?5)",
                rusqlite::params![hash, entity_type, normalized, new_ids_json, now],
            )
            .map_err(|e| e.to_string())?;
            count += 1;
        }
    }

    Ok(count)
}

/// Look up an entity by its raw value and type, returning the list of
/// artifact IDs that reference it. The value is normalized and hashed
/// before the lookup so callers can pass in any casing / whitespace.
///
/// Returns `None` when the entity has not been indexed.
pub fn lookup_entity(conn: &Connection, value: &str, entity_type: &str) -> Option<Vec<String>> {
    let normalized = normalize_entity_value(value);
    let hash = hash_entity_value(&normalized);

    let ids_json: String = conn
        .query_row(
            "SELECT source_artifact_ids FROM entity_index
             WHERE value_hash = ?1 AND entity_type = ?2",
            rusqlite::params![hash, entity_type],
            |row| row.get(0),
        )
        .ok()?;

    serde_json::from_str(&ids_json).ok()
}

// ── Public API: graph extraction ──────────────────────────────────

/// Extract entities from all artifacts in the given case and persist them as
/// investigative-graph nodes connected to their source artifact nodes.
///
/// First tries the persistent `entity_index`; if it has no coverage for this
/// case, falls back to a full regex scan.  Call `index_entities` beforehand
/// to populate the index.
///
/// * `conn` — an open SQLite connection for the case database.
/// * `case_id` — the case whose artifacts should be scanned.
///
/// Returns the number of unique entities created (0 when no entities are found
/// or no artifacts exist for the case).
pub fn extract_entities_from_artifacts(conn: &Connection, case_id: &str) -> Result<u64, String> {
    // ── Try the persistent index first ──
    let artifact_ids: Vec<String> = get_artifact_ids_for_case(conn, case_id)?;
    if !artifact_ids.is_empty() {
        let indexed = build_entity_map_from_index(conn, &artifact_ids)?;
        if !indexed.is_empty() {
            return persist_entity_graph(conn, case_id, &artifact_ids, indexed);
        }
    }

    // ── Fall back: full regex scan ──
    let entity_map = regex_scan_artifacts(conn, case_id)?;
    if entity_map.is_empty() {
        return Ok(0);
    }

    persist_entity_graph(conn, case_id, &artifact_ids, entity_map)
}

// ── Internal helpers ──────────────────────────────────────────────

/// Run the regex-based entity scan over all artifacts for a case.
/// Returns a map of (value, entity_type) → list of source artifact IDs.
fn regex_scan_artifacts(
    conn: &Connection,
    case_id: &str,
) -> Result<HashMap<(String, String), Vec<String>>, String> {
    let email_re = Regex::new(r"[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}").unwrap();
    let sid_re = Regex::new(r"S-1-5-21-\d+-\d+-\d+-\d+").unwrap();

    let mut stmt = conn
        .prepare("SELECT id, title, summary, attrs FROM artifacts WHERE case_id = ?1")
        .map_err(|e| e.to_string())?;

    let rows: Vec<(String, String, String, String)> = stmt
        .query_map(rusqlite::params![case_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
            ))
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;

    let mut entity_map: HashMap<(String, String), Vec<String>> = HashMap::new();

    for (artifact_id, title, summary, attrs_json) in &rows {
        let combined = format!("{} {} {}", title, summary, attrs_json);

        // Email addresses → Person entities
        for cap in email_re.captures_iter(&combined) {
            let value = cap[0].to_lowercase();
            entity_map
                .entry((value, "person".into()))
                .or_default()
                .push(artifact_id.clone());
        }

        // SIDs → Account entities
        for cap in sid_re.captures_iter(&combined) {
            let value = cap[0].to_string();
            entity_map
                .entry((value, "account".into()))
                .or_default()
                .push(artifact_id.clone());
        }

        // Hostname / computer_name fields in attrs → Device entities
        let attrs: serde_json::Map<String, serde_json::Value> =
            serde_json::from_str(attrs_json).unwrap_or_default();
        for key in &[
            "hostname",
            "computer_name",
            "computerName",
            "machine_name",
            "machineName",
        ] {
            if let Some(serde_json::Value::String(val)) = attrs.get(*key) {
                let val = val.trim().to_lowercase();
                if !val.is_empty() {
                    entity_map
                        .entry((val, "device".into()))
                        .or_default()
                        .push(artifact_id.clone());
                }
            }
        }
    }

    Ok(entity_map)
}

/// Fetch all artifact IDs for a case.
fn get_artifact_ids_for_case(conn: &Connection, case_id: &str) -> Result<Vec<String>, String> {
    let mut stmt = conn
        .prepare("SELECT id FROM artifacts WHERE case_id = ?1")
        .map_err(|e| e.to_string())?;
    let ids: Vec<String> = stmt
        .query_map(rusqlite::params![case_id], |row| row.get::<_, String>(0))
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    Ok(ids)
}

/// Build an entity_map from the persistent index, filtering to only
/// the given artifact_ids. Reads every entity_index row and keeps those
/// whose `source_artifact_ids` JSON intersects the case's artifacts.
fn build_entity_map_from_index(
    conn: &Connection,
    artifact_ids: &[String],
) -> Result<HashMap<(String, String), Vec<String>>, String> {
    let artifact_set: std::collections::HashSet<&str> =
        artifact_ids.iter().map(|s| s.as_str()).collect();

    let mut stmt = conn
        .prepare("SELECT value_normalized, entity_type, source_artifact_ids FROM entity_index")
        .map_err(|e| e.to_string())?;

    let rows: Vec<(String, String, String)> = stmt
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;

    let mut entity_map: HashMap<(String, String), Vec<String>> = HashMap::new();

    for (normalized, entity_type, ids_json) in &rows {
        let ids: Vec<String> = serde_json::from_str(ids_json).unwrap_or_default();
        let matching: Vec<String> = ids
            .into_iter()
            .filter(|id| artifact_set.contains(id.as_str()))
            .collect();
        if !matching.is_empty() {
            entity_map
                .entry((normalized.clone(), entity_type.clone()))
                .or_default()
                .extend(matching);
        }
    }

    Ok(entity_map)
}

/// Persist entity graph nodes and edges from an entity map, handling
/// cleanup of previous entities and creation of missing artifact nodes.
fn persist_entity_graph(
    conn: &Connection,
    case_id: &str,
    _artifact_ids: &[String],
    entity_map: HashMap<(String, String), Vec<String>>,
) -> Result<u64, String> {
    let now = Utc::now().to_rfc3339();

    // ── Remove previous extraction artefacts for this case ──
    conn.execute(
        "DELETE FROM graph_nodes WHERE case_id = ?1 AND node_type = 'entity'",
        rusqlite::params![case_id],
    )
    .map_err(|e| e.to_string())?;

    // ── Ensure artifact graph nodes exist ──
    {
        let existing: std::collections::HashSet<String> = {
            let mut stmt = conn
                .prepare("SELECT id FROM graph_nodes WHERE case_id = ?1 AND node_type = 'artifact'")
                .map_err(|e| e.to_string())?;
            let ids: Vec<String> = stmt
                .query_map(rusqlite::params![case_id], |row| row.get::<_, String>(0))
                .map_err(|e| e.to_string())?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| e.to_string())?;
            ids.into_iter().collect()
        };

        // Fetch title + summary for missing artifact nodes.
        let mut stmt = conn
            .prepare("SELECT id, title, summary FROM artifacts WHERE case_id = ?1")
            .map_err(|e| e.to_string())?;
        let rows: Vec<(String, String, String)> = stmt
            .query_map(rusqlite::params![case_id], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?))
            })
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;

        let missing: Vec<GraphNode> = rows
            .iter()
            .filter(|(id, _, _)| !existing.contains(id.as_str()))
            .map(|(id, title, summary)| GraphNode {
                id: id.clone(),
                case_id: case_id.to_string(),
                node_type: NodeType::Artifact,
                label: title.clone(),
                summary: summary.clone(),
                tags: Vec::new(),
                created_at: now.clone(),
            })
            .collect();

        if !missing.is_empty() {
            let graph_repo = GraphRepo::new(conn);
            graph_repo
                .insert_nodes_batch(&missing)
                .map_err(|e| format!("graph node insert (artifact): {e}"))?;
        }
    }

    // ── Build node & edge vectors ──
    let mut nodes: Vec<GraphNode> = Vec::new();
    let mut edges: Vec<GraphEdge> = Vec::new();

    let mut entries: Vec<((String, String), Vec<String>)> = entity_map.into_iter().collect();
    entries.sort_by(|a, b| a.0 .0.cmp(&b.0 .0).then_with(|| a.0 .1.cmp(&b.0 .1)));

    for ((value, entity_type), source_artifact_ids) in &entries {
        let node_id = format!("entity:{}:{}", case_id, uuid::Uuid::new_v4().as_simple());

        let (type_tag, summary) = match entity_type.as_str() {
            "person" => ("person", "Email address"),
            "account" => ("account", "Windows security identifier (SID)"),
            "device" => ("device", "Hostname / computer name"),
            _ => ("entity", "Extracted entity"),
        };

        nodes.push(GraphNode {
            id: node_id.clone(),
            case_id: case_id.to_string(),
            node_type: NodeType::Entity,
            label: value.clone(),
            summary: summary.to_string(),
            tags: vec!["entity".into(), type_tag.into()],
            created_at: now.clone(),
        });

        for artifact_id in source_artifact_ids {
            edges.push(GraphEdge {
                id: format!("derives_from:{}:{}", node_id, artifact_id),
                case_id: case_id.to_string(),
                source_id: node_id.clone(),
                target_id: artifact_id.clone(),
                edge_type: EdgeType::DerivesFrom,
                confidence: None,
                provenance: Some("entity_extraction_v1".into()),
                created_at: now.clone(),
            });
        }
    }

    let graph_repo = GraphRepo::new(conn);
    let total = nodes.len() as u64;

    if !nodes.is_empty() {
        graph_repo
            .insert_nodes_batch(&nodes)
            .map_err(|e| format!("graph node insert: {e}"))?;
    }
    if !edges.is_empty() {
        graph_repo
            .insert_edges_batch(&edges)
            .map_err(|e| format!("graph edge insert: {e}"))?;
    }

    Ok(total)
}

// ── Tests ─────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use domain::{Artifact, ArtifactId, DataSourceId};
    use persistence_sqlite::repositories::artifact_repo::ArtifactRepo;
    use std::collections::BTreeMap;
    use std::time::Instant;

    fn setup_db() -> rusqlite::Connection {
        let conn = persistence_sqlite::connection::open_in_memory().unwrap();
        persistence_sqlite::runner::run_all(&conn).unwrap();

        // Insert a case so FK constraints are satisfied.
        let case = domain::CaseMeta {
            id: domain::CaseId("case-1".to_string()),
            name: "test-case".to_string(),
            number: None,
            examiner: None,
            notes: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        persistence_sqlite::repositories::case_repo::CaseRepo::new(&conn)
            .create(&case)
            .unwrap();

        // Insert a data source so FK constraints are satisfied.
        let ds = domain::DataSource {
            id: DataSourceId("ds-1".to_string()),
            name: "test-ds".to_string(),
            kind: domain::DataSourceKind::LogicalDirectory,
            source_path: std::path::PathBuf::from("C:/test"),
            imported_at: Utc::now(),
            provenance: domain::DataSourceProvenance::unknown(),
        };
        persistence_sqlite::repositories::datasource_repo::DataSourceRepo::new(&conn)
            .insert(&domain::CaseId("case-1".to_string()), &ds)
            .unwrap();

        conn
    }

    fn make_artifact(
        id: &str,
        title: &str,
        summary: &str,
        attrs: BTreeMap<String, serde_json::Value>,
    ) -> Artifact {
        Artifact {
            id: ArtifactId(id.to_string()),
            family: "Test".to_string(),
            title: title.to_string(),
            summary: summary.to_string(),
            source_object_id: None,
            extractor_id: None,
            extractor_version: None,
            confidence: None,
            source_attribution: None,
            created_at: Utc::now(),
            attrs,
        }
    }

    fn insert_artifact(conn: &rusqlite::Connection, artifact: &Artifact) {
        ArtifactRepo::new(conn)
            .insert_batch(std::slice::from_ref(artifact), "case-1", "ds-1")
            .unwrap();
    }

    // ── Existing tests ────────────────────────────────────────────

    #[test]
    fn extracts_email_as_person_entity() {
        let conn = setup_db();
        insert_artifact(
            &conn,
            &make_artifact(
                "a1",
                "Email from alice@example.com",
                "Contact: bob@test.org",
                BTreeMap::new(),
            ),
        );

        let count = extract_entities_from_artifacts(&conn, "case-1").unwrap();
        assert_eq!(count, 2);

        // Verify graph nodes exist
        let graph_repo = GraphRepo::new(&conn);
        let snapshot = graph_repo.get_snapshot("case-1").unwrap();
        // 1 artifact node (created in step 3b) + 2 entity nodes = 3
        assert_eq!(snapshot.total_nodes, 3);
        assert_eq!(snapshot.total_edges, 2);

        // Verify we can retrieve each entity
        let emails = ["alice@example.com", "bob@test.org"];
        for email in &emails {
            let mut stmt = conn
                .prepare("SELECT id FROM graph_nodes WHERE case_id = ?1 AND label = ?2")
                .unwrap();
            let node_id: String = stmt
                .query_row(rusqlite::params!["case-1", email], |row| row.get(0))
                .unwrap();
            // Verify there's a DerivesFrom edge from this entity to the artifact
            let mut stmt = conn
                .prepare("SELECT COUNT(*) FROM graph_edges WHERE source_id = ?1 AND target_id = 'a1' AND edge_type = 'derives_from'")
                .unwrap();
            let edge_count: i64 = stmt
                .query_row(rusqlite::params![node_id], |row| row.get(0))
                .unwrap();
            assert_eq!(edge_count, 1);
        }
    }

    #[test]
    fn extracts_sid_as_account_entity() {
        let conn = setup_db();
        insert_artifact(
            &conn,
            &make_artifact(
                "a-sid",
                "User SID found",
                "The SID is S-1-5-21-3623811015-3361044348-30300820-1013",
                BTreeMap::new(),
            ),
        );

        let count = extract_entities_from_artifacts(&conn, "case-1").unwrap();
        assert_eq!(count, 1);

        let mut stmt = conn
            .prepare("SELECT label, summary FROM graph_nodes WHERE case_id = ?1 AND node_type = 'entity'")
            .unwrap();
        let rows: Vec<(String, String)> = stmt
            .query_map(rusqlite::params!["case-1"], |row| {
                Ok((row.get(0)?, row.get(1)?))
            })
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].0, "S-1-5-21-3623811015-3361044348-30300820-1013");
        assert!(rows[0].1.contains("SID"));
    }

    #[test]
    fn extracts_hostname_as_device_entity() {
        let conn = setup_db();
        let mut attrs = BTreeMap::new();
        attrs.insert(
            "hostname".to_string(),
            serde_json::Value::String("WIN10-WORKSTATION".to_string()),
        );
        insert_artifact(
            &conn,
            &make_artifact("a-host", "System Info", "Some system", attrs),
        );

        let count = extract_entities_from_artifacts(&conn, "case-1").unwrap();
        assert_eq!(count, 1);

        let mut stmt = conn
            .prepare("SELECT label FROM graph_nodes WHERE case_id = ?1 AND node_type = 'entity'")
            .unwrap();
        let label: String = stmt
            .query_row(rusqlite::params!["case-1"], |row| row.get(0))
            .unwrap();
        assert_eq!(label, "win10-workstation"); // lowercased
    }

    #[test]
    fn deduplicates_same_email_across_artifacts() {
        let conn = setup_db();
        insert_artifact(
            &conn,
            &make_artifact("a1", "Email from alice@example.com", "", BTreeMap::new()),
        );
        insert_artifact(
            &conn,
            &make_artifact("a2", "Also alice@example.com here", "", BTreeMap::new()),
        );

        let count = extract_entities_from_artifacts(&conn, "case-1").unwrap();
        assert_eq!(count, 1); // deduplicated

        // Should have 2 edges (one from the entity to each artifact)
        let edge_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM graph_edges WHERE case_id = 'case-1' AND edge_type = 'derives_from'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(edge_count, 2);
    }

    #[test]
    fn empty_case_returns_zero() {
        let conn = setup_db();
        let count = extract_entities_from_artifacts(&conn, "case-1").unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn no_matching_patterns_returns_zero() {
        let conn = setup_db();
        insert_artifact(
            &conn,
            &make_artifact(
                "a-plain",
                "Just some text",
                "Nothing to extract here",
                BTreeMap::new(),
            ),
        );

        let count = extract_entities_from_artifacts(&conn, "case-1").unwrap();
        assert_eq!(count, 0);
    }

    // ── Normalization / hashing unit tests ────────────────────────

    #[test]
    fn normalize_lowercase_and_trim() {
        assert_eq!(
            normalize_entity_value("  Alice@Example.COM  "),
            "alice@example.com"
        );
    }

    #[test]
    fn normalize_nfkd_decomposes() {
        // U+00E9 (é) NFKD-decomposes to e + combining acute accent
        let input = "caf\u{00E9}"; // café
        let normalized = normalize_entity_value(input);
        // After NFKD the 'é' becomes 'e' + combining acute → "cafe\u{0301}"
        assert!(normalized.starts_with("cafe"));
        assert!(normalized.len() > 4);
    }

    #[test]
    fn hash_is_stable() {
        let h1 = hash_entity_value("alice@example.com");
        let h2 = hash_entity_value("alice@example.com");
        assert_eq!(h1, h2);
        assert_eq!(h1.len(), 16);
    }

    #[test]
    fn hash_differs_for_different_inputs() {
        let h1 = hash_entity_value("alice@example.com");
        let h2 = hash_entity_value("bob@example.com");
        assert_ne!(h1, h2);
    }

    // ── Entity index tests ─────────────────────────────────────────

    #[test]
    fn index_and_lookup_entity() {
        let conn = setup_db();
        insert_artifact(
            &conn,
            &make_artifact("a1", "Email from alice@example.com", "", BTreeMap::new()),
        );

        let indexed = index_entities(&conn, "case-1").unwrap();
        assert_eq!(indexed, 1);

        // Lookup with different casing / whitespace should still hit.
        let artifact_ids = lookup_entity(&conn, "  Alice@Example.COM  ", "person");
        assert!(artifact_ids.is_some());
        assert_eq!(artifact_ids.unwrap(), vec!["a1"]);

        // Lookup for non-existent entity returns None.
        assert!(lookup_entity(&conn, "nobody@nowhere.com", "person").is_none());
    }

    #[test]
    fn normalized_lookup_matches_regex_scan() {
        let conn = setup_db();
        insert_artifact(
            &conn,
            &make_artifact(
                "a-mixed",
                "Contact: Alice@Example.COM and Bob@Test.ORG",
                "Also re: cecil@host.org",
                BTreeMap::new(),
            ),
        );

        // Run the full regex scan graph extraction.
        let regex_count = extract_entities_from_artifacts(&conn, "case-1").unwrap();
        assert_eq!(regex_count, 3);

        // Now index the same data.
        let _ = index_entities(&conn, "case-1").unwrap();

        // lookup_entity with mixed-case / whitespace should match all three.
        let alice = lookup_entity(&conn, "  Alice@Example.COM  ", "person").unwrap();
        assert_eq!(alice, vec!["a-mixed"]);

        let bob = lookup_entity(&conn, "bob@test.org", "person").unwrap();
        assert_eq!(bob, vec!["a-mixed"]);

        let cecil = lookup_entity(&conn, "Cecil@Host.ORG", "person").unwrap();
        assert_eq!(cecil, vec!["a-mixed"]);
    }

    #[test]
    fn index_deduplicates_across_artifacts() {
        let conn = setup_db();
        insert_artifact(
            &conn,
            &make_artifact("a1", "Email alice@example.com", "", BTreeMap::new()),
        );
        insert_artifact(
            &conn,
            &make_artifact("a2", "Also alice@example.com here", "", BTreeMap::new()),
        );
        insert_artifact(
            &conn,
            &make_artifact("a3", "bob@example.com contact", "", BTreeMap::new()),
        );

        let indexed = index_entities(&conn, "case-1").unwrap();
        // Two unique entities: alice@example.com and bob@example.com
        assert_eq!(indexed, 2);

        // alice@example.com should reference both a1 and a2
        let alice_ids = lookup_entity(&conn, "alice@example.com", "person").unwrap();
        // Sort for deterministic comparison
        let mut sorted = alice_ids.clone();
        sorted.sort();
        assert_eq!(sorted, vec!["a1", "a2"]);

        // bob@example.com should reference only a3
        let bob_ids = lookup_entity(&conn, "bob@example.com", "person").unwrap();
        assert_eq!(bob_ids, vec!["a3"]);
    }

    #[test]
    fn index_preserves_existing_on_reindex() {
        let conn = setup_db();
        insert_artifact(
            &conn,
            &make_artifact("a1", "alice@example.com", "", BTreeMap::new()),
        );

        // First index.
        let count1 = index_entities(&conn, "case-1").unwrap();
        assert_eq!(count1, 1);

        // Add another artifact and re-index.
        insert_artifact(
            &conn,
            &make_artifact("a2", "alice@example.com again", "", BTreeMap::new()),
        );
        let count2 = index_entities(&conn, "case-1").unwrap();
        // The existing row should have been updated (merged), so count2 is >= 1.
        // It counts rows where we changed something — the update merges a2 so it's 1.
        assert_eq!(count2, 1);

        let ids = lookup_entity(&conn, "alice@example.com", "person").unwrap();
        let mut sorted = ids.clone();
        sorted.sort();
        assert_eq!(sorted, vec!["a1", "a2"]);
    }

    #[test]
    fn lookup_under_10ms() {
        let conn = setup_db();
        insert_artifact(
            &conn,
            &make_artifact("a1", "alice@example.com", "", BTreeMap::new()),
        );

        index_entities(&conn, "case-1").unwrap();

        // Warm up
        lookup_entity(&conn, "alice@example.com", "person");

        let start = Instant::now();
        for _ in 0..100 {
            let result = lookup_entity(&conn, "alice@example.com", "person");
            assert!(result.is_some());
        }
        let elapsed = start.elapsed();
        let per_lookup = elapsed / 100;

        // Each lookup should be well under 10ms.
        assert!(
            per_lookup.as_millis() < 10,
            "lookup took {:?} (expected < 10ms)",
            per_lookup
        );
    }

    // ── Index-backed extraction tests ─────────────────────────────

    #[test]
    fn extract_uses_index_when_available() {
        let conn = setup_db();
        insert_artifact(
            &conn,
            &make_artifact("a1", "Email from alice@example.com", "", BTreeMap::new()),
        );

        // Populate the index first.
        index_entities(&conn, "case-1").unwrap();

        // Now extract — it should use the index path.
        let count = extract_entities_from_artifacts(&conn, "case-1").unwrap();
        assert_eq!(count, 1);

        // Verify graph was built correctly.
        let graph_repo = GraphRepo::new(&conn);
        let snapshot = graph_repo.get_snapshot("case-1").unwrap();
        assert_eq!(snapshot.total_nodes, 2); // 1 artifact + 1 entity
        assert_eq!(snapshot.total_edges, 1);
    }

    #[test]
    fn extract_falls_back_when_index_empty() {
        let conn = setup_db();
        insert_artifact(
            &conn,
            &make_artifact("a1", "alice@example.com", "", BTreeMap::new()),
        );

        // entity_index is empty — should fall back to regex scan.
        let count = extract_entities_from_artifacts(&conn, "case-1").unwrap();
        assert_eq!(count, 1);

        let graph_repo = GraphRepo::new(&conn);
        let snapshot = graph_repo.get_snapshot("case-1").unwrap();
        assert!(snapshot.total_nodes >= 2);
    }
}
