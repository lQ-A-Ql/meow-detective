use domain::{EntryStatus, EvidenceCitation, NodeType, NotebookEntry, NotebookEntryType};

use super::InvestigationStep;

pub(super) fn entry_type_str(entry_type: &NotebookEntryType) -> &'static str {
    match entry_type {
        NotebookEntryType::Observation => "observation",
        NotebookEntryType::Hypothesis => "hypothesis",
        NotebookEntryType::Finding => "finding",
        NotebookEntryType::ActionItem => "action_item",
        NotebookEntryType::Conclusion => "conclusion",
    }
}

pub(super) fn entry_status_str(status: &EntryStatus) -> &'static str {
    match status {
        EntryStatus::Draft => "draft",
        EntryStatus::Reviewed => "reviewed",
        EntryStatus::Final => "final",
    }
}

pub(super) fn node_type_str(node_type: &NodeType) -> &'static str {
    match node_type {
        NodeType::File => "file",
        NodeType::Artifact => "artifact",
        NodeType::TimelineEvent => "timeline_event",
        NodeType::Entity => "entity",
        NodeType::Lead => "lead",
        NodeType::NotebookEntry => "notebook_entry",
    }
}

pub(super) fn row_to_notebook_entry(row: &rusqlite::Row<'_>) -> rusqlite::Result<NotebookEntry> {
    let tags_str: String = row.get(7)?;
    let tags: Vec<String> = serde_json::from_str(&tags_str).unwrap_or_default();
    Ok(NotebookEntry {
        id: row.get(0)?,
        case_id: row.get(1)?,
        parent_id: row.get(2)?,
        author: row.get(3)?,
        entry_type: parse_entry_type(&row.get::<_, String>(4)?),
        title: row.get(5)?,
        body_markdown: row.get(6)?,
        tags,
        status: parse_entry_status(&row.get::<_, String>(8)?),
        created_at: row.get(9)?,
        updated_at: row.get(10)?,
    })
}

pub(super) fn row_to_citation(row: &rusqlite::Row<'_>) -> rusqlite::Result<EvidenceCitation> {
    Ok(EvidenceCitation {
        id: row.get(0)?,
        entry_id: row.get(1)?,
        target_node_type: parse_node_type(&row.get::<_, String>(2)?),
        target_node_id: row.get(3)?,
        display_label: row.get(4)?,
        snippet: row.get(5)?,
        cited_at: row.get(6)?,
    })
}

pub(super) fn row_to_step(row: &rusqlite::Row<'_>) -> rusqlite::Result<InvestigationStep> {
    Ok(InvestigationStep {
        id: row.get(0)?,
        case_id: row.get(1)?,
        step_kind: row.get(2)?,
        params_json: row.get(3)?,
        timestamp: row.get(4)?,
        duration_ms: row.get(5)?,
        case_state_hash: row.get(6)?,
        success: row.get::<_, Option<i32>>(7)?.map(|value| value != 0),
        error_code: row.get(8)?,
    })
}

fn parse_entry_type(value: &str) -> NotebookEntryType {
    match value {
        "observation" => NotebookEntryType::Observation,
        "hypothesis" => NotebookEntryType::Hypothesis,
        "finding" => NotebookEntryType::Finding,
        "action_item" => NotebookEntryType::ActionItem,
        "conclusion" => NotebookEntryType::Conclusion,
        _ => NotebookEntryType::Observation,
    }
}

fn parse_entry_status(value: &str) -> EntryStatus {
    match value {
        "draft" => EntryStatus::Draft,
        "reviewed" => EntryStatus::Reviewed,
        "final" => EntryStatus::Final,
        "deleted" => EntryStatus::Draft,
        _ => EntryStatus::Draft,
    }
}

fn parse_node_type(value: &str) -> NodeType {
    match value {
        "file" => NodeType::File,
        "artifact" => NodeType::Artifact,
        "timeline_event" => NodeType::TimelineEvent,
        "entity" => NodeType::Entity,
        "lead" => NodeType::Lead,
        "notebook_entry" => NodeType::NotebookEntry,
        _ => NodeType::Entity,
    }
}
