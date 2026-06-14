//! Entity extraction from artifacts — v1.
//!
//! Scans artifact title / summary / attrs for email addresses, Windows SIDs,
//! and hostname / computer_name fields, creates Entity graph nodes, and wires
//! them to their source artifact nodes via DerivesFrom edges.
//!
//! Entities are deduplicated by (entity_value, entity_type) within a case so
//! that the same email appearing in multiple artifacts produces a single node
//! with multiple edges.

use chrono::Utc;
use domain::{EdgeType, GraphEdge, GraphNode, NodeType};
use persistence_sqlite::repositories::graph_repo::GraphRepo;
use regex::Regex;
use rusqlite::Connection;
use std::collections::HashMap;

/// Extract entities from all artifacts in the given case and persist them as
/// investigative-graph nodes connected to their source artifact nodes.
///
/// * `conn` — an open SQLite connection for the case database.
/// * `case_id` — the case whose artifacts should be scanned.
///
/// Returns the number of unique entities created (0 when no entities are found
/// or no artifacts exist for the case).
pub fn extract_entities_from_artifacts(conn: &Connection, case_id: &str) -> Result<u64, String> {
    // Regexes are compiled once; a malformed literal is a hard bug.
    let email_re = Regex::new(r"[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}").unwrap();
    let sid_re = Regex::new(r"S-1-5-21-\d+-\d+-\d+-\d+").unwrap();

    // ── Step 1: read every artifact row for this case ──
    let mut stmt = conn
        .prepare("SELECT id, title, summary, attrs FROM artifacts WHERE case_id = ?1")
        .map_err(|e| e.to_string())?;

    let rows: Vec<(String, String, String, String)> = stmt
        .query_map(rusqlite::params![case_id], |row| {
            Ok((
                row.get::<_, String>(0)?, // id
                row.get::<_, String>(1)?, // title
                row.get::<_, String>(2)?, // summary
                row.get::<_, String>(3)?, // attrs (JSON)
            ))
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;

    if rows.is_empty() {
        return Ok(0);
    }

    // ── Step 2: extract entity candidates ──
    // Key: (entity_value_lowercase, entity_type) → deduplicated artifact ids
    let mut entity_map: HashMap<(String, String), Vec<String>> = HashMap::new();

    for (artifact_id, title, summary, attrs_json) in &rows {
        // Concatenate text fields so a single regex pass catches everything.
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

    if entity_map.is_empty() {
        return Ok(0);
    }

    let now = Utc::now().to_rfc3339();

    // ── Step 3: remove previous extraction artefacts for this case ──
    // FK ON DELETE CASCADE on graph_edges means deleting entity nodes
    // automatically cleans up their edges.
    conn.execute(
        "DELETE FROM graph_nodes WHERE case_id = ?1 AND node_type = 'entity'",
        rusqlite::params![case_id],
    )
    .map_err(|e| e.to_string())?;

    // ── Step 3b: ensure artifact graph nodes exist ──
    // In production store_artifacts creates them; be resilient for calls that
    // happen before store_artifacts or when populate_artifact_graph was skipped.
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

        let missing: Vec<GraphNode> = rows
            .iter()
            .map(|(id, title, summary, _)| (id, title, summary))
            .filter(|(id, _, _)| !existing.contains(*id))
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

    // ── Step 4: build node & edge vectors ──
    let mut nodes: Vec<GraphNode> = Vec::new();
    let mut edges: Vec<GraphEdge> = Vec::new();

    // Sort entries for deterministic output.
    let mut entries: Vec<((String, String), Vec<String>)> = entity_map.into_iter().collect();
    entries.sort_by(|a, b| a.0 .0.cmp(&b.0 .0).then_with(|| a.0 .1.cmp(&b.0 .1)));

    for ((value, entity_type), artifact_ids) in &entries {
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

        for artifact_id in artifact_ids {
            edges.push(GraphEdge {
                id: format!("derives_from:{}:{}", node_id, artifact_id),
                case_id: case_id.to_string(),
                // Entity --DerivesFrom--> Artifact
                source_id: node_id.clone(),
                target_id: artifact_id.clone(),
                edge_type: EdgeType::DerivesFrom,
                confidence: None,
                provenance: Some("entity_extraction_v1".into()),
                created_at: now.clone(),
            });
        }
    }

    // ── Step 5: persist ──
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

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use domain::{Artifact, ArtifactId, DataSourceId};
    use persistence_sqlite::repositories::artifact_repo::ArtifactRepo;
    use std::collections::BTreeMap;

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
            // We don't know the exact node_id (UUID-based), but we can query by case
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
}
