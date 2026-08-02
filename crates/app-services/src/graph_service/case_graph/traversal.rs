use std::collections::{BTreeMap, BTreeSet, HashSet, VecDeque};
use std::path::Path;

use domain::{DataSourceId, EdgeType, GraphEdge, GraphNode};
use persistence_sqlite::repositories::graph_repo::{Direction, GraphNeighborPage, GraphRepo};
use rusqlite::Connection;
use transport::dto::{GraphQueryDto, GraphQueryResultDto};

use crate::source_db;

use super::projection::ensure_case_graph;
use crate::graph_service::{
    query::{edge_to_dto, node_to_dto},
    GraphServiceError,
};

const MAX_START_IDS: usize = 64;
const MAX_DEPTH: u32 = 5;
const MAX_NODE_LIMIT: u32 = 500;
const MAX_EDGE_LIMIT: u32 = 2_000;

pub(crate) fn query_case_graph(
    case_conn: &Connection,
    case_root: &Path,
    case_id: &str,
    query: GraphQueryDto,
) -> Result<GraphQueryResultDto, GraphServiceError> {
    let settings = QuerySettings::validate(query)?;
    let overlay = ensure_case_graph(case_conn, case_root, case_id)?;
    let source_connections = source_db::open_ready_source_connections_read_only(
        case_conn,
        case_root,
        &domain::CaseId(case_id.to_string()),
    )?
    .into_iter()
    .map(|(source_id, connection)| (source_id.0, connection))
    .collect();
    let reader = HybridGraphReader {
        overlay: &overlay.connection,
        sources: source_connections,
    };
    traverse(&reader, settings)
}

struct QuerySettings {
    start_ids: Vec<String>,
    edge_types: Vec<EdgeType>,
    max_depth: u32,
    confidence_floor: Option<f64>,
    node_limit: u32,
    edge_limit: u32,
}

impl QuerySettings {
    fn validate(query: GraphQueryDto) -> Result<Self, GraphServiceError> {
        if query.start_ids.is_empty() {
            return Ok(Self {
                start_ids: Vec::new(),
                edge_types: parse_edge_types(&query.edge_types)?,
                max_depth: query.max_depth.min(MAX_DEPTH),
                confidence_floor: validate_confidence(query.confidence_floor)?,
                node_limit: query.limit.clamp(1, MAX_NODE_LIMIT),
                edge_limit: query.edge_limit.clamp(1, MAX_EDGE_LIMIT),
            });
        }
        if query.start_ids.len() > MAX_START_IDS {
            return Err(GraphServiceError::InvalidInput(format!(
                "graph query accepts at most {MAX_START_IDS} startIds"
            )));
        }
        if query.max_depth == 0 || query.max_depth > MAX_DEPTH {
            return Err(GraphServiceError::InvalidInput(format!(
                "graph maxDepth must be between 1 and {MAX_DEPTH}"
            )));
        }
        if query.limit == 0 || query.limit > MAX_NODE_LIMIT {
            return Err(GraphServiceError::InvalidInput(format!(
                "graph node limit must be between 1 and {MAX_NODE_LIMIT}"
            )));
        }
        if query.edge_limit == 0 || query.edge_limit > MAX_EDGE_LIMIT {
            return Err(GraphServiceError::InvalidInput(format!(
                "graph edgeLimit must be between 1 and {MAX_EDGE_LIMIT}"
            )));
        }
        let mut start_ids = query.start_ids;
        start_ids.sort();
        start_ids.dedup();
        Ok(Self {
            start_ids,
            edge_types: parse_edge_types(&query.edge_types)?,
            max_depth: query.max_depth,
            confidence_floor: validate_confidence(query.confidence_floor)?,
            node_limit: query.limit,
            edge_limit: query.edge_limit,
        })
    }
}

struct HybridGraphReader<'a> {
    overlay: &'a Connection,
    sources: BTreeMap<String, Connection>,
}

impl HybridGraphReader<'_> {
    fn get_node(&self, node_id: &str) -> Result<Option<GraphNode>, GraphServiceError> {
        if node_id.starts_with("case:entity:") {
            return GraphRepo::new(self.overlay)
                .get_node(node_id)
                .map_err(Into::into);
        }
        let (source_id, local_id) = parse_scoped_node_id(node_id)?;
        let connection = self.source_connection(&source_id)?;
        Ok(GraphRepo::new(connection)
            .get_node(&local_id)?
            .map(|node| scope_node(node, &source_id)))
    }

    fn get_neighbors(
        &self,
        node_id: &str,
        edge_types: &[EdgeType],
        confidence_floor: Option<f64>,
        limit: u32,
    ) -> Result<GraphNeighborPage, GraphServiceError> {
        let mut truncated = false;
        let mut neighbors = Vec::new();
        if node_id.starts_with("case:entity:") {
            let page = GraphRepo::new(self.overlay).get_neighbors_bounded(
                node_id,
                edge_types,
                Direction::Both,
                confidence_floor,
                limit,
            )?;
            return Ok(page);
        }

        let (source_id, local_id) = parse_scoped_node_id(node_id)?;
        let local_page = GraphRepo::new(self.source_connection(&source_id)?)
            .get_neighbors_bounded(
                &local_id,
                edge_types,
                Direction::Both,
                confidence_floor,
                limit,
            )?;
        truncated |= local_page.truncated;
        neighbors.extend(
            local_page
                .neighbors
                .into_iter()
                .map(|(edge, node)| (scope_edge(edge, &source_id), scope_node(node, &source_id))),
        );

        let overlay_page = GraphRepo::new(self.overlay).get_neighbors_bounded(
            node_id,
            edge_types,
            Direction::Both,
            confidence_floor,
            limit,
        )?;
        truncated |= overlay_page.truncated;
        neighbors.extend(overlay_page.neighbors);
        neighbors.sort_by(|left, right| left.0.id.cmp(&right.0.id));
        neighbors.dedup_by(|left, right| left.0.id == right.0.id);
        if neighbors.len() > limit as usize {
            neighbors.truncate(limit as usize);
            truncated = true;
        }
        Ok(GraphNeighborPage {
            neighbors,
            truncated,
        })
    }

    fn source_connection(
        &self,
        source_id: &DataSourceId,
    ) -> Result<&Connection, GraphServiceError> {
        self.sources.get(&source_id.0).ok_or_else(|| {
            GraphServiceError::InvalidInput(format!(
                "graph node data source '{}' is not ready or unavailable",
                source_id.0
            ))
        })
    }
}

fn traverse(
    reader: &HybridGraphReader<'_>,
    settings: QuerySettings,
) -> Result<GraphQueryResultDto, GraphServiceError> {
    let mut state = TraversalState::default();
    for start_id in &settings.start_ids {
        let Some(node) = reader.get_node(start_id)? else {
            continue;
        };
        if state.nodes.len() >= settings.node_limit as usize {
            state.truncated = true;
            break;
        }
        if state.visited_nodes.insert(node.id.clone()) {
            state.queue.push_back((node.id.clone(), 0));
            state.nodes.insert(node.id.clone(), node);
        }
    }
    while let Some((node_id, depth)) = state.queue.pop_front() {
        state.max_depth_reached = state.max_depth_reached.max(depth);
        if depth >= settings.max_depth {
            continue;
        }
        let remaining_edges = settings.edge_limit.saturating_sub(state.edges.len() as u32);
        if remaining_edges == 0 {
            state.truncated = true;
            break;
        }
        let page = reader.get_neighbors(
            &node_id,
            &settings.edge_types,
            settings.confidence_floor,
            remaining_edges,
        )?;
        state.truncated |= page.truncated;
        for (edge, neighbor) in page.neighbors {
            let is_new_node = !state.visited_nodes.contains(&neighbor.id);
            if is_new_node && state.nodes.len() >= settings.node_limit as usize {
                state.truncated = true;
                continue;
            }
            if is_new_node {
                state.visited_nodes.insert(neighbor.id.clone());
                state.nodes.insert(neighbor.id.clone(), neighbor.clone());
                state.queue.push_back((neighbor.id.clone(), depth + 1));
                state.max_depth_reached = state.max_depth_reached.max(depth + 1);
            }
            if state.visited_edges.insert(edge.id.clone()) {
                state.edges.insert(edge.id.clone(), edge);
            }
        }
    }
    Ok(state.into_result())
}

#[derive(Default)]
struct TraversalState {
    visited_nodes: HashSet<String>,
    visited_edges: HashSet<String>,
    nodes: BTreeMap<String, GraphNode>,
    edges: BTreeMap<String, GraphEdge>,
    queue: VecDeque<(String, u32)>,
    truncated: bool,
    max_depth_reached: u32,
}

impl TraversalState {
    fn into_result(self) -> GraphQueryResultDto {
        let data_source_ids = represented_sources(self.nodes.keys());
        let nodes = self
            .nodes
            .into_values()
            .map(node_to_dto)
            .collect::<Vec<_>>();
        let edges = self
            .edges
            .into_values()
            .map(edge_to_dto)
            .collect::<Vec<_>>();
        GraphQueryResultDto {
            node_count: nodes.len() as u32,
            edge_count: edges.len() as u32,
            nodes,
            edges,
            truncated: self.truncated,
            max_depth_reached: self.max_depth_reached,
            data_source_ids,
        }
    }
}

fn represented_sources<'a>(node_ids: impl Iterator<Item = &'a String>) -> Vec<String> {
    node_ids
        .filter_map(|node_id| {
            source_db::parse_source_scoped_id("Graph node id", node_id)
                .ok()
                .map(|(source_id, _)| source_id.0)
        })
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn parse_scoped_node_id(node_id: &str) -> Result<(DataSourceId, String), GraphServiceError> {
    source_db::parse_source_scoped_id("Graph node id", node_id).map_err(|error| {
        GraphServiceError::InvalidInput(format!("{error}. Expected ds:<dataSourceId>:<localId>"))
    })
}

fn parse_edge_types(values: &[String]) -> Result<Vec<EdgeType>, GraphServiceError> {
    values
        .iter()
        .map(|value| match value.as_str() {
            "contains" => Ok(EdgeType::Contains),
            "references" => Ok(EdgeType::References),
            "correlatesWith" | "correlates_with" => Ok(EdgeType::CorrelatesWith),
            "derivesFrom" | "derives_from" => Ok(EdgeType::DerivesFrom),
            "precedes" => Ok(EdgeType::Precedes),
            "cites" => Ok(EdgeType::Cites),
            "annotates" => Ok(EdgeType::Annotates),
            _ => Err(GraphServiceError::InvalidInput(format!(
                "unsupported graph edge type '{value}'"
            ))),
        })
        .collect()
}

fn validate_confidence(value: Option<f64>) -> Result<Option<f64>, GraphServiceError> {
    match value {
        Some(value) if !value.is_finite() || !(0.0..=1.0).contains(&value) => {
            Err(GraphServiceError::InvalidInput(
                "graph confidenceFloor must be between 0 and 1".to_string(),
            ))
        }
        value => Ok(value),
    }
}

fn scope_node(mut node: GraphNode, source_id: &DataSourceId) -> GraphNode {
    node.id = source_db::encode_source_scoped_id(source_id, &node.id);
    node
}

fn scope_edge(mut edge: GraphEdge, source_id: &DataSourceId) -> GraphEdge {
    edge.id = source_db::encode_source_scoped_id(source_id, &edge.id);
    edge.source_id = source_db::encode_source_scoped_id(source_id, &edge.source_id);
    edge.target_id = source_db::encode_source_scoped_id(source_id, &edge.target_id);
    edge
}
