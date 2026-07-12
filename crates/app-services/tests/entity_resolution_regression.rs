mod entity_phase4_support;

use app_services::entity_resolution::{
    CrossCaseEntityMatcher, EntityMergeEngine, EntityRelationshipEngine, MatchStrategy,
    RelationshipType,
};
use domain::{EdgeType, NodeType};
use entity_phase4_support::{
    case_db, create_case_file, insert_graph_edge, insert_graph_node, CASE_ID,
};

#[test]
fn merge_canonicalizes_groups_and_sorts_sources() {
    let conn = case_db();
    insert_graph_node(
        &conn,
        "entity-b",
        NodeType::Entity,
        "Alice@Example.COM",
        &["entity", "person"],
    );
    insert_graph_node(
        &conn,
        "entity-a",
        NodeType::Entity,
        "mailto:alice@example.com",
        &["entity", "person"],
    );

    let resolved = EntityMergeEngine::merge_entities(&conn, CASE_ID).expect("merge");
    assert_eq!(resolved.len(), 1);
    assert_eq!(resolved[0].canonical_value, "alice@example.com");
    assert_eq!(resolved[0].source_entities, vec!["entity-a", "entity-b"]);
    assert_eq!(resolved[0].confidence, 0.85);
}

#[test]
fn merge_keeps_distinct_values_and_types_separate() {
    let conn = case_db();
    for (id, label, entity_type) in [
        ("person-a", "user@example.com", "person"),
        ("account-a", "user@example.com", "account"),
        ("person-b", "other@example.com", "person"),
    ] {
        insert_graph_node(&conn, id, NodeType::Entity, label, &["entity", entity_type]);
    }
    let resolved = EntityMergeEngine::merge_entities(&conn, CASE_ID).expect("merge");
    assert_eq!(resolved.len(), 3);
    assert!(resolved.iter().all(|entity| entity.confidence == 0.70));
    assert_eq!(
        EntityMergeEngine::canonicalize_entity("sid:S-1-5-21-1", "account"),
        "s-1-5-21-1"
    );
}

#[test]
fn deduplication_repoints_edges_and_persists_resolution() {
    let conn = case_db();
    insert_graph_node(
        &conn,
        "entity-a",
        NodeType::Entity,
        "alice@example.com",
        &["entity", "person"],
    );
    insert_graph_node(
        &conn,
        "entity-b",
        NodeType::Entity,
        "Alice@Example.COM",
        &["entity", "person"],
    );
    insert_graph_node(&conn, "artifact-1", NodeType::Artifact, "mail", &[]);
    insert_graph_edge(
        &conn,
        "edge-a",
        "entity-a",
        "artifact-1",
        EdgeType::DerivesFrom,
    );
    insert_graph_edge(
        &conn,
        "edge-b",
        "entity-b",
        "artifact-1",
        EdgeType::DerivesFrom,
    );

    assert_eq!(
        EntityMergeEngine::deduplicate_entity_nodes(&conn, CASE_ID).expect("deduplicate"),
        1
    );
    let resolved_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM resolved_entities", [], |row| {
            row.get(0)
        })
        .expect("count resolved");
    let merge_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM entity_merge_log", [], |row| {
            row.get(0)
        })
        .expect("count merge log");
    assert_eq!((resolved_count, merge_count), (1, 1));
}

#[test]
fn relationship_inference_and_persistence_are_deterministic() {
    let conn = case_db();
    insert_graph_node(
        &conn,
        "entity-alice",
        NodeType::Entity,
        "alice@example.com",
        &["entity", "person"],
    );
    insert_graph_node(
        &conn,
        "entity-bob",
        NodeType::Entity,
        "bob@example.com",
        &["entity", "person"],
    );
    insert_graph_node(
        &conn,
        "email-1",
        NodeType::Artifact,
        "Email message",
        &["EmailMessage"],
    );
    insert_graph_edge(
        &conn,
        "edge-b",
        "entity-bob",
        "email-1",
        EdgeType::CorrelatesWith,
    );
    insert_graph_edge(
        &conn,
        "edge-a",
        "entity-alice",
        "email-1",
        EdgeType::CorrelatesWith,
    );

    let relationships =
        EntityRelationshipEngine::infer_relationships(&conn, CASE_ID).expect("infer");
    assert_eq!(relationships.len(), 1);
    assert_eq!(
        relationships[0].relationship_type,
        RelationshipType::CommunicatesWith
    );
    assert_eq!(relationships[0].evidence_edge_ids, vec!["edge-a", "edge-b"]);
    assert_eq!(
        EntityRelationshipEngine::persist_relationships(&conn, CASE_ID, &relationships)
            .expect("persist"),
        1
    );
}

#[test]
fn relationship_confidence_increases_with_independent_evidence() {
    let conn = case_db();
    insert_graph_node(
        &conn,
        "entity-a",
        NodeType::Entity,
        "a@example.com",
        &["entity", "person"],
    );
    insert_graph_node(
        &conn,
        "entity-b",
        NodeType::Entity,
        "b@example.com",
        &["entity", "person"],
    );
    for suffix in ["1", "2"] {
        let artifact = format!("email-{suffix}");
        insert_graph_node(
            &conn,
            &artifact,
            NodeType::Artifact,
            "Email",
            &["EmailMessage"],
        );
        insert_graph_edge(
            &conn,
            &format!("edge-a-{suffix}"),
            "entity-a",
            &artifact,
            EdgeType::CorrelatesWith,
        );
        insert_graph_edge(
            &conn,
            &format!("edge-b-{suffix}"),
            "entity-b",
            &artifact,
            EdgeType::CorrelatesWith,
        );
    }
    let relationships =
        EntityRelationshipEngine::infer_relationships(&conn, CASE_ID).expect("infer");
    assert_eq!(relationships.len(), 1);
    assert_eq!(relationships[0].confidence, 0.95);
    assert_eq!(relationships[0].evidence_edge_ids.len(), 4);
}

#[test]
fn relationship_inference_handles_empty_case() {
    let conn = case_db();
    assert!(
        EntityRelationshipEngine::infer_relationships(&conn, CASE_ID)
            .expect("infer empty case")
            .is_empty()
    );
}

#[test]
fn logged_in_and_executed_patterns_remain_available() {
    let conn = case_db();
    insert_graph_node(
        &conn,
        "entity-user",
        NodeType::Entity,
        "root",
        &["entity", "person"],
    );
    insert_graph_node(
        &conn,
        "entity-device",
        NodeType::Entity,
        "server-01",
        &["entity", "device"],
    );
    insert_graph_node(&conn, "wtmp-1", NodeType::Artifact, "wtmp", &["wtmp"]);
    insert_graph_node(&conn, "file-wtmp", NodeType::File, "/var/log/wtmp", &[]);
    insert_graph_edge(
        &conn,
        "edge-user-wtmp",
        "entity-user",
        "wtmp-1",
        EdgeType::DerivesFrom,
    );
    insert_graph_edge(
        &conn,
        "edge-wtmp-file",
        "wtmp-1",
        "file-wtmp",
        EdgeType::References,
    );
    insert_graph_edge(
        &conn,
        "edge-device-file",
        "entity-device",
        "file-wtmp",
        EdgeType::Contains,
    );
    insert_graph_node(
        &conn,
        "prefetch-1",
        NodeType::Artifact,
        "cmd.exe Prefetch",
        &["Prefetch"],
    );
    insert_graph_node(&conn, "file-cmd", NodeType::File, "cmd.exe", &[]);
    insert_graph_edge(
        &conn,
        "edge-user-prefetch",
        "entity-user",
        "prefetch-1",
        EdgeType::DerivesFrom,
    );
    insert_graph_edge(
        &conn,
        "edge-prefetch-file",
        "prefetch-1",
        "file-cmd",
        EdgeType::References,
    );

    let relationships =
        EntityRelationshipEngine::infer_relationships(&conn, CASE_ID).expect("infer");
    assert!(relationships
        .iter()
        .any(|item| item.relationship_type == RelationshipType::LoggedInto));
    assert!(relationships
        .iter()
        .any(|item| item.relationship_type == RelationshipType::Executed));
}

#[test]
fn cross_case_exact_normalized_and_fuzzy_matching_is_stable() {
    let temp = tempfile::TempDir::new().expect("temporary directory");
    let first = create_case_file(
        temp.path(),
        "case-a",
        &[
            ("person-a", "person", "alice@example.com"),
            ("account-a", "account", "DOMAIN\\jdoe"),
            ("device-a", "device", "desktop-a"),
        ],
    );
    let second = create_case_file(
        temp.path(),
        "case-b",
        &[
            ("person-b", "person", "alice@example.com"),
            ("account-b", "account", "jdoe"),
            ("device-b", "device", "desktop-b"),
        ],
    );

    let first_run =
        CrossCaseEntityMatcher::match_entities_across_cases(&[first.clone(), second.clone()])
            .expect("match entities");
    let second_run = CrossCaseEntityMatcher::match_entities_across_cases(&[first, second])
        .expect("repeat matching");
    assert_eq!(
        first_run.iter().map(|item| &item.id).collect::<Vec<_>>(),
        second_run.iter().map(|item| &item.id).collect::<Vec<_>>()
    );
    assert!(first_run
        .iter()
        .any(|item| item.match_strategy == MatchStrategy::Exact));
    assert!(first_run
        .iter()
        .any(|item| item.match_strategy == MatchStrategy::Normalized));
    assert!(first_run
        .iter()
        .any(|item| item.match_strategy == MatchStrategy::Fuzzy));
}

#[test]
fn cross_case_requires_two_databases() {
    let result = CrossCaseEntityMatcher::match_entities_across_cases(&[]);
    assert!(result.is_err());
}

#[test]
fn cross_case_empty_databases_return_no_matches() {
    let temp = tempfile::TempDir::new().expect("temporary directory");
    let first = create_case_file(temp.path(), "case-empty-a", &[]);
    let second = create_case_file(temp.path(), "case-empty-b", &[]);
    assert!(
        CrossCaseEntityMatcher::match_entities_across_cases(&[first, second])
            .expect("match empty databases")
            .is_empty()
    );
}
