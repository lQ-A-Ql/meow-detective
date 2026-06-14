//! Graph domain types for knowledge-graph style entity relationship modeling.
//!
//! Provides the core node and edge types used to represent investigative
//! relationships between files, artifacts, timeline events, entities, leads,
//! and notebook entries within a case.

use serde::{Deserialize, Serialize};

/// The type of a graph node, categorizing the kind of investigative item it represents.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum NodeType {
    File,
    Artifact,
    TimelineEvent,
    Entity,
    Lead,
    NotebookEntry,
}

/// The type of a graph edge, describing the semantic relationship between two nodes.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum EdgeType {
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
pub struct GraphNode {
    /// Unique identifier for this node.
    pub id: String,
    /// The case this node belongs to.
    pub case_id: String,
    /// The kind of investigative item this node represents.
    pub node_type: NodeType,
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
pub struct GraphEdge {
    /// Unique identifier for this edge.
    pub id: String,
    /// The case this edge belongs to.
    pub case_id: String,
    /// The id of the source node.
    pub source_id: String,
    /// The id of the target node.
    pub target_id: String,
    /// The semantic type of this relationship.
    pub edge_type: EdgeType,
    /// Optional confidence score between 0.0 and 1.0.
    pub confidence: Option<f64>,
    /// Optional structured provenance metadata serialized as JSON.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provenance: Option<String>,
    /// ISO 8601 creation timestamp.
    pub created_at: String,
}
