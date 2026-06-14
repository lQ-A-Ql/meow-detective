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
    pub largest_component_size: u64,
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
mod tests {
    use super::*;

    #[test]
    fn graph_node_dto_serializes_camel_case() {
        let node = GraphNodeDto {
            id: "node-1".to_string(),
            case_id: "case-1".to_string(),
            node_type: GraphNodeTypeDto::File,
            label: "cmd.exe".to_string(),
            summary: "Command Prompt executable".to_string(),
            tags: vec!["executable".to_string(), "system".to_string()],
            created_at: "2026-06-14T00:00:00Z".to_string(),
        };

        let json = serde_json::to_value(node).unwrap();

        assert_eq!(json["id"], "node-1");
        assert_eq!(json["caseId"], "case-1");
        assert_eq!(json["nodeType"], "file");
        assert_eq!(json["label"], "cmd.exe");
        assert_eq!(json["summary"], "Command Prompt executable");
        assert_eq!(json["tags"][0], "executable");
        assert_eq!(json["tags"][1], "system");
        assert_eq!(json["createdAt"], "2026-06-14T00:00:00Z");
        // Ensure snake_case keys are absent
        assert!(json.get("case_id").is_none());
        assert!(json.get("node_type").is_none());
        assert!(json.get("created_at").is_none());
    }

    #[test]
    fn graph_edge_dto_serializes_camel_case() {
        let edge = GraphEdgeDto {
            id: "edge-1".to_string(),
            case_id: "case-1".to_string(),
            source_id: "node-1".to_string(),
            target_id: "node-2".to_string(),
            edge_type: GraphEdgeTypeDto::References,
            confidence: Some(0.95),
            provenance: None,
            created_at: "2026-06-14T00:00:00Z".to_string(),
        };

        let json = serde_json::to_value(edge).unwrap();

        assert_eq!(json["id"], "edge-1");
        assert_eq!(json["caseId"], "case-1");
        assert_eq!(json["sourceId"], "node-1");
        assert_eq!(json["targetId"], "node-2");
        assert_eq!(json["edgeType"], "references");
        assert_eq!(json["confidence"], 0.95);
        assert_eq!(json["createdAt"], "2026-06-14T00:00:00Z");
        // provenance is None, should be absent
        assert!(json.get("provenance").is_none());
        // Ensure snake_case keys are absent
        assert!(json.get("case_id").is_none());
        assert!(json.get("source_id").is_none());
        assert!(json.get("target_id").is_none());
        assert!(json.get("edge_type").is_none());
    }

    #[test]
    fn graph_edge_dto_omits_none_confidence() {
        let edge = GraphEdgeDto {
            id: "edge-2".to_string(),
            case_id: "case-1".to_string(),
            source_id: "node-1".to_string(),
            target_id: "node-3".to_string(),
            edge_type: GraphEdgeTypeDto::CorrelatesWith,
            confidence: None,
            provenance: Some(r#"{"source":"artifact-1"}"#.to_string()),
            created_at: "2026-06-14T00:00:00Z".to_string(),
        };

        let json = serde_json::to_value(edge).unwrap();

        assert!(json.get("confidence").is_none());
        assert_eq!(json["provenance"], r#"{"source":"artifact-1"}"#);
    }

    #[test]
    fn graph_query_dto_defaults_and_camel_case() {
        let query = GraphQueryDto {
            start_ids: vec!["node-1".to_string()],
            edge_types: vec!["references".to_string(), "contains".to_string()],
            max_depth: 3,
            confidence_floor: Some(0.5),
            limit: 100,
        };

        let json = serde_json::to_value(&query).unwrap();

        assert_eq!(json["startIds"][0], "node-1");
        assert_eq!(json["edgeTypes"][1], "contains");
        assert_eq!(json["maxDepth"], 3);
        assert_eq!(json["confidenceFloor"], 0.5);
        assert_eq!(json["limit"], 100);
        assert!(json.get("start_ids").is_none());
        assert!(json.get("max_depth").is_none());
        assert!(json.get("confidence_floor").is_none());
    }

    #[test]
    fn graph_query_dto_deserializes_with_defaults() {
        let json = serde_json::json!({
            "startIds": ["node-1"],
            "edgeTypes": []
        });

        let query: GraphQueryDto = serde_json::from_value(json).unwrap();

        assert_eq!(query.start_ids, vec!["node-1".to_string()]);
        assert!(query.edge_types.is_empty());
        assert_eq!(query.max_depth, 3);
        assert_eq!(query.confidence_floor, None);
        assert_eq!(query.limit, 100);
    }

    #[test]
    fn graph_query_result_dto_serializes_camel_case() {
        let node = GraphNodeDto {
            id: "node-1".to_string(),
            case_id: "case-1".to_string(),
            node_type: GraphNodeTypeDto::Artifact,
            label: "LNK Artifact".to_string(),
            summary: "Shell link file".to_string(),
            tags: vec![],
            created_at: "2026-06-14T00:00:00Z".to_string(),
        };

        let edge = GraphEdgeDto {
            id: "edge-1".to_string(),
            case_id: "case-1".to_string(),
            source_id: "node-1".to_string(),
            target_id: "node-2".to_string(),
            edge_type: GraphEdgeTypeDto::References,
            confidence: Some(0.8),
            provenance: None,
            created_at: "2026-06-14T00:00:00Z".to_string(),
        };

        let result = GraphQueryResultDto {
            nodes: vec![node],
            edges: vec![edge],
            node_count: 1,
            edge_count: 1,
        };

        let json = serde_json::to_value(result).unwrap();

        assert_eq!(json["nodeCount"], 1);
        assert_eq!(json["edgeCount"], 1);
        assert_eq!(json["nodes"][0]["id"], "node-1");
        assert_eq!(json["edges"][0]["id"], "edge-1");
        assert!(json.get("node_count").is_none());
        assert!(json.get("edge_count").is_none());
    }

    #[test]
    fn graph_snapshot_dto_serializes_camel_case() {
        let mut node_count_by_type = std::collections::HashMap::new();
        node_count_by_type.insert("file".to_string(), 42);
        node_count_by_type.insert("artifact".to_string(), 15);

        let mut edge_count_by_type = std::collections::HashMap::new();
        edge_count_by_type.insert("references".to_string(), 30);
        edge_count_by_type.insert("contains".to_string(), 20);

        let snapshot = GraphSnapshotDto {
            node_count_by_type,
            edge_count_by_type,
            total_nodes: 57,
            total_edges: 50,
            density: 0.0313,
            largest_component_size: 40,
        };

        let json = serde_json::to_value(snapshot).unwrap();

        assert_eq!(json["totalNodes"], 57);
        assert_eq!(json["totalEdges"], 50);
        assert_eq!(json["density"], 0.0313);
        assert_eq!(json["largestComponentSize"], 40);
        assert_eq!(json["nodeCountByType"]["file"], 42);
        assert_eq!(json["nodeCountByType"]["artifact"], 15);
        assert_eq!(json["edgeCountByType"]["references"], 30);
        assert_eq!(json["edgeCountByType"]["contains"], 20);
        assert!(json.get("total_nodes").is_none());
        assert!(json.get("total_edges").is_none());
        assert!(json.get("node_count_by_type").is_none());
        assert!(json.get("edge_count_by_type").is_none());
        assert!(json.get("largest_component_size").is_none());
    }
}
