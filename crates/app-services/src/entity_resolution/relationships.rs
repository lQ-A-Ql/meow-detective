//! Entity relationship discovery — infers semantic relationships between
//! resolved entities based on graph edge patterns.
//!
//! Phase 2 of entity resolution: after canonicalization and merge, this
//! engine walks the investigative graph to discover how entities relate
//! to one another (CommunicatesWith, Owns, LoggedInto, Executed, etc.).

use super::EntityResolutionError;
use persistence_sqlite::repositories::entity_repo;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};

/// The type of relationship between two resolved entities.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum RelationshipType {
    CommunicatesWith,
    Owns,
    LoggedInto,
    Executed,
    Downloaded,
    Accessed,
}

impl RelationshipType {
    fn as_db_str(&self) -> &'static str {
        match self {
            RelationshipType::CommunicatesWith => "communicates_with",
            RelationshipType::Owns => "owns",
            RelationshipType::LoggedInto => "logged_into",
            RelationshipType::Executed => "executed",
            RelationshipType::Downloaded => "downloaded",
            RelationshipType::Accessed => "accessed",
        }
    }
}

/// A discovered relationship between two entities, inferred from graph edges.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntityRelationship {
    pub id: String,
    pub case_id: String,
    pub source_entity_id: String,
    pub target_entity_id: String,
    pub relationship_type: RelationshipType,
    pub confidence: f64,
    pub evidence_edge_ids: Vec<String>,
    pub created_at: String,
}

/// Engine that infers entity relationships by traversing graph edge patterns
/// and persists them into the `entity_relationships` table.
pub struct EntityRelationshipEngine;

impl EntityRelationshipEngine {
    // ── Public API ──────────────────────────────────────────────────

    /// Walk the investigative graph for a case and infer all entity
    /// relationships from edge patterns.
    ///
    /// # Errors
    ///
    /// Returns a string error if a database query fails.
    pub fn infer_relationships(
        conn: &Connection,
        case_id: &str,
    ) -> Result<Vec<EntityRelationship>, EntityResolutionError> {
        let mut relationships: Vec<EntityRelationship> = Vec::new();

        relationships.extend(Self::infer_communicates_with(conn, case_id)?);
        relationships.extend(Self::infer_ownership(conn, case_id)?);
        relationships.extend(Self::infer_logged_into(conn, case_id)?);
        relationships.extend(Self::infer_executed(conn, case_id)?);

        // Deduplicate: merge evidence for the same (source, target, type) pair.
        relationships = Self::deduplicate(relationships);

        Ok(relationships)
    }

    /// Persist inferred relationships into `entity_relationships`.
    ///
    /// Uses `INSERT OR REPLACE` so that re-running inference for the
    /// same case is idempotent.
    ///
    /// # Errors
    ///
    /// Returns a string error if the insert batch fails.
    pub fn persist_relationships(
        conn: &Connection,
        case_id: &str,
        relationships: &[EntityRelationship],
    ) -> Result<u64, EntityResolutionError> {
        if relationships.is_empty() {
            return Ok(0);
        }

        let tx = conn.unchecked_transaction().map_err(|e| {
            EntityResolutionError::Other(format!("failed to begin transaction: {e}"))
        })?;

        let mut count = 0u64;
        for r in relationships {
            if r.case_id != case_id {
                continue;
            }
            let edge_json = serde_json::to_string(&r.evidence_edge_ids).unwrap_or_default();
            entity_repo::upsert_entity_relationship(
                &tx,
                &r.id,
                &r.case_id,
                &r.source_entity_id,
                &r.target_entity_id,
                r.relationship_type.as_db_str(),
                r.confidence,
                &edge_json,
                &r.created_at,
            )
            .map_err(|e| {
                EntityResolutionError::Other(format!("failed to insert relationship {}: {e}", r.id))
            })?;
            count += 1;
        }

        tx.commit().map_err(|e| {
            EntityResolutionError::Other(format!("failed to commit relationships: {e}"))
        })?;

        Ok(count)
    }

    // ── Pattern inference methods ──────────────────────────────────

    /// CommunicatesWith: two Person entities connected through shared
    /// EmailMessage artifacts.
    ///
    /// Pattern: Entity(Person) --[correlates_with]--> EmailMessage
    ///          <--[correlates_with]-- Entity(Person)
    fn infer_communicates_with(
        conn: &Connection,
        case_id: &str,
    ) -> Result<Vec<EntityRelationship>, EntityResolutionError> {
        let mut stmt = conn.prepare(
            "SELECT e1.id, e2.id, GROUP_CONCAT(DISTINCT ge1.id), GROUP_CONCAT(DISTINCT ge2.id)
             FROM graph_nodes e1
             JOIN graph_edges ge1 ON ge1.source_id = e1.id
                 AND ge1.edge_type = 'correlates_with'
             JOIN graph_nodes art ON art.id = ge1.target_id
                 AND art.node_type = 'artifact'
             JOIN graph_edges ge2 ON ge2.target_id = art.id
                 AND ge2.edge_type = 'correlates_with'
                 AND ge2.source_id != e1.id
             JOIN graph_nodes e2 ON e2.id = ge2.source_id
                 AND e2.node_type = 'entity'
             WHERE e1.case_id = ?1
               AND e1.node_type = 'entity'
               AND e1.tags LIKE '%\"person\"%'
               AND e2.tags LIKE '%\"person\"%'
               AND e1.id < e2.id
               AND (art.tags LIKE '%\"EmailMessage\"%'
                    OR LOWER(art.label) LIKE '%email%')
             GROUP BY e1.id, e2.id",
        )?;

        let rows: Vec<(String, String, String, String)> = stmt
            .query_map(rusqlite::params![case_id], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;

        Self::build_relationships(case_id, rows, RelationshipType::CommunicatesWith)
    }

    /// Owns: Person entity derived from Registry artifacts that reference
    /// Files belonging to a Device entity.
    ///
    /// Pattern: Entity(Person) --[derives_from]--> Registry --[references]-->
    ///          File <--[???]-- Entity(Device)
    fn infer_ownership(
        conn: &Connection,
        case_id: &str,
    ) -> Result<Vec<EntityRelationship>, EntityResolutionError> {
        let mut stmt = conn.prepare(
            "SELECT e1.id, e2.id,
                        GROUP_CONCAT(DISTINCT ge1.id),
                        GROUP_CONCAT(DISTINCT ge2.id || ',' || ge3.id)
                 FROM graph_nodes e1
                 JOIN graph_edges ge1 ON ge1.source_id = e1.id
                     AND ge1.edge_type = 'derives_from'
                 JOIN graph_nodes reg ON reg.id = ge1.target_id
                     AND reg.node_type = 'artifact'
                 JOIN graph_edges ge2 ON ge2.source_id = reg.id
                     AND ge2.edge_type = 'references'
                 JOIN graph_nodes f ON f.id = ge2.target_id
                     AND f.node_type = 'file'
                 JOIN graph_edges ge3 ON (
                     (ge3.source_id = f.id AND ge3.target_id = e2.id)
                     OR
                     (ge3.target_id = f.id AND ge3.source_id = e2.id)
                 )
                 JOIN graph_nodes e2 ON (
                     e2.id = CASE WHEN ge3.source_id = f.id
                                  THEN ge3.target_id ELSE ge3.source_id END
                 )
                 WHERE e1.case_id = ?1
                   AND e1.node_type = 'entity'
                   AND e1.tags LIKE '%\"person\"%'
                   AND e2.node_type = 'entity'
                   AND e2.tags LIKE '%\"device\"%'
                   AND (reg.tags LIKE '%\"Registry\"%'
                        OR LOWER(reg.label) LIKE '%registry%')
                   AND e1.id != e2.id
                 GROUP BY e1.id, e2.id",
        )?;

        let rows: Vec<(String, String, String, String)> = stmt
            .query_map(rusqlite::params![case_id], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;

        Self::build_relationships(case_id, rows, RelationshipType::Owns)
    }

    /// LoggedInto: Person entity derived from wtmp artifacts that reference
    /// Files belonging to a Device entity.
    ///
    /// Pattern: Entity(Person) --[derives_from]--> wtmp --[references]-->
    ///          File <--[???]-- Entity(Device)
    fn infer_logged_into(
        conn: &Connection,
        case_id: &str,
    ) -> Result<Vec<EntityRelationship>, EntityResolutionError> {
        let mut stmt = conn.prepare(
            "SELECT e1.id, e2.id,
                        GROUP_CONCAT(DISTINCT ge1.id),
                        GROUP_CONCAT(DISTINCT ge2.id || ',' || ge3.id)
                 FROM graph_nodes e1
                 JOIN graph_edges ge1 ON ge1.source_id = e1.id
                     AND ge1.edge_type = 'derives_from'
                 JOIN graph_nodes art ON art.id = ge1.target_id
                     AND art.node_type = 'artifact'
                 JOIN graph_edges ge2 ON ge2.source_id = art.id
                     AND ge2.edge_type = 'references'
                 JOIN graph_nodes f ON f.id = ge2.target_id
                     AND f.node_type = 'file'
                 JOIN graph_edges ge3 ON (
                     (ge3.source_id = f.id AND ge3.target_id = e2.id)
                     OR
                     (ge3.target_id = f.id AND ge3.source_id = e2.id)
                 )
                 JOIN graph_nodes e2 ON (
                     e2.id = CASE WHEN ge3.source_id = f.id
                                  THEN ge3.target_id ELSE ge3.source_id END
                 )
                 WHERE e1.case_id = ?1
                   AND e1.node_type = 'entity'
                   AND e1.tags LIKE '%\"person\"%'
                   AND e2.node_type = 'entity'
                   AND e2.tags LIKE '%\"device\"%'
                   AND (art.tags LIKE '%\"wtmp\"%'
                        OR LOWER(art.label) LIKE '%wtmp%')
                   AND e1.id != e2.id
                 GROUP BY e1.id, e2.id",
        )?;

        let rows: Vec<(String, String, String, String)> = stmt
            .query_map(rusqlite::params![case_id], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;

        Self::build_relationships(case_id, rows, RelationshipType::LoggedInto)
    }

    /// Executed: Person entity derived from Prefetch artifacts that reference
    /// executable Files.
    ///
    /// Pattern: Entity(Person) --[derives_from]--> Prefetch
    ///          --[references]--> File (executable)
    fn infer_executed(
        conn: &Connection,
        case_id: &str,
    ) -> Result<Vec<EntityRelationship>, EntityResolutionError> {
        // Executed is a special case: it relates a Person entity to an
        // executable File, not another entity. We infer it when a Person
        // entity derives from a Prefetch artifact that references a file.
        // The relationship target is the file node — the caller/UI can
        // associate that file with a Device entity if needed.
        let mut stmt = conn.prepare(
            "SELECT e1.id, f.id,
                        GROUP_CONCAT(DISTINCT ge1.id),
                        GROUP_CONCAT(DISTINCT ge2.id)
                 FROM graph_nodes e1
                 JOIN graph_edges ge1 ON ge1.source_id = e1.id
                     AND ge1.edge_type = 'derives_from'
                 JOIN graph_nodes art ON art.id = ge1.target_id
                     AND art.node_type = 'artifact'
                 JOIN graph_edges ge2 ON ge2.source_id = art.id
                     AND ge2.edge_type = 'references'
                 JOIN graph_nodes f ON f.id = ge2.target_id
                     AND f.node_type = 'file'
                 WHERE e1.case_id = ?1
                   AND e1.node_type = 'entity'
                   AND e1.tags LIKE '%\"person\"%'
                   AND (art.tags LIKE '%\"Prefetch\"%'
                        OR LOWER(art.label) LIKE '%prefetch%')
                 GROUP BY e1.id, f.id",
        )?;

        let rows: Vec<(String, String, String, String)> = stmt
            .query_map(rusqlite::params![case_id], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;

        Self::build_relationships(case_id, rows, RelationshipType::Executed)
    }

    // ── Helpers ────────────────────────────────────────────────────

    /// Build `EntityRelationship` instances from raw row data.
    /// Each row contains (source_id, target_id, edges_concat_a, edges_concat_b).
    fn build_relationships(
        case_id: &str,
        rows: Vec<(String, String, String, String)>,
        rel_type: RelationshipType,
    ) -> Result<Vec<EntityRelationship>, EntityResolutionError> {
        let now = chrono::Utc::now().to_rfc3339();
        let mut results = Vec::with_capacity(rows.len());

        for (source_id, target_id, edges_a, edges_b) in &rows {
            let mut edge_ids: Vec<String> = Vec::new();
            edge_ids.extend(Self::parse_edge_ids(edges_a));
            edge_ids.extend(Self::parse_edge_ids(edges_b));

            // Deduplicate edge IDs
            let edge_set: HashSet<String> = edge_ids.into_iter().collect();
            let unique_edges: Vec<String> = edge_set.into_iter().collect();

            let confidence = Self::compute_confidence(unique_edges.len());
            let id = Self::relationship_id(case_id, source_id, target_id, &rel_type);

            results.push(EntityRelationship {
                id,
                case_id: case_id.to_string(),
                source_entity_id: source_id.clone(),
                target_entity_id: target_id.clone(),
                relationship_type: rel_type.clone(),
                confidence,
                evidence_edge_ids: unique_edges,
                created_at: now.clone(),
            });
        }

        Ok(results)
    }

    /// Parse a comma-separated edge ID string into a `Vec<String>`.
    fn parse_edge_ids(concat: &str) -> Vec<String> {
        if concat.is_empty() {
            vec![]
        } else {
            concat
                .split(',')
                .filter(|s| !s.is_empty())
                .map(|s| s.to_string())
                .collect()
        }
    }

    /// Compute confidence based on the number of distinct evidence edges.
    ///
    /// Tiers:
    ///   - 1 edge (direct evidence)        → 0.70
    ///   - 2 edges (corroborated)           → 0.85
    ///   - 3+ edges (multiple sources)      → 0.95
    fn compute_confidence(edge_count: usize) -> f64 {
        match edge_count {
            0 => 0.0,
            1 => 0.70,
            2 => 0.85,
            _ => 0.95,
        }
    }

    /// Build a deterministic relationship ID.
    fn relationship_id(
        case_id: &str,
        source_id: &str,
        target_id: &str,
        rel_type: &RelationshipType,
    ) -> String {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        case_id.hash(&mut hasher);
        source_id.hash(&mut hasher);
        target_id.hash(&mut hasher);
        rel_type.as_db_str().hash(&mut hasher);
        format!("rel:{}:{:016x}", case_id, hasher.finish())
    }

    /// Merge duplicate relationships: for the same (case_id, source, target,
    /// type) key, combine evidence edges and recompute confidence.
    #[allow(clippy::type_complexity)]
    fn deduplicate(relationships: Vec<EntityRelationship>) -> Vec<EntityRelationship> {
        type Key = (String, String, String, RelationshipType);
        type Val = (Vec<String>, String);
        let mut groups: HashMap<Key, Val> = HashMap::new();

        for r in relationships {
            let key = (
                r.case_id.clone(),
                r.source_entity_id.clone(),
                r.target_entity_id.clone(),
                r.relationship_type.clone(),
            );
            let entry = groups
                .entry(key)
                .or_insert_with(|| (Vec::new(), r.created_at.clone()));
            entry.0.extend(r.evidence_edge_ids);
        }

        groups
            .into_iter()
            .map(
                |((case_id, source_id, target_id, rel_type), (edge_ids_vec, created_at))| {
                    let edge_set: HashSet<String> = edge_ids_vec.into_iter().collect();
                    let unique_edges: Vec<String> = edge_set.into_iter().collect();
                    let confidence = Self::compute_confidence(unique_edges.len());
                    let id = Self::relationship_id(&case_id, &source_id, &target_id, &rel_type);
                    EntityRelationship {
                        id,
                        case_id,
                        source_entity_id: source_id,
                        target_entity_id: target_id,
                        relationship_type: rel_type,
                        confidence,
                        evidence_edge_ids: unique_edges,
                        created_at,
                    }
                },
            )
            .collect()
    }
}

// ── Tests ─────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use domain::{CaseId, CaseMeta, GraphNode, NodeType};
    use persistence_sqlite::connection::open_in_memory;
    use persistence_sqlite::repositories::{case_repo::CaseRepo, graph_repo::GraphRepo};
    use persistence_sqlite::runner;
    use rusqlite::Connection;

    fn setup_db() -> Connection {
        let conn = open_in_memory().unwrap();
        runner::run_all(&conn).unwrap();

        let case = CaseMeta {
            id: CaseId("case-1".to_string()),
            name: "test".to_string(),
            number: None,
            examiner: None,
            notes: None,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };
        CaseRepo::new(&conn).create(&case).unwrap();
        conn
    }

    fn insert_entity(conn: &Connection, case_id: &str, id: &str, label: &str, tags: Vec<&str>) {
        let node = GraphNode {
            id: id.to_string(),
            case_id: case_id.to_string(),
            node_type: NodeType::Entity,
            label: label.to_string(),
            summary: String::new(),
            tags: tags.into_iter().map(|s| s.to_string()).collect(),
            created_at: chrono::Utc::now().to_rfc3339(),
        };
        GraphRepo::new(conn).insert_nodes_batch(&[node]).unwrap();
    }

    fn insert_file_node(conn: &Connection, case_id: &str, id: &str, label: &str) {
        let node = GraphNode {
            id: id.to_string(),
            case_id: case_id.to_string(),
            node_type: NodeType::File,
            label: label.to_string(),
            summary: String::new(),
            tags: vec![],
            created_at: chrono::Utc::now().to_rfc3339(),
        };
        GraphRepo::new(conn).insert_nodes_batch(&[node]).unwrap();
    }

    fn insert_artifact_node(
        conn: &Connection,
        case_id: &str,
        id: &str,
        label: &str,
        tags: Vec<&str>,
    ) {
        let node = GraphNode {
            id: id.to_string(),
            case_id: case_id.to_string(),
            node_type: NodeType::Artifact,
            label: label.to_string(),
            summary: String::new(),
            tags: tags.into_iter().map(|s| s.to_string()).collect(),
            created_at: chrono::Utc::now().to_rfc3339(),
        };
        GraphRepo::new(conn).insert_nodes_batch(&[node]).unwrap();
    }

    fn insert_edge(
        conn: &Connection,
        case_id: &str,
        edge_id: &str,
        source_id: &str,
        target_id: &str,
        edge_type: &str,
    ) {
        conn.execute(
            "INSERT INTO graph_edges (id, case_id, source_id, target_id, edge_type, confidence, provenance, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, NULL, NULL, ?6)",
            rusqlite::params![
                edge_id,
                case_id,
                source_id,
                target_id,
                edge_type,
                chrono::Utc::now().to_rfc3339(),
            ],
        )
        .unwrap();
    }

    // ── CommunicatesWith tests ──────────────────────────────────────

    #[test]
    fn test_infer_communicates_with_from_email() {
        let conn = setup_db();

        // Two person entities
        insert_entity(
            &conn,
            "case-1",
            "entity-alice",
            "alice@example.com",
            vec!["entity", "person"],
        );
        insert_entity(
            &conn,
            "case-1",
            "entity-bob",
            "bob@test.org",
            vec!["entity", "person"],
        );

        // EmailMessage artifact
        insert_artifact_node(
            &conn,
            "case-1",
            "email-1",
            "Email from Alice to Bob",
            vec!["EmailMessage"],
        );

        // Edges: both persons --[correlates_with]--> same email artifact
        insert_edge(
            &conn,
            "case-1",
            "edge-1",
            "entity-alice",
            "email-1",
            "correlates_with",
        );
        insert_edge(
            &conn,
            "case-1",
            "edge-2",
            "entity-bob",
            "email-1",
            "correlates_with",
        );

        let rels = EntityRelationshipEngine::infer_relationships(&conn, "case-1").unwrap();

        // Should find CommunicatesWith between alice and bob
        let communicates: Vec<_> = rels
            .iter()
            .filter(|r| r.relationship_type == RelationshipType::CommunicatesWith)
            .collect();
        assert_eq!(communicates.len(), 1);

        let rel = &communicates[0];
        assert_eq!(rel.relationship_type, RelationshipType::CommunicatesWith);
        assert!(
            rel.evidence_edge_ids.contains(&"edge-1".to_string())
                || rel.evidence_edge_ids.contains(&"edge-2".to_string())
        );
        assert!(
            (rel.confidence - 0.85).abs() < f64::EPSILON,
            "expected 0.85 for 2 edges, got {}",
            rel.confidence
        );
    }

    #[test]
    fn test_infer_ownership_from_registry() {
        let conn = setup_db();

        // Person entity
        insert_entity(
            &conn,
            "case-1",
            "entity-user",
            "DOMAIN\\jdoe",
            vec!["entity", "person"],
        );

        // Device entity
        insert_entity(
            &conn,
            "case-1",
            "entity-pc",
            "DESKTOP-ABC123",
            vec!["entity", "device"],
        );

        // Registry artifact
        insert_artifact_node(
            &conn,
            "case-1",
            "reg-1",
            "SAM registry hive",
            vec!["Registry"],
        );

        // File node for registry hive
        insert_file_node(
            &conn,
            "case-1",
            "file-sam",
            "C:\\Windows\\System32\\config\\SAM",
        );

        // --- Edges forming the ownership pattern ---

        // Person --[derives_from]--> Registry
        insert_edge(
            &conn,
            "case-1",
            "edge-p2r",
            "entity-user",
            "reg-1",
            "derives_from",
        );

        // Registry --[references]--> File
        insert_edge(
            &conn,
            "case-1",
            "edge-r2f",
            "reg-1",
            "file-sam",
            "references",
        );

        // Device --[contains]--> File (edge from device to file)
        insert_edge(
            &conn,
            "case-1",
            "edge-d2f",
            "entity-pc",
            "file-sam",
            "contains",
        );

        let rels = EntityRelationshipEngine::infer_relationships(&conn, "case-1").unwrap();

        let owns: Vec<_> = rels
            .iter()
            .filter(|r| r.relationship_type == RelationshipType::Owns)
            .collect();
        assert!(!owns.is_empty(), "expected at least one Owns relationship");
        assert_eq!(owns[0].source_entity_id, "entity-user");
        assert_eq!(owns[0].target_entity_id, "entity-pc");
    }

    // ── Confidence scoring tests ────────────────────────────────────

    #[test]
    fn test_confidence_increases_with_multiple_edges() {
        let conn = setup_db();

        // Two person entities
        insert_entity(
            &conn,
            "case-1",
            "entity-alice",
            "alice@example.com",
            vec!["entity", "person"],
        );
        insert_entity(
            &conn,
            "case-1",
            "entity-bob",
            "bob@test.org",
            vec!["entity", "person"],
        );

        // Two separate EmailMessage artifacts (multiple independent sources)
        insert_artifact_node(&conn, "case-1", "email-1", "Email 1", vec!["EmailMessage"]);
        insert_artifact_node(&conn, "case-1", "email-2", "Email 2", vec!["EmailMessage"]);

        // Both entities correlated with both emails → 4 edges total
        insert_edge(
            &conn,
            "case-1",
            "edge-1",
            "entity-alice",
            "email-1",
            "correlates_with",
        );
        insert_edge(
            &conn,
            "case-1",
            "edge-2",
            "entity-bob",
            "email-1",
            "correlates_with",
        );
        insert_edge(
            &conn,
            "case-1",
            "edge-3",
            "entity-alice",
            "email-2",
            "correlates_with",
        );
        insert_edge(
            &conn,
            "case-1",
            "edge-4",
            "entity-bob",
            "email-2",
            "correlates_with",
        );

        let rels = EntityRelationshipEngine::infer_relationships(&conn, "case-1").unwrap();

        let communicates: Vec<_> = rels
            .iter()
            .filter(|r| r.relationship_type == RelationshipType::CommunicatesWith)
            .collect();
        assert_eq!(communicates.len(), 1);

        // 4+ edges → 0.95
        assert!(
            (communicates[0].confidence - 0.95).abs() < f64::EPSILON,
            "expected 0.95 for 4+ edges, got {}",
            communicates[0].confidence
        );
        assert_eq!(communicates[0].evidence_edge_ids.len(), 4);
    }

    #[test]
    fn test_no_relationship_when_no_edges() {
        let conn = setup_db();

        // Two person entities with NO connecting edges
        insert_entity(
            &conn,
            "case-1",
            "entity-alice",
            "alice@example.com",
            vec!["entity", "person"],
        );
        insert_entity(
            &conn,
            "case-1",
            "entity-bob",
            "bob@test.org",
            vec!["entity", "person"],
        );

        let rels = EntityRelationshipEngine::infer_relationships(&conn, "case-1").unwrap();
        assert!(
            rels.is_empty(),
            "expected no relationships when there are no edges"
        );
    }

    #[test]
    fn test_persist_and_reload() {
        let conn = setup_db();

        insert_entity(
            &conn,
            "case-1",
            "entity-alice",
            "alice@example.com",
            vec!["entity", "person"],
        );
        insert_entity(
            &conn,
            "case-1",
            "entity-bob",
            "bob@test.org",
            vec!["entity", "person"],
        );
        insert_artifact_node(&conn, "case-1", "email-1", "email", vec!["EmailMessage"]);
        insert_edge(
            &conn,
            "case-1",
            "edge-1",
            "entity-alice",
            "email-1",
            "correlates_with",
        );
        insert_edge(
            &conn,
            "case-1",
            "edge-2",
            "entity-bob",
            "email-1",
            "correlates_with",
        );

        let rels = EntityRelationshipEngine::infer_relationships(&conn, "case-1").unwrap();
        let count =
            EntityRelationshipEngine::persist_relationships(&conn, "case-1", &rels).unwrap();
        assert_eq!(count, 1);

        // Verify in DB
        let db_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM entity_relationships WHERE case_id = 'case-1'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(db_count, 1);

        // Verify relationship_type in DB
        let db_type: String = conn
            .query_row(
                "SELECT relationship_type FROM entity_relationships WHERE case_id = 'case-1'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(db_type, "communicates_with");
    }

    #[test]
    fn test_empty_case_returns_empty() {
        let conn = setup_db();
        let rels = EntityRelationshipEngine::infer_relationships(&conn, "case-1").unwrap();
        assert!(rels.is_empty());
    }

    #[test]
    fn test_infer_logged_into_from_wtmp() {
        let conn = setup_db();

        insert_entity(
            &conn,
            "case-1",
            "entity-user",
            "root",
            vec!["entity", "person"],
        );
        insert_entity(
            &conn,
            "case-1",
            "entity-server",
            "webserver-01",
            vec!["entity", "device"],
        );
        insert_artifact_node(&conn, "case-1", "wtmp-1", "wtmp login record", vec!["wtmp"]);
        insert_file_node(&conn, "case-1", "file-wtmp", "/var/log/wtmp");

        insert_edge(
            &conn,
            "case-1",
            "edge-p2w",
            "entity-user",
            "wtmp-1",
            "derives_from",
        );
        insert_edge(
            &conn,
            "case-1",
            "edge-w2f",
            "wtmp-1",
            "file-wtmp",
            "references",
        );
        insert_edge(
            &conn,
            "case-1",
            "edge-d2f",
            "entity-server",
            "file-wtmp",
            "contains",
        );

        let rels = EntityRelationshipEngine::infer_relationships(&conn, "case-1").unwrap();

        let logged_in: Vec<_> = rels
            .iter()
            .filter(|r| r.relationship_type == RelationshipType::LoggedInto)
            .collect();
        assert!(!logged_in.is_empty(), "expected LoggedInto relationship");
        assert_eq!(logged_in[0].source_entity_id, "entity-user");
        assert_eq!(logged_in[0].target_entity_id, "entity-server");
    }

    #[test]
    fn test_infer_executed_from_prefetch() {
        let conn = setup_db();

        insert_entity(
            &conn,
            "case-1",
            "entity-user",
            "DOMAIN\\jdoe",
            vec!["entity", "person"],
        );
        insert_artifact_node(
            &conn,
            "case-1",
            "pf-1",
            "CMD.EXE prefetch",
            vec!["Prefetch"],
        );
        insert_file_node(
            &conn,
            "case-1",
            "file-exe",
            "C:\\Windows\\System32\\cmd.exe",
        );

        insert_edge(
            &conn,
            "case-1",
            "edge-p2p",
            "entity-user",
            "pf-1",
            "derives_from",
        );
        insert_edge(
            &conn,
            "case-1",
            "edge-p2f",
            "pf-1",
            "file-exe",
            "references",
        );

        let rels = EntityRelationshipEngine::infer_relationships(&conn, "case-1").unwrap();

        let executed: Vec<_> = rels
            .iter()
            .filter(|r| r.relationship_type == RelationshipType::Executed)
            .collect();
        assert!(!executed.is_empty(), "expected Executed relationship");
        assert_eq!(executed[0].source_entity_id, "entity-user");
        assert_eq!(executed[0].target_entity_id, "file-exe");
    }
}
