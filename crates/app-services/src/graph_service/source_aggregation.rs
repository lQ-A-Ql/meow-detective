use std::{collections::HashMap, path::Path};

use domain::{CaseId, DataSourceId};
use rusqlite::Connection;
use transport::dto::{
    GraphNodeDto, GraphProvenanceEntryDto, GraphQueryDto, GraphQueryResultDto, GraphSnapshotDto,
};

use crate::source_db::{self, encode_source_scoped_id};

use super::{
    case_graph::{ensure_case_graph, query_case_graph},
    query::get_provenance_chain,
    snapshot::{get_graph_snapshot, graph_density},
    GraphServiceError,
};

pub fn get_graph_snapshot_for_case(
    case_conn: &Connection,
    case_root: &Path,
    case_id: &str,
) -> Result<GraphSnapshotDto, GraphServiceError> {
    let case_graph = ensure_case_graph(case_conn, case_root, case_id)?;
    let mut merged = empty_snapshot();
    for (_, source_conn) in source_db::open_ready_source_connections_read_only(
        case_conn,
        case_root,
        &CaseId(case_id.to_string()),
    )? {
        merge_snapshot(&mut merged, get_graph_snapshot(&source_conn, case_id)?);
    }
    let projection = case_graph.projection;
    merged.total_nodes = merged
        .total_nodes
        .saturating_add(projection.cross_source_entity_count);
    merged.total_edges = merged
        .total_edges
        .saturating_add(projection.cross_source_edge_count);
    *merged
        .node_count_by_type
        .entry("entity".to_string())
        .or_default() += projection.cross_source_entity_count;
    *merged
        .edge_count_by_type
        .entry("correlates_with".to_string())
        .or_default() += projection.cross_source_edge_count;
    merged.density = graph_density(merged.total_nodes, merged.total_edges);
    merged.data_source_count = projection.source_count;
    merged.cross_source_entity_count = projection.cross_source_entity_count;
    merged.cross_source_edge_count = projection.cross_source_edge_count;
    merged.seed_ids = projection.seed_ids;
    merged.projection_built_at = Some(projection.built_at);
    Ok(merged)
}

pub fn query_graph_for_case(
    case_conn: &Connection,
    case_root: &Path,
    case_id: &str,
    query: GraphQueryDto,
) -> Result<GraphQueryResultDto, GraphServiceError> {
    query_case_graph(case_conn, case_root, case_id, query)
}

pub fn get_node_neighborhood_for_case(
    case_conn: &Connection,
    case_root: &Path,
    case_id: &str,
    node_id: &str,
    depth: u32,
) -> Result<GraphQueryResultDto, GraphServiceError> {
    query_case_graph(
        case_conn,
        case_root,
        case_id,
        GraphQueryDto {
            start_ids: vec![node_id.to_string()],
            edge_types: Vec::new(),
            max_depth: depth,
            confidence_floor: None,
            limit: 200,
            edge_limit: 600,
        },
    )
}

pub fn get_provenance_chain_for_case(
    case_conn: &Connection,
    case_root: &Path,
    case_id: &str,
    edge_id: &str,
) -> Result<Vec<GraphProvenanceEntryDto>, GraphServiceError> {
    if edge_id.starts_with("case:edge:") {
        let case_graph = ensure_case_graph(case_conn, case_root, case_id)?;
        return get_provenance_chain(&case_graph.connection, edge_id);
    }
    let (source_id, local_id) = parse_scoped_id("Graph edge id", edge_id, "graph edges")?;
    let source = source_db::open_ready_source_read_only_by_id(
        case_conn,
        case_root,
        &CaseId(case_id.to_string()),
        &source_id,
    )?;
    let mut entries = get_provenance_chain(&source.connection, &local_id)?;
    for entry in &mut entries {
        entry.edge_id = encode_source_scoped_id(&source_id, &entry.edge_id);
    }
    Ok(entries)
}

pub(crate) fn scope_graph_node(
    mut node: GraphNodeDto,
    data_source_id: &DataSourceId,
) -> GraphNodeDto {
    node.id = encode_source_scoped_id(data_source_id, &node.id);
    node
}

fn parse_scoped_id(
    label: &str,
    value: &str,
    subject: &str,
) -> Result<(DataSourceId, String), GraphServiceError> {
    source_db::parse_source_scoped_id(label, value).map_err(|error| {
        GraphServiceError::InvalidInput(format!(
            "{error}; source database {subject} require ds:<dataSourceId>:<localId>"
        ))
    })
}

fn merge_snapshot(target: &mut GraphSnapshotDto, source: GraphSnapshotDto) {
    merge_counts(&mut target.node_count_by_type, source.node_count_by_type);
    merge_counts(&mut target.edge_count_by_type, source.edge_count_by_type);
    target.total_nodes = target.total_nodes.saturating_add(source.total_nodes);
    target.total_edges = target.total_edges.saturating_add(source.total_edges);
}

fn merge_counts(target: &mut HashMap<String, u64>, source: HashMap<String, u64>) {
    for (key, count) in source {
        *target.entry(key).or_default() += count;
    }
}

fn empty_snapshot() -> GraphSnapshotDto {
    GraphSnapshotDto {
        node_count_by_type: HashMap::new(),
        edge_count_by_type: HashMap::new(),
        total_nodes: 0,
        total_edges: 0,
        density: 0.0,
        largest_component_size: 0,
        data_source_count: 0,
        cross_source_entity_count: 0,
        cross_source_edge_count: 0,
        seed_ids: Vec::new(),
        projection_built_at: None,
    }
}
