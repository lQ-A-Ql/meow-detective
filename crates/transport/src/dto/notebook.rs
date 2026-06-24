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
mod tests {
    use super::*;

    // ── Notebook entry ──────────────────────────────────────────────────

    #[test]
    fn notebook_entry_dto_serializes_camel_case() {
        let entry = NotebookEntryDto {
            id: "entry-1".to_string(),
            case_id: "case-1".to_string(),
            parent_id: None,
            author: "investigator".to_string(),
            entry_type: NotebookEntryTypeDto::Observation,
            title: "Suspicious file noted".to_string(),
            body_markdown: "## Details\n\nFound `cmd.exe` with an unexpected hash.".to_string(),
            tags: vec!["suspicious".to_string(), "cmd".to_string()],
            status: NotebookEntryStatusDto::Draft,
            created_at: "2026-06-14T12:00:00Z".to_string(),
            updated_at: "2026-06-14T12:30:00Z".to_string(),
        };

        let json = serde_json::to_value(entry).unwrap();

        assert_eq!(json["id"], "entry-1");
        assert_eq!(json["caseId"], "case-1");
        assert_eq!(json["author"], "investigator");
        assert_eq!(json["entryType"], "observation");
        assert_eq!(json["title"], "Suspicious file noted");
        assert_eq!(
            json["bodyMarkdown"],
            "## Details\n\nFound `cmd.exe` with an unexpected hash."
        );
        assert_eq!(json["tags"][0], "suspicious");
        assert_eq!(json["tags"][1], "cmd");
        assert_eq!(json["status"], "draft");
        assert_eq!(json["createdAt"], "2026-06-14T12:00:00Z");
        assert_eq!(json["updatedAt"], "2026-06-14T12:30:00Z");
        // Optional fields with None should be absent
        assert!(json.get("parentId").is_none());
        // Ensure snake_case keys are absent
        assert!(json.get("case_id").is_none());
        assert!(json.get("entry_type").is_none());
        assert!(json.get("body_markdown").is_none());
    }

    #[test]
    fn notebook_entry_dto_serializes_with_parent_id() {
        let entry = NotebookEntryDto {
            id: "entry-2".to_string(),
            case_id: "case-1".to_string(),
            parent_id: Some("entry-1".to_string()),
            author: "investigator".to_string(),
            entry_type: NotebookEntryTypeDto::Hypothesis,
            title: "Malware hypothesis".to_string(),
            body_markdown: "Possibly a dropper.".to_string(),
            tags: vec![],
            status: NotebookEntryStatusDto::Draft,
            created_at: "2026-06-14T13:00:00Z".to_string(),
            updated_at: "2026-06-14T13:00:00Z".to_string(),
        };

        let json = serde_json::to_value(entry).unwrap();

        assert_eq!(json["parentId"], "entry-1");
        assert_eq!(json["entryType"], "hypothesis");
        // Empty tags vec should be absent
        assert!(json.get("tags").is_none());
    }

    #[test]
    fn notebook_entry_dto_roundtrip() {
        let entry = NotebookEntryDto {
            id: "entry-1".to_string(),
            case_id: "case-1".to_string(),
            parent_id: Some("parent-1".to_string()),
            author: "investigator".to_string(),
            entry_type: NotebookEntryTypeDto::Finding,
            title: "Confirmed persistence".to_string(),
            body_markdown: "Run key found.".to_string(),
            tags: vec!["persistence".to_string(), "registry".to_string()],
            status: NotebookEntryStatusDto::Reviewed,
            created_at: "2026-06-14T12:00:00Z".to_string(),
            updated_at: "2026-06-14T14:00:00Z".to_string(),
        };

        let json = serde_json::to_value(&entry).unwrap();
        let roundtripped: NotebookEntryDto = serde_json::from_value(json).unwrap();

        assert_eq!(entry, roundtripped);
    }

    // ── Evidence citation ───────────────────────────────────────────────

    #[test]
    fn evidence_citation_dto_serializes_camel_case() {
        let citation = EvidenceCitationDto {
            id: "cite-1".to_string(),
            entry_id: "entry-1".to_string(),
            target_node_type: GraphNodeTypeDto::File,
            target_node_id: "node-file-1".to_string(),
            display_label: "cmd.exe hash mismatch".to_string(),
            snippet: Some("SHA256: abcd1234...".to_string()),
            cited_at: "2026-06-14T12:05:00Z".to_string(),
        };

        let json = serde_json::to_value(citation).unwrap();

        assert_eq!(json["id"], "cite-1");
        assert_eq!(json["entryId"], "entry-1");
        assert_eq!(json["targetNodeType"], "file");
        assert_eq!(json["targetNodeId"], "node-file-1");
        assert_eq!(json["displayLabel"], "cmd.exe hash mismatch");
        assert_eq!(json["snippet"], "SHA256: abcd1234...");
        assert_eq!(json["citedAt"], "2026-06-14T12:05:00Z");
        // Ensure snake_case keys are absent
        assert!(json.get("entry_id").is_none());
        assert!(json.get("target_node_type").is_none());
        assert!(json.get("target_node_id").is_none());
        assert!(json.get("display_label").is_none());
        assert!(json.get("cited_at").is_none());
    }

    #[test]
    fn evidence_citation_dto_omits_none_snippet() {
        let citation = EvidenceCitationDto {
            id: "cite-2".to_string(),
            entry_id: "entry-1".to_string(),
            target_node_type: GraphNodeTypeDto::Artifact,
            target_node_id: "node-artifact-1".to_string(),
            display_label: "LNK artifact".to_string(),
            snippet: None,
            cited_at: "2026-06-14T12:10:00Z".to_string(),
        };

        let json = serde_json::to_value(citation).unwrap();

        assert!(json.get("snippet").is_none());
    }

    #[test]
    fn evidence_citation_dto_roundtrip() {
        let citation = EvidenceCitationDto {
            id: "cite-1".to_string(),
            entry_id: "entry-1".to_string(),
            target_node_type: GraphNodeTypeDto::File,
            target_node_id: "node-1".to_string(),
            display_label: "label".to_string(),
            snippet: Some("snippet text".to_string()),
            cited_at: "2026-06-14T12:00:00Z".to_string(),
        };

        let json = serde_json::to_value(&citation).unwrap();
        let roundtripped: EvidenceCitationDto = serde_json::from_value(json).unwrap();

        assert_eq!(citation, roundtripped);
    }

    // ── Investigation step ──────────────────────────────────────────────

    #[test]
    fn investigation_step_dto_serializes_camel_case() {
        let step = InvestigationStepDto {
            id: "step-1".to_string(),
            case_id: "case-1".to_string(),
            step_kind: "search".to_string(),
            params_json: r#"{"query":"malware","caseId":"case-1"}"#.to_string(),
            timestamp: "2026-06-14T12:00:00Z".to_string(),
            duration_ms: 1523,
            case_state_hash: Some("abc123hash".to_string()),
            success: true,
            error_code: None,
        };

        let json = serde_json::to_value(step).unwrap();

        assert_eq!(json["id"], "step-1");
        assert_eq!(json["caseId"], "case-1");
        assert_eq!(json["stepKind"], "search");
        assert_eq!(
            json["paramsJson"],
            r#"{"query":"malware","caseId":"case-1"}"#
        );
        assert_eq!(json["timestamp"], "2026-06-14T12:00:00Z");
        assert_eq!(json["durationMs"], 1523);
        assert_eq!(json["caseStateHash"], "abc123hash");
        assert_eq!(json["success"], true);
        assert!(json.get("errorCode").is_none());
        // Ensure snake_case keys are absent
        assert!(json.get("case_id").is_none());
        assert!(json.get("step_kind").is_none());
        assert!(json.get("params_json").is_none());
        assert!(json.get("duration_ms").is_none());
        assert!(json.get("case_state_hash").is_none());
        assert!(json.get("error_code").is_none());
    }

    #[test]
    fn investigation_step_dto_serializes_failed_step() {
        let step = InvestigationStepDto {
            id: "step-2".to_string(),
            case_id: "case-1".to_string(),
            step_kind: "artifact_extract".to_string(),
            params_json: r#"{"family":"LNK"}"#.to_string(),
            timestamp: "2026-06-14T12:01:00Z".to_string(),
            duration_ms: 0,
            case_state_hash: None,
            success: false,
            error_code: Some("E_PARSE_FAILED".to_string()),
        };

        let json = serde_json::to_value(step).unwrap();

        assert_eq!(json["success"], false);
        assert_eq!(json["errorCode"], "E_PARSE_FAILED");
        assert!(json.get("caseStateHash").is_none());
    }

    #[test]
    fn investigation_step_dto_roundtrip() {
        let step = InvestigationStepDto {
            id: "step-1".to_string(),
            case_id: "case-1".to_string(),
            step_kind: "timeline_query".to_string(),
            params_json: r#"{"start":"2026-01-01","end":"2026-06-14"}"#.to_string(),
            timestamp: "2026-06-14T12:00:00Z".to_string(),
            duration_ms: 2500,
            case_state_hash: Some("def456".to_string()),
            success: true,
            error_code: None,
        };

        let json = serde_json::to_value(&step).unwrap();
        let roundtripped: InvestigationStepDto = serde_json::from_value(json).unwrap();

        assert_eq!(step, roundtripped);
    }

    // ── Step replay ─────────────────────────────────────────────────────

    #[test]
    fn step_replay_dto_serializes_camel_case() {
        let steps = vec![
            InvestigationStepDto {
                id: "step-1".to_string(),
                case_id: "case-1".to_string(),
                step_kind: "search".to_string(),
                params_json: r#"{"query":"test"}"#.to_string(),
                timestamp: "2026-06-14T12:00:00Z".to_string(),
                duration_ms: 100,
                case_state_hash: None,
                success: true,
                error_code: None,
            },
            InvestigationStepDto {
                id: "step-2".to_string(),
                case_id: "case-1".to_string(),
                step_kind: "artifact_extract".to_string(),
                params_json: r#"{"family":"LNK"}"#.to_string(),
                timestamp: "2026-06-14T12:01:00Z".to_string(),
                duration_ms: 500,
                case_state_hash: None,
                success: true,
                error_code: None,
            },
        ];

        let replay = StepReplayDto {
            steps,
            total_steps: 2,
            replayable: true,
            caveats: vec![
                "Case state hash differs from recording".to_string(),
                "External MCP connections may not be available".to_string(),
            ],
        };

        let json = serde_json::to_value(replay).unwrap();

        assert_eq!(json["totalSteps"], 2);
        assert_eq!(json["replayable"], true);
        assert_eq!(json["caveats"][0], "Case state hash differs from recording");
        assert_eq!(
            json["caveats"][1],
            "External MCP connections may not be available"
        );
        assert_eq!(json["steps"][0]["stepKind"], "search");
        assert_eq!(json["steps"][1]["stepKind"], "artifact_extract");
        // Ensure snake_case keys are absent
        assert!(json.get("total_steps").is_none());
    }

    #[test]
    fn step_replay_dto_roundtrip() {
        let steps = vec![InvestigationStepDto {
            id: "step-1".to_string(),
            case_id: "case-1".to_string(),
            step_kind: "search".to_string(),
            params_json: r#"{}"#.to_string(),
            timestamp: "2026-06-14T12:00:00Z".to_string(),
            duration_ms: 0,
            case_state_hash: None,
            success: true,
            error_code: None,
        }];

        let replay = StepReplayDto {
            steps,
            total_steps: 1,
            replayable: false,
            caveats: vec!["test caveat".to_string()],
        };

        let json = serde_json::to_value(&replay).unwrap();
        let roundtripped: StepReplayDto = serde_json::from_value(json).unwrap();

        assert_eq!(replay, roundtripped);
    }

    // ── Thread edge ─────────────────────────────────────────────────────

    #[test]
    fn notebook_thread_edge_dto_serializes_camel_case() {
        let edge = NotebookThreadEdgeDto {
            source_entry_id: "entry-1".to_string(),
            target_entry_id: "entry-2".to_string(),
            label: "response to".to_string(),
        };

        let json = serde_json::to_value(edge).unwrap();

        assert_eq!(json["sourceEntryId"], "entry-1");
        assert_eq!(json["targetEntryId"], "entry-2");
        assert_eq!(json["label"], "response to");
        assert!(json.get("source_entry_id").is_none());
        assert!(json.get("target_entry_id").is_none());
    }

    #[test]
    fn notebook_thread_edge_dto_roundtrip() {
        let edge = NotebookThreadEdgeDto {
            source_entry_id: "entry-1".to_string(),
            target_entry_id: "entry-2".to_string(),
            label: "follows".to_string(),
        };

        let json = serde_json::to_value(&edge).unwrap();
        let roundtripped: NotebookThreadEdgeDto = serde_json::from_value(json).unwrap();

        assert_eq!(edge, roundtripped);
    }

    // ── Notebook export ─────────────────────────────────────────────────

    #[test]
    fn notebook_export_dto_serializes_camel_case() {
        let entries = vec![NotebookEntryDto {
            id: "entry-1".to_string(),
            case_id: "case-1".to_string(),
            parent_id: None,
            author: "investigator".to_string(),
            entry_type: NotebookEntryTypeDto::Finding,
            title: "Finding 1".to_string(),
            body_markdown: "Content here.".to_string(),
            tags: vec!["tag1".to_string()],
            status: NotebookEntryStatusDto::Final,
            created_at: "2026-06-14T12:00:00Z".to_string(),
            updated_at: "2026-06-14T12:00:00Z".to_string(),
        }];

        let citations = vec![EvidenceCitationDto {
            id: "cite-1".to_string(),
            entry_id: "entry-1".to_string(),
            target_node_type: GraphNodeTypeDto::File,
            target_node_id: "node-1".to_string(),
            display_label: "label".to_string(),
            snippet: None,
            cited_at: "2026-06-14T12:00:00Z".to_string(),
        }];

        let thread_graph = vec![NotebookThreadEdgeDto {
            source_entry_id: "entry-1".to_string(),
            target_entry_id: "entry-2".to_string(),
            label: "follows".to_string(),
        }];

        let export = NotebookExportDto {
            entries,
            citations,
            thread_graph,
        };

        let json = serde_json::to_value(export).unwrap();

        assert_eq!(json["entries"][0]["id"], "entry-1");
        assert_eq!(json["entries"][0]["entryType"], "finding");
        assert_eq!(json["citations"][0]["id"], "cite-1");
        assert_eq!(json["citations"][0]["targetNodeType"], "file");
        assert_eq!(json["threadGraph"][0]["sourceEntryId"], "entry-1");
        assert_eq!(json["threadGraph"][0]["targetEntryId"], "entry-2");
        assert!(json.get("thread_graph").is_none());
    }

    #[test]
    fn notebook_export_dto_roundtrip() {
        let entries = vec![NotebookEntryDto {
            id: "entry-1".to_string(),
            case_id: "case-1".to_string(),
            parent_id: Some("parent-1".to_string()),
            author: "a".to_string(),
            entry_type: NotebookEntryTypeDto::Conclusion,
            title: "T".to_string(),
            body_markdown: "B".to_string(),
            tags: vec!["t".to_string()],
            status: NotebookEntryStatusDto::Final,
            created_at: "2026-06-14T12:00:00Z".to_string(),
            updated_at: "2026-06-14T12:00:00Z".to_string(),
        }];

        let citations = vec![EvidenceCitationDto {
            id: "cite-1".to_string(),
            entry_id: "entry-1".to_string(),
            target_node_type: GraphNodeTypeDto::NotebookEntry,
            target_node_id: "node-1".to_string(),
            display_label: "L".to_string(),
            snippet: Some("S".to_string()),
            cited_at: "2026-06-14T12:00:00Z".to_string(),
        }];

        let thread_graph = vec![NotebookThreadEdgeDto {
            source_entry_id: "entry-1".to_string(),
            target_entry_id: "entry-2".to_string(),
            label: "response".to_string(),
        }];

        let export = NotebookExportDto {
            entries,
            citations,
            thread_graph,
        };

        let json = serde_json::to_value(&export).unwrap();
        let roundtripped: NotebookExportDto = serde_json::from_value(json).unwrap();

        assert_eq!(export, roundtripped);
    }

    // ── Deserialization from camelCase JSON ─────────────────────────────

    #[test]
    fn notebook_entry_dto_deserializes_from_camel_case() {
        let json = serde_json::json!({
            "id": "entry-1",
            "caseId": "case-1",
            "parentId": "parent-1",
            "author": "investigator",
            "entryType": "finding",
            "title": "Test",
            "bodyMarkdown": "Body",
            "tags": ["tag1"],
            "status": "final",
            "createdAt": "2026-06-14T12:00:00Z",
            "updatedAt": "2026-06-14T12:00:00Z"
        });

        let entry: NotebookEntryDto = serde_json::from_value(json).unwrap();

        assert_eq!(entry.id, "entry-1");
        assert_eq!(entry.case_id, "case-1");
        assert_eq!(entry.parent_id, Some("parent-1".to_string()));
        assert_eq!(entry.entry_type, NotebookEntryTypeDto::Finding);
        assert_eq!(entry.status, NotebookEntryStatusDto::Final);
    }

    #[test]
    fn evidence_citation_dto_deserializes_from_camel_case() {
        let json = serde_json::json!({
            "id": "cite-1",
            "entryId": "entry-1",
            "targetNodeType": "file",
            "targetNodeId": "node-1",
            "displayLabel": "label",
            "citedAt": "2026-06-14T12:00:00Z"
        });

        let citation: EvidenceCitationDto = serde_json::from_value(json).unwrap();

        assert_eq!(citation.id, "cite-1");
        assert_eq!(citation.entry_id, "entry-1");
        assert_eq!(citation.target_node_type, GraphNodeTypeDto::File);
        assert_eq!(citation.target_node_id, "node-1");
        assert_eq!(citation.snippet, None);
    }

    #[test]
    fn investigation_step_dto_deserializes_from_camel_case() {
        let json = serde_json::json!({
            "id": "step-1",
            "caseId": "case-1",
            "stepKind": "search",
            "paramsJson": "{}",
            "timestamp": "2026-06-14T12:00:00Z",
            "durationMs": 100,
            "success": true
        });

        let step: InvestigationStepDto = serde_json::from_value(json).unwrap();

        assert_eq!(step.id, "step-1");
        assert_eq!(step.case_id, "case-1");
        assert_eq!(step.step_kind, "search");
        assert_eq!(step.params_json, "{}");
        assert_eq!(step.duration_ms, 100);
        assert_eq!(step.case_state_hash, None);
        assert!(step.success);
        assert_eq!(step.error_code, None);
    }

    #[test]
    fn step_replay_dto_deserializes_from_camel_case() {
        let json = serde_json::json!({
            "steps": [{
                "id": "step-1",
                "caseId": "case-1",
                "stepKind": "search",
                "paramsJson": "{}",
                "timestamp": "2026-06-14T12:00:00Z",
                "durationMs": 100,
                "success": true
            }],
            "totalSteps": 1,
            "replayable": false,
            "caveats": ["c1"]
        });

        let replay: StepReplayDto = serde_json::from_value(json).unwrap();

        assert_eq!(replay.total_steps, 1);
        assert!(!replay.replayable);
        assert_eq!(replay.caveats, vec!["c1"]);
        assert_eq!(replay.steps.len(), 1);
    }

    #[test]
    fn notebook_export_dto_deserializes_from_camel_case() {
        let json = serde_json::json!({
            "entries": [{
                "id": "entry-1",
                "caseId": "case-1",
                "author": "a",
                "entryType": "observation",
                "title": "T",
                "bodyMarkdown": "B",
                "status": "draft",
                "createdAt": "2026-06-14T12:00:00Z",
                "updatedAt": "2026-06-14T12:00:00Z"
            }],
            "citations": [{
                "id": "cite-1",
                "entryId": "entry-1",
                "targetNodeType": "file",
                "targetNodeId": "node-1",
                "displayLabel": "L",
                "citedAt": "2026-06-14T12:00:00Z"
            }],
            "threadGraph": [{
                "sourceEntryId": "entry-1",
                "targetEntryId": "entry-2",
                "label": "follows"
            }]
        });

        let export: NotebookExportDto = serde_json::from_value(json).unwrap();

        assert_eq!(export.entries.len(), 1);
        assert_eq!(
            export.entries[0].entry_type,
            NotebookEntryTypeDto::Observation
        );
        assert_eq!(export.citations.len(), 1);
        assert_eq!(export.thread_graph.len(), 1);
        assert_eq!(export.thread_graph[0].label, "follows");
    }
}
