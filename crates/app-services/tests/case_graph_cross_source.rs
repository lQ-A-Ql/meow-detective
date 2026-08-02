use app_services::{case_service, graph_service, source_db};
use domain::{
    DataSource, DataSourceId, DataSourceKind, DataSourceProvenance, EdgeType, GraphEdge, GraphNode,
    NodeType,
};
use persistence_sqlite::repositories::{
    datasource_repo::{DataSourceRepo, DataSourceStorage},
    graph_repo::GraphRepo,
};
use rusqlite::Connection;
use transport::dto::GraphQueryDto;

fn node(id: &str, case_id: &str, node_type: NodeType, label: &str, tags: &[&str]) -> GraphNode {
    GraphNode {
        id: id.to_string(),
        case_id: case_id.to_string(),
        node_type,
        label: label.to_string(),
        summary: format!("summary:{id}"),
        tags: tags.iter().map(|tag| (*tag).to_string()).collect(),
        created_at: "2026-08-02T00:00:00Z".to_string(),
    }
}

fn edge(id: &str, case_id: &str, source_id: &str, target_id: &str) -> GraphEdge {
    GraphEdge {
        id: id.to_string(),
        case_id: case_id.to_string(),
        source_id: source_id.to_string(),
        target_id: target_id.to_string(),
        edge_type: EdgeType::DerivesFrom,
        confidence: Some(0.9),
        provenance: Some("entity-test".to_string()),
        created_at: "2026-08-02T00:00:00Z".to_string(),
    }
}

fn register_source(
    case_conn: &Connection,
    case_root: &std::path::Path,
    case_id: &domain::CaseId,
    source_id: &str,
    platform: &str,
    nodes: &[GraphNode],
    edges: &[GraphEdge],
) {
    let source = DataSource {
        id: DataSourceId(source_id.to_string()),
        name: source_id.to_string(),
        kind: DataSourceKind::E01,
        source_path: case_root.join(format!("{source_id}.E01")),
        imported_at: chrono::Utc::now(),
        provenance: DataSourceProvenance::unknown(),
    };
    let mut storage = DataSourceStorage::source_db(source_id, Some(platform), None);
    storage.import_state = "ready".to_string();
    DataSourceRepo::new(case_conn)
        .insert_with_storage(case_id, &source, &storage)
        .unwrap();
    let source_conn = source_db::open_source_db(case_root, &source.id).unwrap();
    DataSourceRepo::new(&source_conn)
        .upsert_source_local_metadata(case_id, &source)
        .unwrap();
    GraphRepo::new(&source_conn)
        .insert_nodes_batch(nodes)
        .unwrap();
    GraphRepo::new(&source_conn)
        .insert_edges_batch(edges)
        .unwrap();
}

fn graph_query(start_ids: Vec<String>, edge_limit: u32) -> GraphQueryDto {
    GraphQueryDto {
        start_ids,
        edge_types: Vec::new(),
        max_depth: 2,
        confidence_floor: None,
        limit: 100,
        edge_limit,
    }
}

#[test]
fn exact_entities_bridge_windows_and_linux_source_graphs() {
    let temp = tempfile::TempDir::new().unwrap();
    let active = case_service::create_case(temp.path(), "case-graph-exact", None).unwrap();
    active
        .with_conn(|case_conn| {
            let case_id = &active.meta.id.0;
            register_source(
                case_conn,
                &active.case_root,
                &active.meta.id,
                "windows",
                "windows",
                &[
                    node(
                        "entity-alice",
                        case_id,
                        NodeType::Entity,
                        "MAILTO:Alice@Example.com",
                        &["entity", "person"],
                    ),
                    node(
                        "artifact-mail",
                        case_id,
                        NodeType::Artifact,
                        "Windows mail",
                        &[],
                    ),
                ],
                &[edge("edge-mail", case_id, "entity-alice", "artifact-mail")],
            );
            register_source(
                case_conn,
                &active.case_root,
                &active.meta.id,
                "linux",
                "linux",
                &[
                    node(
                        "entity-alice",
                        case_id,
                        NodeType::Entity,
                        "alice@example.com",
                        &["entity", "person"],
                    ),
                    node(
                        "artifact-login",
                        case_id,
                        NodeType::Artifact,
                        "Linux login",
                        &[],
                    ),
                ],
                &[edge(
                    "edge-login",
                    case_id,
                    "entity-alice",
                    "artifact-login",
                )],
            );

            let snapshot =
                graph_service::get_graph_snapshot_for_case(case_conn, &active.case_root, case_id)
                    .unwrap();
            assert_eq!(snapshot.data_source_count, 2);
            assert_eq!(snapshot.cross_source_entity_count, 1);
            assert_eq!(snapshot.cross_source_edge_count, 2);
            assert_eq!(snapshot.seed_ids.len(), 1);

            let result = graph_service::query_graph_for_case(
                case_conn,
                &active.case_root,
                case_id,
                graph_query(snapshot.seed_ids, 20),
            )
            .unwrap();
            assert!(!result.truncated);
            assert_eq!(result.data_source_ids, vec!["linux", "windows"]);
            assert!(result
                .nodes
                .iter()
                .any(|node| node.id == "ds:windows:artifact-mail"));
            assert!(result
                .nodes
                .iter()
                .any(|node| node.id == "ds:linux:artifact-login"));
            assert_eq!(
                result
                    .edges
                    .iter()
                    .filter(|edge| edge.id.starts_with("case:edge:"))
                    .count(),
                2
            );
            Ok(())
        })
        .unwrap();
}

#[test]
fn mixed_source_seeds_are_supported_and_edge_budget_is_reported() {
    let temp = tempfile::TempDir::new().unwrap();
    let active = case_service::create_case(temp.path(), "case-graph-mixed", None).unwrap();
    active
        .with_conn(|case_conn| {
            let case_id = &active.meta.id.0;
            for (source_id, platform) in [("one", "windows"), ("two", "linux")] {
                register_source(
                    case_conn,
                    &active.case_root,
                    &active.meta.id,
                    source_id,
                    platform,
                    &[node(
                        "entity-device",
                        case_id,
                        NodeType::Entity,
                        "HOST-01",
                        &["entity", "device"],
                    )],
                    &[],
                );
            }
            let result = graph_service::query_graph_for_case(
                case_conn,
                &active.case_root,
                case_id,
                graph_query(
                    vec![
                        "ds:one:entity-device".to_string(),
                        "ds:two:entity-device".to_string(),
                    ],
                    1,
                ),
            )
            .unwrap();
            assert!(result.truncated);
            assert_eq!(result.edge_count, 1);
            Ok(())
        })
        .unwrap();
}

#[test]
fn similar_but_non_identical_entities_are_not_connected() {
    let temp = tempfile::TempDir::new().unwrap();
    let active = case_service::create_case(temp.path(), "case-graph-no-fuzzy", None).unwrap();
    active
        .with_conn(|case_conn| {
            let case_id = &active.meta.id.0;
            for (source_id, platform, label) in [
                ("one", "windows", "alice@example.com"),
                ("two", "linux", "alice+tag@example.com"),
            ] {
                register_source(
                    case_conn,
                    &active.case_root,
                    &active.meta.id,
                    source_id,
                    platform,
                    &[node(
                        "entity-person",
                        case_id,
                        NodeType::Entity,
                        label,
                        &["entity", "person"],
                    )],
                    &[],
                );
            }
            let snapshot =
                graph_service::get_graph_snapshot_for_case(case_conn, &active.case_root, case_id)
                    .unwrap();
            assert_eq!(snapshot.cross_source_entity_count, 0);
            assert_eq!(snapshot.cross_source_edge_count, 0);
            assert!(snapshot.seed_ids.is_empty());
            Ok(())
        })
        .unwrap();
}

#[test]
fn source_graph_change_invalidates_case_projection() {
    let temp = tempfile::TempDir::new().unwrap();
    let active = case_service::create_case(temp.path(), "case-graph-refresh", None).unwrap();
    active
        .with_conn(|case_conn| {
            let case_id = &active.meta.id.0;
            register_source(
                case_conn,
                &active.case_root,
                &active.meta.id,
                "one",
                "windows",
                &[node(
                    "entity-device",
                    case_id,
                    NodeType::Entity,
                    "HOST-01",
                    &["entity", "device"],
                )],
                &[],
            );
            register_source(
                case_conn,
                &active.case_root,
                &active.meta.id,
                "two",
                "linux",
                &[],
                &[],
            );
            let before =
                graph_service::get_graph_snapshot_for_case(case_conn, &active.case_root, case_id)
                    .unwrap();
            assert_eq!(before.cross_source_entity_count, 0);

            let source_conn = source_db::open_registered_source_db(
                case_conn,
                &active.case_root,
                &DataSourceId("two".to_string()),
            )
            .unwrap();
            GraphRepo::new(&source_conn)
                .insert_nodes_batch(&[node(
                    "entity-device",
                    case_id,
                    NodeType::Entity,
                    "host-01",
                    &["entity", "device"],
                )])
                .unwrap();

            let after =
                graph_service::get_graph_snapshot_for_case(case_conn, &active.case_root, case_id)
                    .unwrap();
            assert_eq!(after.cross_source_entity_count, 1);
            assert_ne!(before.projection_built_at, after.projection_built_at);
            Ok(())
        })
        .unwrap();
}

#[test]
fn invalid_edge_type_is_rejected_instead_of_falling_back() {
    let temp = tempfile::TempDir::new().unwrap();
    let active = case_service::create_case(temp.path(), "case-graph-filter", None).unwrap();
    active
        .with_conn(|case_conn| {
            let error = graph_service::query_graph_for_case(
                case_conn,
                &active.case_root,
                &active.meta.id.0,
                GraphQueryDto {
                    start_ids: Vec::new(),
                    edge_types: vec!["not-a-relation".to_string()],
                    max_depth: 1,
                    confidence_floor: None,
                    limit: 10,
                    edge_limit: 10,
                },
            )
            .unwrap_err();
            assert!(matches!(
                error,
                graph_service::GraphServiceError::InvalidInput(_)
            ));
            Ok(())
        })
        .unwrap();
}
