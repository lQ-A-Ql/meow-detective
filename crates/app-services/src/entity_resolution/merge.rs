//! Entity merge engine — canonicalization, grouping, confidence scoring,
//! and graph-level deduplication of extracted entity nodes.

use rusqlite::Connection;
use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};
use unicode_normalization::UnicodeNormalization;

/// A resolved entity after canonicalization and merge.
#[derive(Debug, Clone)]
pub struct ResolvedEntity {
    pub id: String,
    pub entity_type: String,
    pub canonical_value: String,
    pub source_entities: Vec<String>,
    pub confidence: f64,
    pub attributes: Vec<String>,
}

/// Engine that canonicalizes entity values, groups related entities,
/// computes merge confidence, and deduplicates the graph.
pub struct EntityMergeEngine;

impl EntityMergeEngine {
    /// Canonicalize an entity value for consistent grouping.
    ///
    /// Applies lowercase, trim, and NFKD Unicode normalization.
    /// Strips common prefixes like `mailto:` (for email/person types)
    /// and `sid:` (for account types).
    pub fn canonicalize_entity(value: &str, entity_type: &str) -> String {
        let mut v: String = value.trim().to_lowercase().nfkd().collect();

        // Strip common prefixes
        let prefixes: &[&str] = match entity_type {
            "person" | "email" => &["mailto:"],
            "account" => &["sid:", "SID:"],
            _ => &[],
        };

        for prefix in prefixes {
            if let Some(stripped) = v.strip_prefix(prefix) {
                v = stripped.to_string();
                break;
            }
        }

        v
    }

    /// Extract the entity sub-type from a JSON-encoded tags array.
    ///
    /// Entity nodes carry tags like `["entity", "person"]`. This returns
    /// the first non-`entity` tag, or `"unknown"` as a fallback.
    fn extract_entity_type(tags_json: &str) -> String {
        let tags: Vec<String> = serde_json::from_str(tags_json).unwrap_or_default();
        tags.iter()
            .find(|t| *t != "entity")
            .cloned()
            .unwrap_or_else(|| "unknown".to_string())
    }

    /// Group entity nodes by (canonicalized_value, entity_type) and produce
    /// resolved entities with confidence scores based on source agreement.
    ///
    /// Confidence tiers:
    ///   - 3+ distinct source entities → 0.95
    ///   - 2 distinct source entities → 0.85
    ///   - 1 source entity → 0.70
    ///
    /// # Errors
    ///
    /// Returns an error string if the database query fails.
    pub fn merge_entities(conn: &Connection, case_id: &str) -> Result<Vec<ResolvedEntity>, String> {
        // ── Query all entity nodes for this case ──
        let mut stmt = conn
            .prepare(
                "SELECT id, label, tags FROM graph_nodes
                 WHERE case_id = ?1 AND node_type = 'entity'",
            )
            .map_err(|e| e.to_string())?;

        let rows: Vec<(String, String, String)> = stmt
            .query_map(rusqlite::params![case_id], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            })
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;

        if rows.is_empty() {
            return Ok(Vec::new());
        }

        // ── Group by (canonical_value, entity_type) ──
        let mut groups: HashMap<(String, String), Vec<(String, String)>> = HashMap::new();

        for (id, label, tags_json) in &rows {
            let entity_type = Self::extract_entity_type(tags_json);
            let canonical = Self::canonicalize_entity(label, &entity_type);
            groups
                .entry((canonical, entity_type))
                .or_default()
                .push((id.clone(), label.clone()));
        }

        // ── Build resolved entities ──
        let mut results: Vec<ResolvedEntity> = Vec::new();

        for ((canonical_value, entity_type), ents) in &groups {
            let source_count = ents.len();
            let confidence = if source_count >= 3 {
                0.95
            } else if source_count >= 2 {
                0.85
            } else {
                0.70
            };

            let source_entities: Vec<String> = ents.iter().map(|(id, _)| id.clone()).collect();

            // Collect distinct original label values as attributes
            let attributes: Vec<String> = ents
                .iter()
                .map(|(_, label)| label.clone())
                .collect::<HashSet<_>>()
                .into_iter()
                .collect();

            // Generate a stable ID derived from case + canonical value + type
            let id = Self::resolved_entity_id(case_id, canonical_value, entity_type);

            results.push(ResolvedEntity {
                id,
                entity_type: entity_type.clone(),
                canonical_value: canonical_value.clone(),
                source_entities,
                confidence,
                attributes,
            });
        }

        Ok(results)
    }

    /// Deduplicate entity graph nodes that share the same canonical value and
    /// entity type. For each group, the first entity is kept and all others
    /// have their edges re-pointed to the kept node before deletion.
    ///
    /// Each merge operation is recorded in `entity_merge_log`, and the
    /// resulting resolved entities are persisted into `resolved_entities`.
    ///
    /// Returns the total number of entity nodes that were merged (removed).
    ///
    /// # Errors
    ///
    /// Returns an error string if database mutations fail.
    pub fn deduplicate_entity_nodes(conn: &Connection, case_id: &str) -> Result<u64, String> {
        let resolved = Self::merge_entities(conn, case_id)?;
        let now = chrono::Utc::now().to_rfc3339();
        let mut merged_count = 0u64;

        for entity in &resolved {
            if entity.source_entities.len() <= 1 {
                continue;
            }

            // Keep the first entity node; merge the rest into it
            let kept_id = &entity.source_entities[0];

            for merged_id in entity.source_entities.iter().skip(1) {
                // Re-point outgoing edges (source) from merged → kept
                conn.execute(
                    "UPDATE graph_edges SET source_id = ?1
                     WHERE source_id = ?2 AND case_id = ?3",
                    rusqlite::params![kept_id, merged_id, case_id],
                )
                .map_err(|e| e.to_string())?;

                // Re-point incoming edges (target) from merged → kept
                conn.execute(
                    "UPDATE graph_edges SET target_id = ?1
                     WHERE target_id = ?2 AND case_id = ?3",
                    rusqlite::params![kept_id, merged_id, case_id],
                )
                .map_err(|e| e.to_string())?;

                // Log the merge for auditability (must happen before
                // deletion so the merged entity ID is still valid).
                let merge_id = format!("merge:{}:{}", case_id, uuid::Uuid::new_v4().as_simple());
                conn.execute(
                    "INSERT INTO entity_merge_log
                     (merge_id, case_id, kept_entity_id, merged_entity_id,
                      confidence, merged_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                    rusqlite::params![
                        merge_id,
                        case_id,
                        kept_id,
                        merged_id,
                        entity.confidence,
                        now,
                    ],
                )
                .map_err(|e| e.to_string())?;

                // Delete the merged node (CASCADE cleans up any edges
                // that still reference it)
                conn.execute(
                    "DELETE FROM graph_nodes WHERE id = ?1 AND case_id = ?2",
                    rusqlite::params![merged_id, case_id],
                )
                .map_err(|e| e.to_string())?;

                merged_count += 1;
            }
        }

        // Upsert resolved entities into the denormalized lookup table
        for entity in &resolved {
            let attrs_json = serde_json::to_string(&entity.attributes).unwrap_or_default();
            conn.execute(
                "INSERT OR REPLACE INTO resolved_entities
                 (id, case_id, entity_type, canonical_value, source_count,
                  confidence, attributes_json)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                rusqlite::params![
                    entity.id,
                    case_id,
                    entity.entity_type,
                    entity.canonical_value,
                    entity.source_entities.len() as i64,
                    entity.confidence,
                    attrs_json,
                ],
            )
            .map_err(|e| e.to_string())?;
        }

        Ok(merged_count)
    }

    /// Build a deterministic ID for a resolved entity based on the case,
    /// canonical value, and entity type.
    fn resolved_entity_id(case_id: &str, canonical_value: &str, entity_type: &str) -> String {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        case_id.hash(&mut hasher);
        canonical_value.hash(&mut hasher);
        entity_type.hash(&mut hasher);
        let hash_key = hasher.finish();
        format!("resolved:{}:{:016x}", case_id, hash_key)
    }
}

// ── Tests ─────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use domain::GraphNode;
    use domain::NodeType;
    use persistence_sqlite::connection::open_in_memory;
    use persistence_sqlite::runner;

    fn setup_db() -> Connection {
        let conn = open_in_memory().unwrap();
        runner::run_all(&conn).unwrap();

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

        conn
    }

    fn insert_entity_node(
        conn: &Connection,
        case_id: &str,
        id: &str,
        label: &str,
        entity_type_tag: &str,
    ) {
        let node = GraphNode {
            id: id.to_string(),
            case_id: case_id.to_string(),
            node_type: NodeType::Entity,
            label: label.to_string(),
            summary: String::new(),
            tags: vec!["entity".into(), entity_type_tag.into()],
            created_at: Utc::now().to_rfc3339(),
        };
        persistence_sqlite::repositories::graph_repo::GraphRepo::new(conn)
            .insert_nodes_batch(&[node])
            .unwrap();
    }

    fn insert_edge(
        conn: &Connection,
        case_id: &str,
        edge_id: &str,
        source_id: &str,
        target_id: &str,
    ) {
        use domain::{EdgeType, GraphEdge};
        let edge = GraphEdge {
            id: edge_id.to_string(),
            case_id: case_id.to_string(),
            source_id: source_id.to_string(),
            target_id: target_id.to_string(),
            edge_type: EdgeType::DerivesFrom,
            confidence: None,
            provenance: Some("test".into()),
            created_at: Utc::now().to_rfc3339(),
        };
        // Use raw SQL to insert a single edge — the repo batch insert
        // supports it, but we'll use execute directly to avoid needing
        // to import the full GraphRepo here.
        conn.execute(
            "INSERT INTO graph_edges (id, case_id, source_id, target_id, edge_type, confidence, provenance, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            rusqlite::params![
                edge.id,
                edge.case_id,
                edge.source_id,
                edge.target_id,
                "derives_from",
                edge.confidence,
                edge.provenance,
                edge.created_at,
            ],
        )
        .unwrap();
    }

    // ── Canonicalization tests ──────────────────────────────────────

    #[test]
    fn test_canonicalize_normalizes_email() {
        let result = EntityMergeEngine::canonicalize_entity("  Alice@Example.COM  ", "person");
        assert_eq!(result, "alice@example.com");
    }

    #[test]
    fn canonicalize_strips_mailto_prefix() {
        let result = EntityMergeEngine::canonicalize_entity("mailto:alice@example.com", "person");
        assert_eq!(result, "alice@example.com");
    }

    #[test]
    fn canonicalize_strips_sid_prefix() {
        let result = EntityMergeEngine::canonicalize_entity(
            "sid:s-1-5-21-3623811015-3361044348-30300820-1013",
            "account",
        );
        assert_eq!(result, "s-1-5-21-3623811015-3361044348-30300820-1013");
    }

    #[test]
    fn canonicalize_nfkd_normalizes_unicode() {
        // U+00E9 (é) decomposes to e + combining acute accent
        let input = "caf\u{00E9}@example.com"; // café
        let result = EntityMergeEngine::canonicalize_entity(input, "person");
        assert!(result.starts_with("cafe"));
        assert!(result.contains('@'));
        assert!(result.len() > 16); // decomposed longer than original
    }

    #[test]
    fn canonicalize_trims_whitespace() {
        let result = EntityMergeEngine::canonicalize_entity("  \t bob@test.org \n ", "person");
        assert_eq!(result, "bob@test.org");
    }

    #[test]
    fn canonicalize_empty_becomes_empty() {
        let result = EntityMergeEngine::canonicalize_entity("   ", "person");
        assert_eq!(result, "");
    }

    // ── Merge tests ─────────────────────────────────────────────────

    #[test]
    fn test_merge_same_email_across_artifacts() {
        let conn = setup_db();
        insert_entity_node(&conn, "case-1", "entity-1", "alice@example.com", "person");
        insert_entity_node(&conn, "case-1", "entity-2", "Alice@Example.COM", "person");

        let resolved = EntityMergeEngine::merge_entities(&conn, "case-1").unwrap();

        assert_eq!(resolved.len(), 1);
        assert_eq!(resolved[0].canonical_value, "alice@example.com");
        assert_eq!(resolved[0].source_entities.len(), 2);
    }

    #[test]
    fn test_merge_confidence_increases_with_source_count() {
        let conn = setup_db();

        // Single source → 0.70
        insert_entity_node(&conn, "case-1", "e1", "alice@example.com", "person");
        let resolved = EntityMergeEngine::merge_entities(&conn, "case-1").unwrap();
        assert_eq!(resolved.len(), 1);
        assert!(
            (resolved[0].confidence - 0.70).abs() < f64::EPSILON,
            "expected 0.70, got {}",
            resolved[0].confidence
        );

        // Two sources → 0.85
        insert_entity_node(&conn, "case-1", "e2", "Alice@Example.COM", "person");
        let resolved = EntityMergeEngine::merge_entities(&conn, "case-1").unwrap();
        assert_eq!(resolved.len(), 1);
        assert!(
            (resolved[0].confidence - 0.85).abs() < f64::EPSILON,
            "expected 0.85, got {}",
            resolved[0].confidence
        );

        // Three sources → 0.95
        insert_entity_node(&conn, "case-1", "e3", "alice@example.com", "person");
        let resolved = EntityMergeEngine::merge_entities(&conn, "case-1").unwrap();
        assert_eq!(resolved.len(), 1);
        assert!(
            (resolved[0].confidence - 0.95).abs() < f64::EPSILON,
            "expected 0.95, got {}",
            resolved[0].confidence
        );
    }

    #[test]
    fn test_different_entities_not_merged() {
        let conn = setup_db();
        insert_entity_node(&conn, "case-1", "e1", "alice@example.com", "person");
        insert_entity_node(&conn, "case-1", "e2", "bob@test.org", "person");

        let resolved = EntityMergeEngine::merge_entities(&conn, "case-1").unwrap();

        assert_eq!(resolved.len(), 2);
        let canonical_values: Vec<&str> = resolved
            .iter()
            .map(|r| r.canonical_value.as_str())
            .collect();
        assert!(canonical_values.contains(&"alice@example.com"));
        assert!(canonical_values.contains(&"bob@test.org"));
    }

    #[test]
    fn merge_different_types_not_grouped() {
        let conn = setup_db();
        // Same label, different entity types
        insert_entity_node(&conn, "case-1", "e1", "user@example.com", "person");
        insert_entity_node(&conn, "case-1", "e2", "user@example.com", "account");

        let resolved = EntityMergeEngine::merge_entities(&conn, "case-1").unwrap();

        assert_eq!(resolved.len(), 2);
    }

    #[test]
    fn merge_empty_case_returns_empty() {
        let conn = setup_db();
        let resolved = EntityMergeEngine::merge_entities(&conn, "case-1").unwrap();
        assert!(resolved.is_empty());
    }

    // ── Deduplication tests ─────────────────────────────────────────

    #[test]
    fn deduplicate_merges_entity_nodes() {
        let conn = setup_db();
        insert_entity_node(&conn, "case-1", "entity-1", "alice@example.com", "person");
        insert_entity_node(&conn, "case-1", "entity-2", "Alice@Example.COM", "person");

        // Add a common target node so edges have somewhere to point
        use domain::GraphNode;
        use domain::NodeType;
        let artifact = GraphNode {
            id: "artifact-1".to_string(),
            case_id: "case-1".to_string(),
            node_type: NodeType::Artifact,
            label: "artifact".to_string(),
            summary: String::new(),
            tags: vec![],
            created_at: Utc::now().to_rfc3339(),
        };
        persistence_sqlite::repositories::graph_repo::GraphRepo::new(&conn)
            .insert_nodes_batch(&[artifact])
            .unwrap();

        // Create edges from both entities to the artifact
        insert_edge(&conn, "case-1", "edge-1", "entity-1", "artifact-1");
        insert_edge(&conn, "case-1", "edge-2", "entity-2", "artifact-1");

        let merged = EntityMergeEngine::deduplicate_entity_nodes(&conn, "case-1").unwrap();
        assert_eq!(merged, 1, "should have merged 1 node");

        // entity-2 should be gone
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM graph_nodes WHERE id = 'entity-2'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 0, "entity-2 should be deleted");

        // entity-1 should still exist
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM graph_nodes WHERE id = 'entity-1'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 1, "entity-1 should still exist");

        // The edge from entity-2 should have been re-pointed to entity-1
        let edge_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM graph_edges WHERE source_id = 'entity-1'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert!(edge_count >= 1, "edges should point to entity-1");

        // Merge log should contain one entry
        let log_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM entity_merge_log WHERE case_id = 'case-1'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(log_count, 1);

        // Resolved entities should be populated
        let resolved_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM resolved_entities WHERE case_id = 'case-1'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(resolved_count, 1);
    }

    #[test]
    fn deduplicate_no_duplicates_merges_zero() {
        let conn = setup_db();
        insert_entity_node(&conn, "case-1", "entity-1", "alice@example.com", "person");
        insert_entity_node(&conn, "case-1", "entity-2", "bob@test.org", "person");

        let merged = EntityMergeEngine::deduplicate_entity_nodes(&conn, "case-1").unwrap();
        assert_eq!(merged, 0);

        // Both nodes should still exist
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM graph_nodes WHERE node_type = 'entity' AND case_id = 'case-1'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 2);
    }

    #[test]
    fn deduplicate_empty_case() {
        let conn = setup_db();
        let merged = EntityMergeEngine::deduplicate_entity_nodes(&conn, "case-1").unwrap();
        assert_eq!(merged, 0);
    }

    // ── Resolved entity ID stability ────────────────────────────────

    #[test]
    fn resolved_entity_id_is_deterministic() {
        // Use the private fn via a merge result — same inputs should
        // produce the same ID every time.
        let conn = setup_db();
        insert_entity_node(&conn, "case-1", "e1", "alice@example.com", "person");

        let r1 = EntityMergeEngine::merge_entities(&conn, "case-1").unwrap();
        let r2 = EntityMergeEngine::merge_entities(&conn, "case-1").unwrap();

        assert_eq!(r1.len(), 1);
        assert_eq!(r2.len(), 1);
        assert_eq!(r1[0].id, r2[0].id, "resolved entity ID must be stable");
    }
}
