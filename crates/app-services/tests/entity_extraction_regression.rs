mod entity_phase4_support;

use std::collections::BTreeMap;

use app_services::entity_extraction::{
    extract_entities_from_artifacts, hash_entity_value, index_entities, lookup_entity,
    normalize_entity_value,
};
use entity_phase4_support::{case_db, insert_artifact, CASE_ID};
use persistence_sqlite::repositories::graph_repo::GraphRepo;
use serde_json::Value;

#[test]
fn normalization_and_hashing_are_stable() {
    let normalized = normalize_entity_value("  Alice@Example.COM  ");
    assert_eq!(normalized, "alice@example.com");
    assert_eq!(
        hash_entity_value(&normalized),
        hash_entity_value(&normalized)
    );
    assert_eq!(hash_entity_value(&normalized).len(), 16);
    assert_ne!(
        hash_entity_value("alice@example.com"),
        hash_entity_value("bob@example.com")
    );
    assert!(normalize_entity_value("caf\u{00e9}").starts_with("cafe"));
}

#[test]
fn extracts_email_sid_and_hostname_entities() {
    let conn = case_db();
    let mut attrs = BTreeMap::new();
    attrs.insert(
        "hostname".to_string(),
        Value::String("SERVER-01".to_string()),
    );
    insert_artifact(
        &conn,
        "artifact-1",
        "Test",
        "alice@example.com",
        "S-1-5-21-1-2-3-1001",
        attrs,
    );

    assert_eq!(
        extract_entities_from_artifacts(&conn, CASE_ID).expect("extract entities"),
        3
    );
    let snapshot = GraphRepo::new(&conn)
        .get_snapshot(CASE_ID)
        .expect("read graph");
    assert_eq!(snapshot.total_nodes, 4);
    assert_eq!(snapshot.total_edges, 3);
}

#[test]
fn extraction_deduplicates_sources_and_orders_edges() {
    let conn = case_db();
    for id in ["artifact-b", "artifact-a"] {
        insert_artifact(&conn, id, "Test", "alice@example.com", "", BTreeMap::new());
    }

    assert_eq!(
        extract_entities_from_artifacts(&conn, CASE_ID).expect("extract entities"),
        1
    );
    let mut statement = conn
        .prepare(
            "SELECT target_id FROM graph_edges
             WHERE edge_type = 'derives_from' ORDER BY id",
        )
        .expect("prepare edge query");
    let targets: Vec<String> = statement
        .query_map([], |row| row.get(0))
        .expect("query edges")
        .collect::<Result<_, _>>()
        .expect("collect edges");
    assert_eq!(targets, vec!["artifact-a", "artifact-b"]);
}

#[test]
fn index_lookup_normalizes_and_merges_reindex_sources() {
    let conn = case_db();
    insert_artifact(
        &conn,
        "artifact-1",
        "Test",
        "alice@example.com",
        "",
        BTreeMap::new(),
    );
    assert_eq!(index_entities(&conn, CASE_ID).expect("index"), 1);

    insert_artifact(
        &conn,
        "artifact-2",
        "Test",
        "Alice@Example.COM",
        "",
        BTreeMap::new(),
    );
    assert_eq!(index_entities(&conn, CASE_ID).expect("reindex"), 1);
    assert_eq!(
        lookup_entity(&conn, " Alice@Example.COM ", "person"),
        Some(vec!["artifact-1".to_string(), "artifact-2".to_string()])
    );
}

#[test]
fn extraction_uses_index_and_empty_cases_return_zero() {
    let conn = case_db();
    assert_eq!(
        extract_entities_from_artifacts(&conn, CASE_ID).expect("empty extraction"),
        0
    );
    insert_artifact(
        &conn,
        "artifact-1",
        "Test",
        "alice@example.com",
        "",
        BTreeMap::new(),
    );
    index_entities(&conn, CASE_ID).expect("index");
    assert_eq!(
        extract_entities_from_artifacts(&conn, CASE_ID).expect("indexed extraction"),
        1
    );
}

#[test]
fn artifacts_without_entity_patterns_return_zero() {
    let conn = case_db();
    insert_artifact(
        &conn,
        "artifact-plain",
        "Test",
        "ordinary title",
        "ordinary summary",
        BTreeMap::new(),
    );
    assert_eq!(
        extract_entities_from_artifacts(&conn, CASE_ID).expect("plain extraction"),
        0
    );
    assert!(lookup_entity(&conn, "nobody@example.com", "person").is_none());
}
