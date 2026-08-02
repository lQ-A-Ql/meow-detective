use serde::{Deserialize, Serialize};

/// The type of a graph node, categorizing the kind of investigative item it represents.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum GraphNodeTypeDto {
    File,
    Artifact,
    TimelineEvent,
    Entity,
    Lead,
    NotebookEntry,
}

/// The type of a graph edge, describing the semantic relationship between two nodes.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum GraphEdgeTypeDto {
    Contains,
    References,
    CorrelatesWith,
    DerivesFrom,
    Precedes,
    Cites,
    Annotates,
}

/// A node in the investigative graph, representing a single item of interest.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GraphNodeDto {
    /// Unique identifier for this node.
    pub id: String,
    /// The case this node belongs to.
    pub case_id: String,
    /// The kind of investigative item this node represents.
    pub node_type: GraphNodeTypeDto,
    /// Short human-readable label.
    pub label: String,
    /// Longer descriptive summary of this node.
    pub summary: String,
    /// Arbitrary tags attached to this node.
    pub tags: Vec<String>,
    /// ISO 8601 creation timestamp.
    pub created_at: String,
}

/// An edge in the investigative graph, representing a directional relationship between two nodes.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct GraphEdgeDto {
    /// Unique identifier for this edge.
    pub id: String,
    /// The case this edge belongs to.
    pub case_id: String,
    /// The id of the source node.
    pub source_id: String,
    /// The id of the target node.
    pub target_id: String,
    /// The semantic type of this relationship.
    pub edge_type: GraphEdgeTypeDto,
    /// Optional confidence score between 0.0 and 1.0.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confidence: Option<f64>,
    /// Optional structured provenance metadata serialized as JSON.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provenance: Option<String>,
    /// ISO 8601 creation timestamp.
    pub created_at: String,
}

fn default_max_depth() -> u32 {
    3
}

fn default_limit() -> u32 {
    100
}

fn default_edge_limit() -> u32 {
    400
}

/// Query parameters for traversing the investigative graph.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct GraphQueryDto {
    /// Starting node ids for graph traversal.
    pub start_ids: Vec<String>,
    /// Filter to specific edge types; empty means all types.
    pub edge_types: Vec<String>,
    /// Maximum traversal depth from starting nodes.
    #[serde(default = "default_max_depth")]
    pub max_depth: u32,
    /// Optional minimum confidence threshold (0.0–1.0) for returned edges.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confidence_floor: Option<f64>,
    /// Maximum number of nodes to return.
    #[serde(default = "default_limit")]
    pub limit: u32,
    /// Maximum number of edges to return.
    #[serde(default = "default_edge_limit")]
    pub edge_limit: u32,
}

/// Result of a graph query, containing the matched subgraph.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct GraphQueryResultDto {
    /// Nodes in the result subgraph.
    pub nodes: Vec<GraphNodeDto>,
    /// Edges in the result subgraph.
    pub edges: Vec<GraphEdgeDto>,
    /// Total number of nodes matched.
    pub node_count: u32,
    /// Total number of edges matched.
    pub edge_count: u32,
    /// True when a node or edge budget prevented the full requested window.
    #[serde(default)]
    pub truncated: bool,
    /// Deepest hop represented in this result.
    #[serde(default)]
    pub max_depth_reached: u32,
    /// Data sources represented by returned source-scoped nodes.
    #[serde(default)]
    pub data_source_ids: Vec<String>,
}

/// Request DTO for listing graph nodes without requiring a traversal seed.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ListGraphNodesRequest {
    /// Maximum number of nodes to return.
    #[serde(default = "default_limit")]
    pub limit: u32,
    /// Zero-based row offset.
    #[serde(default)]
    pub offset: u32,
}

/// Aggregate statistics snapshot of the entire investigative graph.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct GraphSnapshotDto {
    /// Count of nodes grouped by node type.
    pub node_count_by_type: std::collections::HashMap<String, u64>,
    /// Count of edges grouped by edge type.
    pub edge_count_by_type: std::collections::HashMap<String, u64>,
    /// Total number of nodes in the graph.
    pub total_nodes: u64,
    /// Total number of edges in the graph.
    pub total_edges: u64,
    /// Graph density: (2 * total_edges) / (total_nodes * (total_nodes - 1)) for total_nodes > 1, else 0.
    pub density: f64,
    /// Size of the largest connected component.
    /// Zero means the expensive full component calculation was not materialized.
    pub largest_component_size: u64,
    /// Number of ready data sources represented by the case graph.
    #[serde(default)]
    pub data_source_count: u32,
    /// Number of deterministic case-level entity hubs.
    #[serde(default)]
    pub cross_source_entity_count: u64,
    /// Number of exact cross-source entity projection edges.
    #[serde(default)]
    pub cross_source_edge_count: u64,
    /// Backend-selected deterministic entry nodes for case-level exploration.
    #[serde(default)]
    pub seed_ids: Vec<String>,
    /// ISO 8601 timestamp of the current case graph projection.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub projection_built_at: Option<String>,
}

/// Request DTO for `get_node_neighborhood`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetNodeNeighborhoodRequest {
    pub node_id: String,
    #[serde(default = "default_depth")]
    pub depth: u32,
}

fn default_depth() -> u32 {
    1
}

/// Request DTO for `get_provenance_chain`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetProvenanceChainRequest {
    pub edge_id: String,
}

/// Provenance entry tracing how a graph edge was created by a specific rule/parser.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct GraphProvenanceEntryDto {
    /// The edge id this provenance entry belongs to.
    pub edge_id: String,
    /// Identifier of the rule that created this edge (e.g. correlation rule id).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_rule_id: Option<String>,
    /// Identifier of the parser that produced the underlying evidence.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_parser: Option<String>,
    /// ISO 8601 timestamp of when the edge was extracted/created.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extraction_timestamp: Option<String>,
    /// Version of the parser that produced the underlying evidence.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parser_version: Option<String>,
}

#[cfg(test)]
#[path = "../../tests/unit/dto/graph.rs"]
mod tests;
