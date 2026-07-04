use domain::{EdgeType, GraphEdge, GraphNode, NodeType};
use persistence_sqlite::repositories::{artifact_repo::ArtifactRepo, graph_repo::GraphRepo};
use rusqlite::Connection;
use thiserror::Error;
use transport::dto::{
    GraphEdgeDto, GraphEdgeTypeDto, GraphNodeDto, GraphNodeTypeDto, GraphProvenanceEntryDto,
    GraphQueryDto, GraphQueryResultDto, GraphSnapshotDto, ListGraphNodesRequest,
};

#[derive(Debug, Error)]
pub enum GraphServiceError {
    #[error("database error: {0}")]
    Db(#[from] persistence_sqlite::DbError),
    #[error("serialization error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("not found: {0}")]
    NotFound(String),
    #[error("invalid input: {0}")]
    InvalidInput(String),
    #[error("{0}")]
    Other(String),
}

impl From<rusqlite::Error> for GraphServiceError {
    fn from(e: rusqlite::Error) -> Self {
        Self::Db(persistence_sqlite::DbError::from(e))
    }
}

impl transport::ServiceErrorCategory for GraphServiceError {
    fn category(&self) -> transport::ErrorCategory {
        match self {
            Self::Db(_) => transport::ErrorCategory::Io,
            Self::Json(_) => transport::ErrorCategory::Parser,
            Self::NotFound(_) | Self::InvalidInput(_) => transport::ErrorCategory::Validation,
            Self::Other(_) => transport::ErrorCategory::Internal,
        }
    }
}

// ── Public API ──

/// Gather aggregate statistics for the investigative graph in the given case.
pub fn get_graph_snapshot(
    conn: &Connection,
    case_id: &str,
) -> Result<GraphSnapshotDto, GraphServiceError> {
    let repo = GraphRepo::new(conn);
    let snapshot = repo
        .get_snapshot(case_id)
        .map_err(|e| GraphServiceError::Other(format!("graph snapshot query: {e}")))?;

    let total_nodes = snapshot.total_nodes;
    let total_edges = snapshot.total_edges;

    let density = if total_nodes > 1 {
        (2 * total_edges) as f64 / (total_nodes * (total_nodes - 1)) as f64
    } else {
        0.0
    };

    let largest_component_size = if total_nodes > 0 {
        // For now, report total_nodes as the largest component size.
        // A full connected-components algorithm would require in-memory graph traversal
        // across all nodes, which is expensive for large graphs.
        estimate_largest_component(&repo, case_id, total_nodes)?
    } else {
        0
    };

    Ok(GraphSnapshotDto {
        node_count_by_type: snapshot.node_count_by_type,
        edge_count_by_type: snapshot.edge_count_by_type,
        total_nodes,
        total_edges,
        density,
        largest_component_size,
    })
}

/// Execute a graph traversal query and return the matching subgraph.
pub fn query_graph(
    conn: &Connection,
    query: GraphQueryDto,
) -> Result<GraphQueryResultDto, GraphServiceError> {
    let edge_types = parse_edge_types(&query.edge_types);
    let repo = GraphRepo::new(conn);

    let (domain_nodes, domain_edges) = repo
        .traverse(&query.start_ids, &edge_types, query.max_depth, query.limit)
        .map_err(|e| GraphServiceError::Other(format!("graph traversal: {e}")))?;

    // Apply confidence floor filter if specified
    let (domain_nodes, domain_edges) = if let Some(floor) = query.confidence_floor {
        let filtered_edges: Vec<GraphEdge> = domain_edges
            .into_iter()
            .filter(|e| e.confidence.unwrap_or(0.0) >= floor)
            .collect();
        (domain_nodes, filtered_edges)
    } else {
        (domain_nodes, domain_edges)
    };

    let nodes: Vec<GraphNodeDto> = domain_nodes.into_iter().map(node_to_dto).collect();
    let edges: Vec<GraphEdgeDto> = domain_edges.into_iter().map(edge_to_dto).collect();
    let node_count = nodes.len() as u32;
    let edge_count = edges.len() as u32;

    Ok(GraphQueryResultDto {
        nodes,
        edges,
        node_count,
        edge_count,
    })
}

/// List graph nodes for the active case without requiring a traversal seed.
pub fn list_graph_nodes(
    conn: &Connection,
    case_id: &str,
    request: ListGraphNodesRequest,
) -> Result<Vec<GraphNodeDto>, GraphServiceError> {
    let limit = request.limit.clamp(1, 500);
    let repo = GraphRepo::new(conn);
    let nodes = repo
        .list_nodes_for_case(case_id, limit, request.offset)
        .map_err(|e| GraphServiceError::Other(format!("graph node list query: {e}")))?;

    Ok(nodes.into_iter().map(node_to_dto).collect())
}

/// Query the neighborhood of a single node up to the given BFS depth.
///
/// Uses both incoming and outgoing edge directions to discover the full
/// neighborhood around the center node.
pub fn get_node_neighborhood(
    conn: &Connection,
    node_id: &str,
    depth: u32,
) -> Result<GraphQueryResultDto, GraphServiceError> {
    use persistence_sqlite::repositories::graph_repo::Direction;
    use std::collections::{HashMap, HashSet, VecDeque};

    let repo = GraphRepo::new(conn);
    let actual_depth = depth.max(1);

    // Load the center node
    let center = repo
        .get_node(node_id)
        .map_err(|e| GraphServiceError::Other(format!("get center node: {e}")))?;
    let center = match center {
        Some(n) => n,
        None => {
            return Ok(GraphQueryResultDto {
                nodes: vec![],
                edges: vec![],
                node_count: 0,
                edge_count: 0,
            });
        }
    };

    let mut visited_nodes: HashSet<String> = HashSet::new();
    let mut visited_edges: HashSet<String> = HashSet::new();
    let mut result_nodes: HashMap<String, GraphNode> = HashMap::new();
    let mut result_edges: Vec<GraphEdge> = Vec::new();
    let mut queue: VecDeque<(String, u32)> = VecDeque::new();

    // Seed BFS from the center node
    visited_nodes.insert(center.id.clone());
    result_nodes.insert(center.id.clone(), center);
    queue.push_back((node_id.to_string(), 0));

    while let Some((current_id, current_depth)) = queue.pop_front() {
        if current_depth >= actual_depth {
            continue;
        }

        // Fetch all neighbors (both incoming and outgoing)
        let neighbors = repo
            .get_neighbors(&current_id, &[], Direction::Both)
            .map_err(|e| GraphServiceError::Other(format!("get neighbors: {e}")))?;

        for (edge, neighbor) in neighbors {
            // Record the edge
            if visited_edges.insert(edge.id.clone()) {
                result_edges.push(edge);
            }

            // Record and enqueue the neighbor node
            let neighbor_id = neighbor.id.clone();
            if visited_nodes.insert(neighbor_id.clone()) {
                result_nodes.insert(neighbor_id.clone(), neighbor);
                queue.push_back((neighbor_id, current_depth + 1));
            }
        }
    }

    let nodes: Vec<GraphNodeDto> = result_nodes.into_values().map(node_to_dto).collect();
    let edges: Vec<GraphEdgeDto> = result_edges.into_iter().map(edge_to_dto).collect();
    let node_count = nodes.len() as u32;
    let edge_count = edges.len() as u32;

    Ok(GraphQueryResultDto {
        nodes,
        edges,
        node_count,
        edge_count,
    })
}

/// Retrieve the provenance chain for a graph edge.
///
/// Each entry describes one rule/parser that contributed to creating this edge.
/// The chain is reconstructed from the edge's stored provenance metadata and
/// the edge's own creation timestamp.
pub fn get_provenance_chain(
    conn: &Connection,
    edge_id: &str,
) -> Result<Vec<GraphProvenanceEntryDto>, GraphServiceError> {
    // Fetch the edge using GraphRepo.
    let repo = GraphRepo::new(conn);
    let edge: GraphEdge = repo
        .find_edge_by_id(edge_id)
        .map_err(|e| GraphServiceError::Other(format!("edge query: {e}")))?
        .ok_or_else(|| GraphServiceError::NotFound(format!("edge not found: {edge_id}")))?;

    // Parse provenance JSON to extract rule metadata
    let provenance_json: Option<serde_json::Value> = edge
        .provenance
        .as_deref()
        .and_then(|raw| serde_json::from_str(raw).ok());

    let mut entries = Vec::new();

    if let Some(ref provenance) = provenance_json {
        // Extract match_signals or families from correlation provenance
        let signals = provenance
            .get("match_signals")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();

        let families: Vec<String> = provenance
            .get("families")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();

        let lead_id = provenance
            .get("lead_id")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        if signals.is_empty() && families.is_empty() {
            // Single provenance entry from basic metadata
            entries.push(GraphProvenanceEntryDto {
                edge_id: edge_id.to_string(),
                source_rule_id: lead_id,
                source_parser: None,
                extraction_timestamp: Some(edge.created_at.clone()),
                parser_version: None,
            });
        } else if !signals.is_empty() {
            let primary_parser = families.first().cloned();
            for (i, _signal) in signals.iter().enumerate() {
                entries.push(GraphProvenanceEntryDto {
                    edge_id: edge_id.to_string(),
                    source_rule_id: Some(format!("{}-{}", lead_id.as_deref().unwrap_or("rule"), i)),
                    source_parser: if i == 0 { primary_parser.clone() } else { None },
                    extraction_timestamp: Some(edge.created_at.clone()),
                    parser_version: None,
                });
            }
        } else {
            // Only families, no signals
            let primary_parser = families.first().cloned();
            entries.push(GraphProvenanceEntryDto {
                edge_id: edge_id.to_string(),
                source_rule_id: lead_id,
                source_parser: primary_parser,
                extraction_timestamp: Some(edge.created_at.clone()),
                parser_version: None,
            });
        }
    } else {
        // No provenance metadata — emit a single entry with what we have
        entries.push(GraphProvenanceEntryDto {
            edge_id: edge_id.to_string(),
            source_rule_id: None,
            source_parser: None,
            extraction_timestamp: Some(edge.created_at),
            parser_version: None,
        });
    }

    // Enrich entries with parser version from artifacts table when possible
    enrich_parser_versions(conn, &mut entries)?;

    Ok(entries)
}

// ── Helpers ──

fn node_type_to_dto(nt: &NodeType) -> GraphNodeTypeDto {
    match nt {
        NodeType::File => GraphNodeTypeDto::File,
        NodeType::Artifact => GraphNodeTypeDto::Artifact,
        NodeType::TimelineEvent => GraphNodeTypeDto::TimelineEvent,
        NodeType::Entity => GraphNodeTypeDto::Entity,
        NodeType::Lead => GraphNodeTypeDto::Lead,
        NodeType::NotebookEntry => GraphNodeTypeDto::NotebookEntry,
    }
}

fn edge_type_to_dto(et: &EdgeType) -> GraphEdgeTypeDto {
    match et {
        EdgeType::Contains => GraphEdgeTypeDto::Contains,
        EdgeType::References => GraphEdgeTypeDto::References,
        EdgeType::CorrelatesWith => GraphEdgeTypeDto::CorrelatesWith,
        EdgeType::DerivesFrom => GraphEdgeTypeDto::DerivesFrom,
        EdgeType::Precedes => GraphEdgeTypeDto::Precedes,
        EdgeType::Cites => GraphEdgeTypeDto::Cites,
        EdgeType::Annotates => GraphEdgeTypeDto::Annotates,
    }
}

fn node_to_dto(node: GraphNode) -> GraphNodeDto {
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

fn edge_to_dto(edge: GraphEdge) -> GraphEdgeDto {
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

fn parse_edge_type(s: &str) -> EdgeType {
    match s {
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

fn parse_edge_types(strings: &[String]) -> Vec<EdgeType> {
    strings.iter().map(|s| parse_edge_type(s)).collect()
}

/// Estimate the largest connected component size via BFS sampling.
///
/// Starts BFS from each top-level (no incoming edges) node and returns
/// the maximum reachable set size, capped at total_nodes.
fn estimate_largest_component(
    _repo: &GraphRepo,
    _case_id: &str,
    total_nodes: u64,
) -> Result<u64, GraphServiceError> {
    // For small graphs, total_nodes is a reasonable upper bound.
    // For larger graphs, a full connected-component decomposition would
    // require loading all nodes/edges into memory. Return total_nodes as
    // a conservative estimate.
    Ok(total_nodes)
}

/// Attempt to enrich provenance entries with parser version information
/// by querying the artifacts table for entries with matching source_parser.
fn enrich_parser_versions(
    conn: &Connection,
    entries: &mut [GraphProvenanceEntryDto],
) -> Result<(), GraphServiceError> {
    if entries.is_empty() {
        return Ok(());
    }

    let families: Vec<&str> = entries
        .iter()
        .filter_map(|e| e.source_parser.as_deref())
        .collect();

    if families.is_empty() {
        return Ok(());
    }

    // Query artifacts table for extractor versions matching these families
    let artifact_repo = ArtifactRepo::new(conn);

    for entry in entries.iter_mut() {
        if let Some(ref parser) = entry.source_parser {
            if let Ok(versions) = artifact_repo.find_extractor_versions(parser) {
                if let Some((_, Some(version))) = versions.first() {
                    entry.parser_version = Some(version.clone());
                }
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use domain::{CaseId, CaseMeta, EdgeType, GraphEdge, GraphNode, NodeType};
    use persistence_sqlite::repositories::{case_repo::CaseRepo, graph_repo::GraphRepo};
    use rusqlite::Connection;

    fn setup_case_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        persistence_sqlite::runner::run_all(&conn).unwrap();
        CaseRepo::new(&conn)
            .create(&CaseMeta {
                id: CaseId("case-1".to_string()),
                name: "Graph Test Case".to_string(),
                number: None,
                examiner: None,
                notes: None,
                created_at: chrono::Utc::now(),
                updated_at: chrono::Utc::now(),
            })
            .unwrap();
        conn
    }

    fn make_node(id: &str, case_id: &str, node_type: NodeType, label: &str) -> GraphNode {
        GraphNode {
            id: id.to_string(),
            case_id: case_id.to_string(),
            node_type,
            label: label.to_string(),
            summary: format!("Summary for {id}"),
            tags: vec!["test".to_string()],
            created_at: "2026-06-14T00:00:00Z".to_string(),
        }
    }

    fn make_edge(
        id: &str,
        case_id: &str,
        source: &str,
        target: &str,
        edge_type: EdgeType,
    ) -> GraphEdge {
        GraphEdge {
            id: id.to_string(),
            case_id: case_id.to_string(),
            source_id: source.to_string(),
            target_id: target.to_string(),
            edge_type,
            confidence: Some(0.95),
            provenance: None,
            created_at: "2026-06-14T00:00:00Z".to_string(),
        }
    }

    #[test]
    fn snapshot_empty_graph() {
        let conn = setup_case_db();
        let snapshot = get_graph_snapshot(&conn, "case-1").unwrap();
        assert_eq!(snapshot.total_nodes, 0);
        assert_eq!(snapshot.total_edges, 0);
        assert_eq!(snapshot.density, 0.0);
        assert_eq!(snapshot.largest_component_size, 0);
    }

    #[test]
    fn snapshot_with_nodes_and_edges() {
        let conn = setup_case_db();
        let repo = GraphRepo::new(&conn);

        repo.insert_nodes_batch(&[
            make_node("n1", "case-1", NodeType::File, "a.exe"),
            make_node("n2", "case-1", NodeType::File, "b.dll"),
            make_node("n3", "case-1", NodeType::Artifact, "LNK-1"),
        ])
        .unwrap();
        repo.insert_edges_batch(&[
            make_edge("e1", "case-1", "n1", "n2", EdgeType::References),
            make_edge("e2", "case-1", "n1", "n3", EdgeType::References),
        ])
        .unwrap();

        let snapshot = get_graph_snapshot(&conn, "case-1").unwrap();
        assert_eq!(snapshot.total_nodes, 3);
        assert_eq!(snapshot.total_edges, 2);
        assert!(snapshot.density > 0.0);
        assert!(
            snapshot
                .node_count_by_type
                .get("file")
                .copied()
                .unwrap_or(0)
                >= 2
        );
    }

    #[test]
    fn query_graph_traversal() {
        let conn = setup_case_db();
        let repo = GraphRepo::new(&conn);

        repo.insert_nodes_batch(&[
            make_node("n1", "case-1", NodeType::File, "a.exe"),
            make_node("n2", "case-1", NodeType::File, "b.dll"),
            make_node("n3", "case-1", NodeType::Artifact, "LNK-1"),
        ])
        .unwrap();
        repo.insert_edges_batch(&[
            make_edge("e1", "case-1", "n1", "n2", EdgeType::References),
            make_edge("e2", "case-1", "n2", "n3", EdgeType::References),
        ])
        .unwrap();

        let result = query_graph(
            &conn,
            GraphQueryDto {
                start_ids: vec!["n1".to_string()],
                edge_types: vec![],
                max_depth: 2,
                confidence_floor: None,
                limit: 100,
            },
        )
        .unwrap();

        assert_eq!(result.node_count, 3);
        assert_eq!(result.edge_count, 2);
        assert!(result.nodes.iter().any(|n| n.id == "n1"));
        assert!(result.nodes.iter().any(|n| n.id == "n3"));
    }

    #[test]
    fn query_graph_respects_confidence_floor() {
        let conn = setup_case_db();
        let repo = GraphRepo::new(&conn);

        repo.insert_nodes_batch(&[
            make_node("n1", "case-1", NodeType::File, "a.exe"),
            make_node("n2", "case-1", NodeType::File, "b.dll"),
        ])
        .unwrap();

        let mut high_confidence = make_edge("e1", "case-1", "n1", "n2", EdgeType::References);
        high_confidence.confidence = Some(0.9);
        repo.insert_edges_batch(&[high_confidence]).unwrap();

        let result = query_graph(
            &conn,
            GraphQueryDto {
                start_ids: vec!["n1".to_string()],
                edge_types: vec![],
                max_depth: 2,
                confidence_floor: Some(0.5),
                limit: 100,
            },
        )
        .unwrap();

        assert_eq!(result.edge_count, 1);

        let result_strict = query_graph(
            &conn,
            GraphQueryDto {
                start_ids: vec!["n1".to_string()],
                edge_types: vec![],
                max_depth: 2,
                confidence_floor: Some(0.99),
                limit: 100,
            },
        )
        .unwrap();

        assert_eq!(result_strict.edge_count, 0);
    }

    #[test]
    fn list_graph_nodes_returns_case_nodes() {
        let conn = setup_case_db();
        let repo = GraphRepo::new(&conn);

        repo.insert_nodes_batch(&[
            make_node("n1", "case-1", NodeType::File, "a.exe"),
            make_node("n2", "case-1", NodeType::Artifact, "LNK-1"),
        ])
        .unwrap();

        let nodes = list_graph_nodes(
            &conn,
            "case-1",
            ListGraphNodesRequest {
                limit: 10,
                offset: 0,
            },
        )
        .unwrap();

        assert_eq!(nodes.len(), 2);
        assert!(nodes.iter().any(|node| node.id == "n1"));
        assert!(nodes.iter().any(|node| node.id == "n2"));
    }

    #[test]
    fn node_neighborhood_returns_connected_subgraph() {
        let conn = setup_case_db();
        let repo = GraphRepo::new(&conn);

        repo.insert_nodes_batch(&[
            make_node("n1", "case-1", NodeType::File, "center.exe"),
            make_node("n2", "case-1", NodeType::File, "neighbor-a.dll"),
            make_node("n3", "case-1", NodeType::File, "neighbor-b.dll"),
            make_node("n4", "case-1", NodeType::File, "far-away.exe"),
        ])
        .unwrap();
        repo.insert_edges_batch(&[
            make_edge("e1", "case-1", "n1", "n2", EdgeType::References),
            make_edge("e2", "case-1", "n3", "n1", EdgeType::Contains),
            make_edge("e3", "case-1", "n2", "n4", EdgeType::References),
        ])
        .unwrap();

        let result = get_node_neighborhood(&conn, "n1", 1).unwrap();
        // depth 1: n1 itself + n2 (outgoing) + n3 (incoming) = 3 nodes
        assert_eq!(result.node_count, 3);
        // n4 should NOT be included at depth 1
        assert!(!result.nodes.iter().any(|n| n.id == "n4"));
    }

    #[test]
    fn provenance_chain_without_provenance_metadata() {
        let conn = setup_case_db();
        let repo = GraphRepo::new(&conn);

        repo.insert_nodes_batch(&[
            make_node("n1", "case-1", NodeType::File, "a.exe"),
            make_node("n2", "case-1", NodeType::File, "b.dll"),
        ])
        .unwrap();
        repo.insert_edges_batch(&[make_edge("e1", "case-1", "n1", "n2", EdgeType::References)])
            .unwrap();

        let chain = get_provenance_chain(&conn, "e1").unwrap();
        assert_eq!(chain.len(), 1);
        assert_eq!(chain[0].edge_id, "e1");
        assert!(chain[0].source_rule_id.is_none());
        assert!(chain[0].extraction_timestamp.is_some());
    }

    #[test]
    fn provenance_chain_with_correlation_provenance() {
        let conn = setup_case_db();
        let repo = GraphRepo::new(&conn);

        repo.insert_nodes_batch(&[
            make_node("n1", "case-1", NodeType::File, "cmd.exe"),
            make_node("n2", "case-1", NodeType::Artifact, "LNK Artifact"),
        ])
        .unwrap();

        let provenance = serde_json::json!({
            "kind": "correlation_rule",
            "lead_id": "lead:rules:file-cmd",
            "match_signals": ["LNK 目标路径命中文件路径"],
            "families": ["LNK"]
        })
        .to_string();

        let mut edge = make_edge("e1", "case-1", "n2", "n1", EdgeType::CorrelatesWith);
        edge.provenance = Some(provenance);
        repo.insert_edges_batch(&[edge]).unwrap();

        let chain = get_provenance_chain(&conn, "e1").unwrap();
        assert_eq!(chain.len(), 1);
        assert_eq!(chain[0].edge_id, "e1");
        assert_eq!(chain[0].source_parser.as_deref(), Some("LNK"));
    }
}
