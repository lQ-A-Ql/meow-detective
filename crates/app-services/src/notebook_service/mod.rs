//! Notebook service: create/update/list notebook entries, manage evidence citations,
//! and record/list investigation steps. Uses `NotebookRepo` for persistence and
//! converts between domain types and transport DTOs.

pub mod error;
mod request_filters;
pub use error::NotebookError;
pub use request_filters::{list_entries_for_request, list_steps_for_request};

use domain::{EntryStatus, EvidenceCitation, NodeType, NotebookEntry, NotebookEntryType};
use persistence_sqlite::repositories::notebook_repo::{
    InvestigationStep, NotebookEntryFilters, NotebookRepo, StepFilters,
};
use rusqlite::Connection;
use transport::dto::{
    EvidenceCitationDto, GraphNodeTypeDto, InvestigationStepDto, NotebookEntryDto,
    NotebookEntryStatusDto, NotebookEntryTypeDto,
};
use uuid::Uuid;

fn entry_type_to_dto(et: &NotebookEntryType) -> NotebookEntryTypeDto {
    match et {
        NotebookEntryType::Observation => NotebookEntryTypeDto::Observation,
        NotebookEntryType::Hypothesis => NotebookEntryTypeDto::Hypothesis,
        NotebookEntryType::Finding => NotebookEntryTypeDto::Finding,
        NotebookEntryType::ActionItem => NotebookEntryTypeDto::ActionItem,
        NotebookEntryType::Conclusion => NotebookEntryTypeDto::Conclusion,
    }
}

fn entry_type_from_dto(dto: &NotebookEntryTypeDto) -> NotebookEntryType {
    match dto {
        NotebookEntryTypeDto::Observation => NotebookEntryType::Observation,
        NotebookEntryTypeDto::Hypothesis => NotebookEntryType::Hypothesis,
        NotebookEntryTypeDto::Finding => NotebookEntryType::Finding,
        NotebookEntryTypeDto::ActionItem => NotebookEntryType::ActionItem,
        NotebookEntryTypeDto::Conclusion => NotebookEntryType::Conclusion,
    }
}

fn status_to_dto(s: &EntryStatus) -> NotebookEntryStatusDto {
    match s {
        EntryStatus::Draft => NotebookEntryStatusDto::Draft,
        EntryStatus::Reviewed => NotebookEntryStatusDto::Reviewed,
        EntryStatus::Final => NotebookEntryStatusDto::Final,
    }
}

fn status_from_dto(dto: &NotebookEntryStatusDto) -> EntryStatus {
    match dto {
        NotebookEntryStatusDto::Draft => EntryStatus::Draft,
        NotebookEntryStatusDto::Reviewed => EntryStatus::Reviewed,
        NotebookEntryStatusDto::Final => EntryStatus::Final,
    }
}

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

fn node_type_from_dto(dto: &GraphNodeTypeDto) -> NodeType {
    match dto {
        GraphNodeTypeDto::File => NodeType::File,
        GraphNodeTypeDto::Artifact => NodeType::Artifact,
        GraphNodeTypeDto::TimelineEvent => NodeType::TimelineEvent,
        GraphNodeTypeDto::Entity => NodeType::Entity,
        GraphNodeTypeDto::Lead => NodeType::Lead,
        GraphNodeTypeDto::NotebookEntry => NodeType::NotebookEntry,
    }
}

fn entry_to_dto(entry: &NotebookEntry) -> NotebookEntryDto {
    NotebookEntryDto {
        id: entry.id.clone(),
        case_id: entry.case_id.clone(),
        parent_id: entry.parent_id.clone(),
        author: entry.author.clone(),
        entry_type: entry_type_to_dto(&entry.entry_type),
        title: entry.title.clone(),
        body_markdown: entry.body_markdown.clone(),
        tags: entry.tags.clone(),
        status: status_to_dto(&entry.status),
        created_at: entry.created_at.clone(),
        updated_at: entry.updated_at.clone(),
    }
}

fn citation_to_dto(c: &EvidenceCitation) -> EvidenceCitationDto {
    EvidenceCitationDto {
        id: c.id.clone(),
        entry_id: c.entry_id.clone(),
        target_node_type: node_type_to_dto(&c.target_node_type),
        target_node_id: c.target_node_id.clone(),
        display_label: c.display_label.clone(),
        snippet: c.snippet.clone(),
        cited_at: c.cited_at.clone(),
    }
}

fn step_to_dto(s: &InvestigationStep) -> InvestigationStepDto {
    InvestigationStepDto {
        id: s.id.clone(),
        case_id: s.case_id.clone(),
        step_kind: s.step_kind.clone(),
        params_json: s.params_json.clone(),
        timestamp: s.timestamp.clone(),
        duration_ms: s.duration_ms.unwrap_or(0) as u32,
        case_state_hash: s.case_state_hash.clone(),
        success: s.success.unwrap_or(true),
        error_code: s.error_code.clone(),
    }
}

// ── Public API ───────────────────────────────────────────────────────────

/// Create a new notebook entry and return its DTO.
#[allow(clippy::too_many_arguments)]
pub fn create_entry(
    conn: &Connection,
    case_id: &str,
    author: &str,
    entry_type: &NotebookEntryTypeDto,
    title: &str,
    body_markdown: &str,
    tags: &[String],
    status: &NotebookEntryStatusDto,
    parent_id: Option<&str>,
) -> Result<NotebookEntryDto, NotebookError> {
    let now = chrono::Utc::now().to_rfc3339();
    let id = Uuid::new_v4().to_string();

    let entry = NotebookEntry {
        id,
        case_id: case_id.to_string(),
        parent_id: parent_id.map(|s| s.to_string()),
        author: author.to_string(),
        entry_type: entry_type_from_dto(entry_type),
        title: title.to_string(),
        body_markdown: body_markdown.to_string(),
        tags: tags.to_vec(),
        status: status_from_dto(status),
        created_at: now.clone(),
        updated_at: now,
    };

    let repo = NotebookRepo::new(conn);
    repo.create_entry(&entry)?;

    let created = repo
        .get_entry(&entry.id)?
        .ok_or_else(|| NotebookError::NotFound("entry not found after creation".to_string()))?;

    Ok(entry_to_dto(&created))
}

/// Update an existing notebook entry (partial update) and return its DTO.
///
/// Only the provided fields are updated; others are left unchanged.
pub fn update_entry(
    conn: &Connection,
    entry_id: &str,
    title: Option<&str>,
    body_markdown: Option<&str>,
    tags: Option<&[String]>,
    status: Option<&NotebookEntryStatusDto>,
) -> Result<NotebookEntryDto, NotebookError> {
    let now = chrono::Utc::now().to_rfc3339();
    let repo = NotebookRepo::new(conn);

    let domain_status: Option<EntryStatus> = status.map(status_from_dto);

    repo.update_entry(
        entry_id,
        title,
        body_markdown,
        tags,
        domain_status.as_ref(),
        &now,
    )?;

    let updated = repo
        .get_entry(entry_id)?
        .ok_or_else(|| NotebookError::NotFound(format!("entry not found: {entry_id}")))?;

    Ok(entry_to_dto(&updated))
}

/// List notebook entries for a case, with optional filters.
pub fn list_entries(
    conn: &Connection,
    case_id: &str,
    filters: &NotebookEntryFilters,
) -> Result<Vec<NotebookEntryDto>, NotebookError> {
    let repo = NotebookRepo::new(conn);
    let entries = repo.list_entries(case_id, filters)?;
    Ok(entries.iter().map(entry_to_dto).collect())
}

/// Retrieve the full conversation thread for a notebook entry.
///
/// Walks up to find the root entry, then uses a recursive CTE to fetch
/// the entire thread (root + all descendants in depth-first order).
pub fn get_thread(
    conn: &Connection,
    entry_id: &str,
) -> Result<Vec<NotebookEntryDto>, NotebookError> {
    let repo = NotebookRepo::new(conn);

    let root_id = find_root_id(&repo, entry_id)?;
    let entries = repo.get_thread(&root_id)?;

    Ok(entries.iter().map(entry_to_dto).collect())
}

/// Add an evidence citation linking a notebook entry to a graph node.
pub fn add_citation(
    conn: &Connection,
    entry_id: &str,
    target_node_type: &GraphNodeTypeDto,
    target_node_id: &str,
    display_label: &str,
    snippet: Option<&str>,
) -> Result<EvidenceCitationDto, NotebookError> {
    let now = chrono::Utc::now().to_rfc3339();
    let id = Uuid::new_v4().to_string();

    let citation = EvidenceCitation {
        id,
        entry_id: entry_id.to_string(),
        target_node_type: node_type_from_dto(target_node_type),
        target_node_id: target_node_id.to_string(),
        display_label: display_label.to_string(),
        snippet: snippet.map(|s| s.to_string()),
        cited_at: now,
    };

    let repo = NotebookRepo::new(conn);
    repo.add_citation(&citation)?;

    Ok(citation_to_dto(&citation))
}

/// Record an investigation step for audit/replay purposes.
///
/// Returns the recorded step as a DTO.
#[allow(clippy::too_many_arguments)]
pub fn record_step(
    conn: &Connection,
    case_id: &str,
    step_kind: &str,
    params_json: &str,
    timestamp: &str,
    duration_ms: u32,
    success: bool,
    error_code: Option<&str>,
    case_state_hash: Option<&str>,
) -> Result<InvestigationStepDto, NotebookError> {
    let id = Uuid::new_v4().to_string();

    let step = InvestigationStep {
        id: id.clone(),
        case_id: case_id.to_string(),
        step_kind: step_kind.to_string(),
        params_json: params_json.to_string(),
        timestamp: timestamp.to_string(),
        duration_ms: Some(duration_ms as i64),
        case_state_hash: case_state_hash.map(|s| s.to_string()),
        success: Some(success),
        error_code: error_code.map(|s| s.to_string()),
    };

    let repo = NotebookRepo::new(conn);
    repo.record_step(&step)?;

    Ok(step_to_dto(&step))
}

/// List investigation steps for a case, with optional filters.
pub fn list_steps(
    conn: &Connection,
    case_id: &str,
    filters: &StepFilters,
) -> Result<Vec<InvestigationStepDto>, NotebookError> {
    let repo = NotebookRepo::new(conn);
    let steps = repo.list_steps(case_id, filters)?;
    Ok(steps.iter().map(step_to_dto).collect())
}

// ── Helpers ──────────────────────────────────────────────────────────────

/// Walk up the parent chain to find the root entry id.
fn find_root_id(repo: &NotebookRepo, entry_id: &str) -> Result<String, NotebookError> {
    let mut current = entry_id.to_string();
    loop {
        let entry = repo
            .get_entry(&current)?
            .ok_or_else(|| NotebookError::NotFound(format!("entry not found: {current}")))?;
        match entry.parent_id {
            Some(parent_id) => current = parent_id,
            None => return Ok(current),
        }
    }
}

// ── Tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
#[path = "../../tests/unit/notebook_service/mod.rs"]
mod tests;
