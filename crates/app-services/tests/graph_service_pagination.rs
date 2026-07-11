use app_services::{active_case::ActiveCase, case_service, graph_service, source_db};
use domain::{
    DataSource, DataSourceId, DataSourceKind, DataSourceProvenance, EdgeType, GraphEdge, GraphNode,
    NodeType,
};
use persistence_sqlite::repositories::{
    datasource_repo::{DataSourceRepo, DataSourceStorage},
    graph_repo::GraphRepo,
};
use rusqlite::Connection;
use transport::dto::{GraphQueryDto, ListGraphNodesRequest};

fn make_node(id: &str, case_id: &str, created_at: String) -> GraphNode {
    GraphNode {
        id: id.to_string(),
        case_id: case_id.to_string(),
        node_type: NodeType::File,
        label: id.to_string(),
        summary: format!("Summary for {id}"),
        tags: vec!["pagination".to_string()],
        created_at,
    }
}

fn register_ready_graph_source(
    case_conn: &Connection,
    active: &ActiveCase,
    source_id: &str,
    platform: &str,
    nodes: &[GraphNode],
) {
    register_graph_source(case_conn, active, source_id, platform, "ready", nodes, &[]);
}

fn register_graph_source(
    case_conn: &Connection,
    active: &ActiveCase,
    source_id: &str,
    platform: &str,
    import_state: &str,
    nodes: &[GraphNode],
    edges: &[GraphEdge],
) {
    let source = DataSource {
        id: DataSourceId(source_id.to_string()),
        name: source_id.to_string(),
        kind: DataSourceKind::E01,
        source_path: active.case_root.join(format!("{source_id}.E01")),
        imported_at: chrono::Utc::now(),
        provenance: DataSourceProvenance::unknown(),
    };
    let mut storage = DataSourceStorage::source_db(source_id, Some(platform), None);
    storage.import_state = import_state.to_string();
    DataSourceRepo::new(case_conn)
        .insert_with_storage(&active.meta.id, &source, &storage)
        .expect("register ready graph source");

    let source_conn = source_db::open_source_db(&active.case_root, &source.id)
        .expect("open graph source database");
    DataSourceRepo::new(&source_conn)
        .upsert_source_local_metadata(&active.meta.id, &source)
        .expect("persist source-local metadata");
    GraphRepo::new(&source_conn)
        .insert_nodes_batch(nodes)
        .expect("insert source graph nodes");
    GraphRepo::new(&source_conn)
        .insert_edges_batch(edges)
        .expect("insert source graph edges");
}

#[test]
fn dual_source_deep_page_applies_global_offset_once() {
    let temp = tempfile::TempDir::new().expect("create graph pagination root");
    let active =
        case_service::create_case(temp.path(), "graph-deep-page", Some("stage2-pagination"))
            .expect("create graph pagination case");

    active
        .with_conn(|case_conn| {
            const NODE_COUNT: usize = 8_192;
            const OFFSET: u32 = 7_013;
            const LIMIT: u32 = 37;
            let base = chrono::DateTime::parse_from_rfc3339("2026-01-01T00:00:00Z")
                .expect("parse timestamp");
            let mut windows_nodes = Vec::new();
            let mut linux_nodes = Vec::new();

            for rank in 0..NODE_COUNT {
                let created_at = (base + chrono::Duration::seconds(rank as i64)).to_rfc3339();
                let node = make_node(&format!("node-{rank:04}"), &active.meta.id.0, created_at);
                if rank % 2 == 0 {
                    windows_nodes.push(node);
                } else {
                    linux_nodes.push(node);
                }
            }

            register_ready_graph_source(
                case_conn,
                &active,
                "windows-source",
                "windows",
                &windows_nodes,
            );
            register_ready_graph_source(case_conn, &active, "linux-source", "linux", &linux_nodes);

            let page = graph_service::list_graph_nodes_for_case(
                case_conn,
                &active.case_root,
                &active.meta.id.0,
                ListGraphNodesRequest {
                    limit: LIMIT,
                    offset: OFFSET,
                },
            )
            .expect("list merged deep graph page");
            let expected = (0..NODE_COUNT)
                .rev()
                .skip(OFFSET as usize)
                .take(LIMIT as usize)
                .map(|rank| {
                    let source = if rank % 2 == 0 {
                        "windows-source"
                    } else {
                        "linux-source"
                    };
                    format!("ds:{source}:node-{rank:04}")
                })
                .collect::<Vec<_>>();
            let actual = page.into_iter().map(|node| node.id).collect::<Vec<_>>();

            assert_eq!(actual, expected);
            Ok(())
        })
        .expect("validate globally paginated graph nodes");
}

#[test]
fn dual_source_ties_are_stable_and_max_offset_does_not_overflow() {
    let temp = tempfile::TempDir::new().expect("create graph ordering root");
    let active =
        case_service::create_case(temp.path(), "graph-stable-order", Some("stage2-pagination"))
            .expect("create graph ordering case");

    active
        .with_conn(|case_conn| {
            let timestamp = "2026-01-01T00:00:00+00:00".to_string();
            let alpha_nodes = vec![
                make_node("node-b", &active.meta.id.0, timestamp.clone()),
                make_node("node-a", &active.meta.id.0, timestamp.clone()),
            ];
            let beta_nodes = vec![
                make_node("node-b", &active.meta.id.0, timestamp.clone()),
                make_node("node-a", &active.meta.id.0, timestamp),
            ];
            register_ready_graph_source(case_conn, &active, "alpha", "windows", &alpha_nodes);
            register_ready_graph_source(case_conn, &active, "beta", "linux", &beta_nodes);

            let request = ListGraphNodesRequest {
                limit: 2,
                offset: 1,
            };
            let first = graph_service::list_graph_nodes_for_case(
                case_conn,
                &active.case_root,
                &active.meta.id.0,
                request.clone(),
            )
            .expect("list first deterministic page");
            let second = graph_service::list_graph_nodes_for_case(
                case_conn,
                &active.case_root,
                &active.meta.id.0,
                request,
            )
            .expect("list repeated deterministic page");
            let ids = first
                .iter()
                .map(|node| node.id.as_str())
                .collect::<Vec<_>>();

            assert_eq!(first, second);
            assert_eq!(ids, vec!["ds:alpha:node-b", "ds:beta:node-a"]);

            let beyond_end = graph_service::list_graph_nodes_for_case(
                case_conn,
                &active.case_root,
                &active.meta.id.0,
                ListGraphNodesRequest {
                    limit: 500,
                    offset: u32::MAX,
                },
            )
            .expect("maximum offset must not overflow");
            assert!(beyond_end.is_empty());
            Ok(())
        })
        .expect("validate deterministic graph ordering");
}

#[test]
fn scoped_graph_reads_reject_non_ready_sources() {
    let temp = tempfile::TempDir::new().expect("create graph readiness root");
    let active =
        case_service::create_case(temp.path(), "graph-readiness", Some("stage2-pagination"))
            .expect("create graph readiness case");

    active
        .with_conn(|case_conn| {
            let nodes = vec![
                make_node(
                    "node-a",
                    &active.meta.id.0,
                    "2026-01-01T00:00:00Z".to_string(),
                ),
                make_node(
                    "node-b",
                    &active.meta.id.0,
                    "2026-01-01T00:00:01Z".to_string(),
                ),
            ];
            let edges = vec![GraphEdge {
                id: "edge-a".to_string(),
                case_id: active.meta.id.0.clone(),
                source_id: "node-a".to_string(),
                target_id: "node-b".to_string(),
                edge_type: EdgeType::References,
                confidence: Some(1.0),
                provenance: None,
                created_at: "2026-01-01T00:00:02Z".to_string(),
            }];
            register_graph_source(
                case_conn,
                &active,
                "importing-source",
                "linux",
                "importing",
                &nodes,
                &edges,
            );

            let query_error = graph_service::query_graph_for_case(
                case_conn,
                &active.case_root,
                &active.meta.id.0,
                GraphQueryDto {
                    start_ids: vec!["ds:importing-source:node-a".to_string()],
                    edge_types: vec![],
                    max_depth: 1,
                    confidence_floor: None,
                    limit: 10,
                },
            )
            .expect_err("seeded query must reject importing source");
            let neighborhood_error = graph_service::get_node_neighborhood_for_case(
                case_conn,
                &active.case_root,
                &active.meta.id.0,
                "ds:importing-source:node-a",
                1,
            )
            .expect_err("neighborhood must reject importing source");
            let provenance_error = graph_service::get_provenance_chain_for_case(
                case_conn,
                &active.case_root,
                &active.meta.id.0,
                "ds:importing-source:edge-a",
            )
            .expect_err("provenance must reject importing source");

            for error in [query_error, neighborhood_error, provenance_error] {
                assert!(matches!(
                    error,
                    graph_service::GraphServiceError::InvalidInput(_)
                ));
                assert!(error.to_string().contains("not ready"));
            }
            Ok(())
        })
        .expect("validate scoped graph readiness boundary");
}
