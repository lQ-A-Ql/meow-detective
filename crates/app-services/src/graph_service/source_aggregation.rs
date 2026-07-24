use std::{collections::HashMap, path::Path};

use domain::{CaseId, DataSourceId};
use rusqlite::Connection;
use transport::dto::{
    GraphEdgeDto, GraphNodeDto, GraphProvenanceEntryDto, GraphQueryDto, GraphQueryResultDto,
    GraphSnapshotDto,
};

use crate::source_db::{self, encode_source_scoped_id};

use super::{
    query::{get_node_neighborhood, get_provenance_chain, query_graph},
    snapshot::{get_graph_snapshot, graph_density},
    GraphServiceError,
};

pub fn get_graph_snapshot_for_case(
    case_conn: &Connection,
    case_root: &Path,
    case_id: &str,
) -> Result<GraphSnapshotDto, GraphServiceError> {
    let mut merged = empty_snapshot();
    for (_, source_conn) in source_db::open_ready_source_connections_read_only(
        case_conn,
        case_root,
        &CaseId(case_id.to_string()),
    )? {
        merge_snapshot(&mut merged, get_graph_snapshot(&source_conn, case_id)?);
    }
    merged.density = graph_density(merged.total_nodes, merged.total_edges);
    Ok(merged)
}

pub fn query_graph_for_case(
    case_conn: &Connection,
    case_root: &Path,
    case_id: &str,
    query: GraphQueryDto,
) -> Result<GraphQueryResultDto, GraphServiceError> {
    if let Some((source_id, local_ids)) = scoped_start_ids(&query.start_ids)? {
        return query_scoped_source(case_conn, case_root, case_id, query, source_id, local_ids);
    }

    let mut merged = empty_query_result();
    for (source_id, source_conn) in source_db::open_ready_source_connections_read_only(
        case_conn,
        case_root,
        &CaseId(case_id.to_string()),
    )? {
        let source_result =
            scope_graph_result(query_graph(&source_conn, query.clone())?, &source_id);
        append_graph_result(&mut merged, source_result);
    }
    sort_graph_result(&mut merged);
    Ok(merged)
}

pub fn get_node_neighborhood_for_case(
    case_conn: &Connection,
    case_root: &Path,
    case_id: &str,
    node_id: &str,
    depth: u32,
) -> Result<GraphQueryResultDto, GraphServiceError> {
    let (source_id, local_id) = parse_scoped_id("Graph node id", node_id, "graph nodes")?;
    let source = source_db::open_ready_source_read_only_by_id(
        case_conn,
        case_root,
        &CaseId(case_id.to_string()),
        &source_id,
    )?;
    Ok(scope_graph_result(
        get_node_neighborhood(&source.connection, &local_id, depth)?,
        &source_id,
    ))
}

pub fn get_provenance_chain_for_case(
    case_conn: &Connection,
    case_root: &Path,
    case_id: &str,
    edge_id: &str,
) -> Result<Vec<GraphProvenanceEntryDto>, GraphServiceError> {
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

pub(crate) fn scoped_start_ids(
    ids: &[String],
) -> Result<Option<(DataSourceId, Vec<String>)>, GraphServiceError> {
    let mut source = None;
    let mut local_ids = Vec::with_capacity(ids.len());
    for id in ids {
        let (candidate, local_id) = parse_scoped_id("Graph start id", id, "graph query startIds")?;
        if source.as_ref().is_some_and(|current| current != &candidate) {
            return Err(GraphServiceError::InvalidInput(
                "graph query startIds cannot mix data sources".to_string(),
            ));
        }
        source.get_or_insert(candidate);
        local_ids.push(local_id);
    }
    Ok(source.map(|source_id| (source_id, local_ids)))
}

pub(super) fn scope_graph_node(
    mut node: GraphNodeDto,
    data_source_id: &DataSourceId,
) -> GraphNodeDto {
    node.id = encode_source_scoped_id(data_source_id, &node.id);
    node
}

fn query_scoped_source(
    case_conn: &Connection,
    case_root: &Path,
    case_id: &str,
    mut query: GraphQueryDto,
    source_id: DataSourceId,
    local_ids: Vec<String>,
) -> Result<GraphQueryResultDto, GraphServiceError> {
    query.start_ids = local_ids;
    let source = source_db::open_ready_source_read_only_by_id(
        case_conn,
        case_root,
        &CaseId(case_id.to_string()),
        &source_id,
    )?;
    Ok(scope_graph_result(
        query_graph(&source.connection, query)?,
        &source_id,
    ))
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

fn scope_graph_result(
    mut result: GraphQueryResultDto,
    source_id: &DataSourceId,
) -> GraphQueryResultDto {
    result.nodes = result
        .nodes
        .into_iter()
        .map(|node| scope_graph_node(node, source_id))
        .collect();
    result.edges = result
        .edges
        .into_iter()
        .map(|edge| scope_graph_edge(edge, source_id))
        .collect();
    result.node_count = result.nodes.len() as u32;
    result.edge_count = result.edges.len() as u32;
    result
}

fn scope_graph_edge(mut edge: GraphEdgeDto, source_id: &DataSourceId) -> GraphEdgeDto {
    edge.id = encode_source_scoped_id(source_id, &edge.id);
    edge.source_id = encode_source_scoped_id(source_id, &edge.source_id);
    edge.target_id = encode_source_scoped_id(source_id, &edge.target_id);
    edge
}

fn append_graph_result(target: &mut GraphQueryResultDto, source: GraphQueryResultDto) {
    target.nodes.extend(source.nodes);
    target.edges.extend(source.edges);
    target.node_count = target.nodes.len() as u32;
    target.edge_count = target.edges.len() as u32;
}

fn sort_graph_result(result: &mut GraphQueryResultDto) {
    result.nodes.sort_by(|left, right| left.id.cmp(&right.id));
    result.edges.sort_by(|left, right| left.id.cmp(&right.id));
}

fn merge_snapshot(target: &mut GraphSnapshotDto, source: GraphSnapshotDto) {
    merge_counts(&mut target.node_count_by_type, source.node_count_by_type);
    merge_counts(&mut target.edge_count_by_type, source.edge_count_by_type);
    target.total_nodes += source.total_nodes;
    target.total_edges += source.total_edges;
    target.largest_component_size = target
        .largest_component_size
        .max(source.largest_component_size);
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
    }
}

fn empty_query_result() -> GraphQueryResultDto {
    GraphQueryResultDto {
        nodes: Vec::new(),
        edges: Vec::new(),
        node_count: 0,
        edge_count: 0,
    }
}
