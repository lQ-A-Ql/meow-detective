use std::collections::{HashMap, HashSet, VecDeque};

use domain::{EdgeType, GraphEdge, GraphNode, NodeType};
use persistence_sqlite::repositories::{
    artifact_repo::ArtifactRepo,
    graph_repo::{Direction, GraphRepo},
};
use rusqlite::Connection;
use transport::dto::{
    GraphEdgeDto, GraphEdgeTypeDto, GraphNodeDto, GraphNodeTypeDto, GraphProvenanceEntryDto,
    GraphQueryDto, GraphQueryResultDto,
};

use super::GraphServiceError;

pub fn query_graph(
    conn: &Connection,
    query: GraphQueryDto,
) -> Result<GraphQueryResultDto, GraphServiceError> {
    let edge_types = query
        .edge_types
        .iter()
        .map(|edge_type| parse_edge_type(edge_type))
        .collect::<Vec<_>>();
    let (nodes, edges) = GraphRepo::new(conn)
        .traverse(&query.start_ids, &edge_types, query.max_depth, query.limit)
        .map_err(|error| GraphServiceError::Other(format!("graph traversal: {error}")))?;
    let edges = filter_edges_by_confidence(edges, query.confidence_floor);
    Ok(to_query_result(nodes, edges))
}

pub fn get_node_neighborhood(
    conn: &Connection,
    node_id: &str,
    depth: u32,
) -> Result<GraphQueryResultDto, GraphServiceError> {
    let repo = GraphRepo::new(conn);
    let Some(center) = repo
        .get_node(node_id)
        .map_err(|error| GraphServiceError::Other(format!("get center node: {error}")))?
    else {
        return Ok(empty_query_result());
    };

    let mut state = NeighborhoodState::new(center, node_id);
    while let Some((current_id, current_depth)) = state.queue.pop_front() {
        if current_depth >= depth.max(1) {
            continue;
        }
        let neighbors = repo
            .get_neighbors(&current_id, &[], Direction::Both)
            .map_err(|error| GraphServiceError::Other(format!("get neighbors: {error}")))?;
        state.add_neighbors(neighbors, current_depth);
    }
    Ok(to_query_result(
        state.nodes.into_values().collect(),
        state.edges,
    ))
}

pub fn get_provenance_chain(
    conn: &Connection,
    edge_id: &str,
) -> Result<Vec<GraphProvenanceEntryDto>, GraphServiceError> {
    let edge = GraphRepo::new(conn)
        .find_edge_by_id(edge_id)
        .map_err(|error| GraphServiceError::Other(format!("edge query: {error}")))?
        .ok_or_else(|| GraphServiceError::NotFound(format!("edge not found: {edge_id}")))?;
    let mut entries = provenance_entries(&edge);
    enrich_parser_versions(conn, &mut entries);
    Ok(entries)
}

struct NeighborhoodState {
    visited_nodes: HashSet<String>,
    visited_edges: HashSet<String>,
    nodes: HashMap<String, GraphNode>,
    edges: Vec<GraphEdge>,
    queue: VecDeque<(String, u32)>,
}

impl NeighborhoodState {
    fn new(center: GraphNode, node_id: &str) -> Self {
        let mut visited_nodes = HashSet::new();
        visited_nodes.insert(center.id.clone());
        let mut nodes = HashMap::new();
        nodes.insert(center.id.clone(), center);
        let mut queue = VecDeque::new();
        queue.push_back((node_id.to_string(), 0));
        Self {
            visited_nodes,
            visited_edges: HashSet::new(),
            nodes,
            edges: Vec::new(),
            queue,
        }
    }

    fn add_neighbors(&mut self, neighbors: Vec<(GraphEdge, GraphNode)>, depth: u32) {
        for (edge, neighbor) in neighbors {
            if self.visited_edges.insert(edge.id.clone()) {
                self.edges.push(edge);
            }
            let neighbor_id = neighbor.id.clone();
            if self.visited_nodes.insert(neighbor_id.clone()) {
                self.nodes.insert(neighbor_id.clone(), neighbor);
                self.queue.push_back((neighbor_id, depth + 1));
            }
        }
    }
}

fn filter_edges_by_confidence(
    edges: Vec<GraphEdge>,
    confidence_floor: Option<f64>,
) -> Vec<GraphEdge> {
    let Some(floor) = confidence_floor else {
        return edges;
    };
    edges
        .into_iter()
        .filter(|edge| edge.confidence.unwrap_or(0.0) >= floor)
        .collect()
}

fn to_query_result(mut nodes: Vec<GraphNode>, mut edges: Vec<GraphEdge>) -> GraphQueryResultDto {
    nodes.sort_by(|left, right| left.id.cmp(&right.id));
    edges.sort_by(|left, right| left.id.cmp(&right.id));
    let nodes = nodes.into_iter().map(node_to_dto).collect::<Vec<_>>();
    let edges = edges.into_iter().map(edge_to_dto).collect::<Vec<_>>();
    GraphQueryResultDto {
        node_count: nodes.len() as u32,
        edge_count: edges.len() as u32,
        nodes,
        edges,
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

fn provenance_entries(edge: &GraphEdge) -> Vec<GraphProvenanceEntryDto> {
    let provenance = edge
        .provenance
        .as_deref()
        .and_then(|raw| serde_json::from_str::<serde_json::Value>(raw).ok());
    match provenance {
        Some(value) => entries_from_metadata(edge, &value),
        None => vec![base_provenance_entry(edge, None, None)],
    }
}

fn entries_from_metadata(
    edge: &GraphEdge,
    provenance: &serde_json::Value,
) -> Vec<GraphProvenanceEntryDto> {
    let signals = string_array(provenance, "match_signals");
    let families = string_array(provenance, "families");
    let lead_id = provenance
        .get("lead_id")
        .and_then(serde_json::Value::as_str);
    let parser = families.first().map(String::as_str);

    if signals.is_empty() {
        return vec![base_provenance_entry(edge, lead_id, parser)];
    }
    signals
        .iter()
        .enumerate()
        .map(|(index, _)| {
            let rule = format!("{}-{index}", lead_id.unwrap_or("rule"));
            base_provenance_entry(edge, Some(&rule), (index == 0).then_some(parser).flatten())
        })
        .collect()
}

fn string_array(value: &serde_json::Value, key: &str) -> Vec<String> {
    value
        .get(key)
        .and_then(serde_json::Value::as_array)
        .map(|values| {
            values
                .iter()
                .filter_map(serde_json::Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

fn base_provenance_entry(
    edge: &GraphEdge,
    source_rule_id: Option<&str>,
    source_parser: Option<&str>,
) -> GraphProvenanceEntryDto {
    GraphProvenanceEntryDto {
        edge_id: edge.id.clone(),
        source_rule_id: source_rule_id.map(str::to_string),
        source_parser: source_parser.map(str::to_string),
        extraction_timestamp: Some(edge.created_at.clone()),
        parser_version: None,
    }
}

fn enrich_parser_versions(conn: &Connection, entries: &mut [GraphProvenanceEntryDto]) {
    let repo = ArtifactRepo::new(conn);
    for entry in entries {
        let Some(parser) = entry.source_parser.as_ref() else {
            continue;
        };
        if let Ok(versions) = repo.find_extractor_versions(parser) {
            if let Some((_, Some(version))) = versions.first() {
                entry.parser_version = Some(version.clone());
            }
        }
    }
}

pub(super) fn node_to_dto(node: GraphNode) -> GraphNodeDto {
    GraphNodeDto {
        id: node.id,
        case_id: node.case_id,
        node_type: node_type_to_dto(&node.node_type),
        label: node.label,
        summary: node.summary,
        tags: node.tags,
        created_at: node.created_at,
    }
}

pub(super) fn edge_to_dto(edge: GraphEdge) -> GraphEdgeDto {
    GraphEdgeDto {
        id: edge.id,
        case_id: edge.case_id,
        source_id: edge.source_id,
        target_id: edge.target_id,
        edge_type: edge_type_to_dto(&edge.edge_type),
        confidence: edge.confidence,
        provenance: edge.provenance,
        created_at: edge.created_at,
    }
}

fn node_type_to_dto(node_type: &NodeType) -> GraphNodeTypeDto {
    match node_type {
        NodeType::File => GraphNodeTypeDto::File,
        NodeType::Artifact => GraphNodeTypeDto::Artifact,
        NodeType::TimelineEvent => GraphNodeTypeDto::TimelineEvent,
        NodeType::Entity => GraphNodeTypeDto::Entity,
        NodeType::Lead => GraphNodeTypeDto::Lead,
        NodeType::NotebookEntry => GraphNodeTypeDto::NotebookEntry,
    }
}

fn edge_type_to_dto(edge_type: &EdgeType) -> GraphEdgeTypeDto {
    match edge_type {
        EdgeType::Contains => GraphEdgeTypeDto::Contains,
        EdgeType::References => GraphEdgeTypeDto::References,
        EdgeType::CorrelatesWith => GraphEdgeTypeDto::CorrelatesWith,
        EdgeType::DerivesFrom => GraphEdgeTypeDto::DerivesFrom,
        EdgeType::Precedes => GraphEdgeTypeDto::Precedes,
        EdgeType::Cites => GraphEdgeTypeDto::Cites,
        EdgeType::Annotates => GraphEdgeTypeDto::Annotates,
    }
}

fn parse_edge_type(value: &str) -> EdgeType {
    match value {
        "contains" => EdgeType::Contains,
        "references" => EdgeType::References,
        "correlates_with" => EdgeType::CorrelatesWith,
        "derives_from" => EdgeType::DerivesFrom,
        "precedes" => EdgeType::Precedes,
        "cites" => EdgeType::Cites,
        "annotates" => EdgeType::Annotates,
        _ => EdgeType::References,
    }
}
