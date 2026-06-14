//! Notebook domain types for investigative notebook entries and evidence citations.
//!
//! Provides types for structured note-taking during investigations: entries with
//! types like observations and findings, status tracking, and explicit citations
//! that link notebook content to graph nodes.

use serde::{Deserialize, Serialize};

/// The classification of a notebook entry, indicating the nature of the investigative note.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum EntryType {
    Observation,
    Hypothesis,
    Finding,
    ActionItem,
    Conclusion,
}

/// The review status of a notebook entry, tracking its maturity in the investigative workflow.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum EntryStatus {
    Draft,
    Reviewed,
    Final,
}

/// A single entry in the investigator's notebook, capturing structured notes and analysis.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct NotebookEntry {
    /// Unique identifier for this notebook entry.
    pub id: String,
    /// The case this entry belongs to.
    pub case_id: String,
    /// Optional parent entry id for hierarchical threading.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<String>,
    /// Name of the investigator who authored this entry.
    pub author: String,
    /// The classification of this entry.
    pub entry_type: EntryType,
    /// Short title summarizing the entry.
    pub title: String,
    /// The full body of the entry in Markdown.
    pub body_markdown: String,
    /// Arbitrary tags attached to this entry.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    /// The review status of this entry.
    pub status: EntryStatus,
    /// ISO 8601 creation timestamp.
    pub created_at: String,
    /// ISO 8601 last-updated timestamp.
    pub updated_at: String,
}

/// A citation linking a notebook entry to a specific graph node as supporting evidence.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EvidenceCitation {
    /// Unique identifier for this citation.
    pub id: String,
    /// The notebook entry that this citation belongs to.
    pub entry_id: String,
    /// The type of graph node being cited.
    pub target_node_type: crate::graph::NodeType,
    /// The id of the graph node being cited.
    pub target_node_id: String,
    /// Human-readable label for the citation link.
    pub display_label: String,
    /// Optional quoted snippet from the source that supports the citation.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub snippet: Option<String>,
    /// ISO 8601 timestamp when this citation was created.
    pub cited_at: String,
}
