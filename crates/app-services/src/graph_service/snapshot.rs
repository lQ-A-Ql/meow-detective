use persistence_sqlite::repositories::graph_repo::GraphRepo;
use rusqlite::Connection;
use transport::dto::GraphSnapshotDto;

use super::GraphServiceError;

pub fn get_graph_snapshot(
    conn: &Connection,
    case_id: &str,
) -> Result<GraphSnapshotDto, GraphServiceError> {
    let snapshot = GraphRepo::new(conn)
        .get_snapshot(case_id)
        .map_err(|error| GraphServiceError::Other(format!("graph snapshot query: {error}")))?;
    let density = graph_density(snapshot.total_nodes, snapshot.total_edges);
    Ok(GraphSnapshotDto {
        node_count_by_type: snapshot.node_count_by_type,
        edge_count_by_type: snapshot.edge_count_by_type,
        total_nodes: snapshot.total_nodes,
        total_edges: snapshot.total_edges,
        density,
        largest_component_size: 0,
        data_source_count: 0,
        cross_source_entity_count: 0,
        cross_source_edge_count: 0,
        seed_ids: Vec::new(),
        projection_built_at: None,
    })
}

pub(super) fn graph_density(total_nodes: u64, total_edges: u64) -> f64 {
    if total_nodes <= 1 {
        return 0.0;
    }
    (2 * total_edges) as f64 / (total_nodes * (total_nodes - 1)) as f64
}
