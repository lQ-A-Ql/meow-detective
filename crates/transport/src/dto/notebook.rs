//! Notebook DTOs shared across the Tauri boundary.
//!
//! Mirrors `crates/domain/src/notebook.rs` domain types with camelCase serde
//! for the frontend, plus investigation-step recording, step replay, and export
//! structures.

use serde::{Deserialize, Serialize};

use super::graph::GraphNodeTypeDto;

/// Classification of a notebook entry, indicating the nature of the investigative note.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum NotebookEntryTypeDto {
    Observation,
    Hypothesis,
    Finding,
    ActionItem,
    Conclusion,
}

/// Review status of a notebook entry, tracking its maturity in the investigative workflow.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum NotebookEntryStatusDto {
    Draft,
    Reviewed,
    Final,
}

/// A single entry in the investigator's notebook.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct NotebookEntryDto {
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
    pub entry_type: NotebookEntryTypeDto,
    /// Short title summarizing the entry.
    pub title: String,
    /// The full body of the entry in Markdown.
    pub body_markdown: String,
    /// Arbitrary tags attached to this entry.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    /// The review status of this entry.
    pub status: NotebookEntryStatusDto,
    /// ISO 8601 creation timestamp.
    pub created_at: String,
    /// ISO 8601 last-updated timestamp.
    pub updated_at: String,
}

/// A citation linking a notebook entry to a specific graph node as supporting evidence.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct EvidenceCitationDto {
    /// Unique identifier for this citation.
    pub id: String,
    /// The notebook entry that this citation belongs to.
    pub entry_id: String,
    /// The type of graph node being cited.
    pub target_node_type: GraphNodeTypeDto,
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

/// A recorded investigation step for audit/replay purposes.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct InvestigationStepDto {
    /// Unique identifier for this step record.
    pub id: String,
    /// The case this step belongs to.
    pub case_id: String,
    /// The kind of investigation action performed (e.g. "search", "artifact_extract", "timeline_query").
    pub step_kind: String,
    /// JSON-serialized parameters for this step.
    pub params_json: String,
    /// ISO 8601 timestamp when the step was executed.
    pub timestamp: String,
    /// Duration of the step in milliseconds.
    pub duration_ms: u32,
    /// Optional hash of the case state before this step was taken.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub case_state_hash: Option<String>,
    /// Whether the step completed without errors.
    pub success: bool,
    /// Optional error code if the step failed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_code: Option<String>,
}

/// A collection of investigation steps that can be replayed.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct StepReplayDto {
    /// The ordered list of investigation steps.
    pub steps: Vec<InvestigationStepDto>,
    /// Total number of steps in the replay.
    pub total_steps: u64,
    /// Whether the recorded steps can be replayed against the current case state.
    pub replayable: bool,
    /// Caveats or warnings about replay fidelity.
    pub caveats: Vec<String>,
}

/// The result of replaying a range of investigation steps.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct StepReplayResultDto {
    /// Steps whose re-execution produced matching results.
    pub matched_steps: Vec<StepReplayMatchDto>,
    /// Steps whose re-execution produced differing results.
    pub differed_steps: Vec<StepReplayDifferDto>,
    /// Steps that failed during re-execution.
    pub failed_steps: Vec<StepReplayFailDto>,
    /// Observations or caveats about the replay.
    pub caveats: Vec<String>,
}

/// A step that replayed successfully with matching results.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct StepReplayMatchDto {
    pub step_id: String,
    pub step_kind: String,
    pub recorded_duration_ms: u32,
    pub replay_duration_ms: u32,
    pub detail: String,
}

/// A step that replayed but produced different results from the recording.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct StepReplayDifferDto {
    pub step_id: String,
    pub step_kind: String,
    pub recorded_duration_ms: u32,
    pub replay_duration_ms: u32,
    pub expected: String,
    pub actual: String,
}

/// A step that failed during re-execution.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct StepReplayFailDto {
    pub step_id: String,
    pub step_kind: String,
    pub recorded_duration_ms: u32,
    pub error: String,
}

// ── Request DTOs for Tauri command parameters ─────────────────────────

/// Request payload for creating a new notebook entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateNotebookEntryRequest {
    pub author: String,
    pub entry_type: NotebookEntryTypeDto,
    pub title: String,
    pub body_markdown: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    pub status: NotebookEntryStatusDto,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<String>,
}

/// Request payload for updating an existing notebook entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateNotebookEntryRequest {
    pub entry_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub body_markdown: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<NotebookEntryStatusDto>,
}

/// Request payload for listing notebook entries with filters.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListNotebookEntriesRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entry_type: Option<NotebookEntryTypeDto>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<NotebookEntryStatusDto>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub search: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub offset: Option<u32>,
}

/// Request payload for retrieving a notebook thread.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetNotebookThreadRequest {
    pub entry_id: String,
}

/// Request payload for adding an evidence citation to a notebook entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AddEvidenceCitationRequest {
    pub entry_id: String,
    pub target_node_type: GraphNodeTypeDto,
    pub target_node_id: String,
    pub display_label: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub snippet: Option<String>,
}

/// Request payload for listing investigation steps with filters.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListInvestigationStepsRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub step_kind: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub success: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub offset: Option<u32>,
}

/// A directed edge in the notebook entry thread graph (parent-child relationships).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct NotebookThreadEdgeDto {
    /// The id of the source (parent) entry.
    pub source_entry_id: String,
    /// The id of the target (child) entry.
    pub target_entry_id: String,
    /// Human-readable label for this thread relationship.
    pub label: String,
}

/// Full notebook export containing entries, citations, and the thread graph.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct NotebookExportDto {
    /// All notebook entries in this export.
    pub entries: Vec<NotebookEntryDto>,
    /// All evidence citations in this export.
    pub citations: Vec<EvidenceCitationDto>,
    /// The parent-child thread edges between entries.
    pub thread_graph: Vec<NotebookThreadEdgeDto>,
}

#[cfg(test)]
#[path = "../../tests/unit/dto/notebook.rs"]
mod tests;
