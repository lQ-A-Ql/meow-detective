//! Notebook service: create/update/list notebook entries, manage evidence citations,
//! and record/list investigation steps. Uses `NotebookRepo` for persistence and
//! converts between domain types and transport DTOs.

pub mod error;
pub use error::NotebookError;

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

// ── Conversion: domain ↔ DTO ────────────────────────────────────────────

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
mod tests {
    use super::*;
    use domain::{CaseId, CaseMeta};
    use persistence_sqlite::repositories::case_repo::CaseRepo;
    use persistence_sqlite::runner;
    use rusqlite::Connection;

    fn setup() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        runner::run_all(&conn).unwrap();
        CaseRepo::new(&conn)
            .create(&CaseMeta {
                id: CaseId("case-1".to_string()),
                name: "Notebook Test Case".to_string(),
                number: None,
                examiner: None,
                notes: None,
                created_at: chrono::Utc::now(),
                updated_at: chrono::Utc::now(),
            })
            .unwrap();
        conn
    }

    #[test]
    fn create_and_retrieve_entry() {
        let conn = setup();
        let dto = create_entry(
            &conn,
            "case-1",
            "investigator",
            &NotebookEntryTypeDto::Finding,
            "Suspicious file found",
            "## Details\n\nFound `cmd.exe` with unexpected hash.",
            &["suspicious".to_string(), "cmd".to_string()],
            &NotebookEntryStatusDto::Draft,
            None,
        )
        .unwrap();

        assert_eq!(dto.case_id, "case-1");
        assert_eq!(dto.author, "investigator");
        assert_eq!(dto.entry_type, NotebookEntryTypeDto::Finding);
        assert_eq!(dto.title, "Suspicious file found");
        assert!(dto.body_markdown.contains("cmd.exe"));
        assert_eq!(dto.tags, vec!["suspicious", "cmd"]);
        assert_eq!(dto.status, NotebookEntryStatusDto::Draft);
        assert!(dto.parent_id.is_none());
    }

    #[test]
    fn create_entry_with_parent() {
        let conn = setup();
        let parent = create_entry(
            &conn,
            "case-1",
            "investigator",
            &NotebookEntryTypeDto::Observation,
            "Parent",
            "Parent body",
            &[],
            &NotebookEntryStatusDto::Draft,
            None,
        )
        .unwrap();

        let child = create_entry(
            &conn,
            "case-1",
            "investigator",
            &NotebookEntryTypeDto::Hypothesis,
            "Child hypothesis",
            "Child body",
            &[],
            &NotebookEntryStatusDto::Draft,
            Some(&parent.id),
        )
        .unwrap();

        assert_eq!(child.parent_id.as_deref(), Some(parent.id.as_str()));
    }

    #[test]
    fn update_entry_fields() {
        let conn = setup();
        let dto = create_entry(
            &conn,
            "case-1",
            "investigator",
            &NotebookEntryTypeDto::Observation,
            "Original title",
            "Original body",
            &["tag-a".to_string()],
            &NotebookEntryStatusDto::Draft,
            None,
        )
        .unwrap();

        let updated = update_entry(
            &conn,
            &dto.id,
            Some("Updated title"),
            Some("Updated body"),
            Some(&["tag-b".to_string()]),
            Some(&NotebookEntryStatusDto::Reviewed),
        )
        .unwrap();

        assert_eq!(updated.id, dto.id);
        assert_eq!(updated.title, "Updated title");
        assert_eq!(updated.body_markdown, "Updated body");
        assert_eq!(updated.tags, vec!["tag-b"]);
        assert_eq!(updated.status, NotebookEntryStatusDto::Reviewed);
    }

    #[test]
    fn update_entry_partial() {
        let conn = setup();
        let dto = create_entry(
            &conn,
            "case-1",
            "investigator",
            &NotebookEntryTypeDto::Observation,
            "Title",
            "Body",
            &["original-tag".to_string()],
            &NotebookEntryStatusDto::Draft,
            None,
        )
        .unwrap();

        // Update only title
        let updated = update_entry(&conn, &dto.id, Some("New title"), None, None, None).unwrap();

        assert_eq!(updated.title, "New title");
        assert_eq!(updated.body_markdown, "Body"); // unchanged
        assert_eq!(updated.tags, vec!["original-tag"]); // unchanged
        assert_eq!(updated.status, NotebookEntryStatusDto::Draft); // unchanged
    }

    #[test]
    fn list_entries_with_filters() {
        let conn = setup();
        create_entry(
            &conn,
            "case-1",
            "a",
            &NotebookEntryTypeDto::Finding,
            "Finding One",
            "body",
            &[],
            &NotebookEntryStatusDto::Draft,
            None,
        )
        .unwrap();
        create_entry(
            &conn,
            "case-1",
            "a",
            &NotebookEntryTypeDto::Observation,
            "Observation ABC",
            "body with searchable text",
            &[],
            &NotebookEntryStatusDto::Final,
            None,
        )
        .unwrap();
        create_entry(
            &conn,
            "case-1",
            "a",
            &NotebookEntryTypeDto::Conclusion,
            "Conclusion XYZ",
            "another body",
            &[],
            &NotebookEntryStatusDto::Draft,
            None,
        )
        .unwrap();

        // All entries
        let all = list_entries(&conn, "case-1", &NotebookEntryFilters::default()).unwrap();
        assert_eq!(all.len(), 3);

        // Filter by entry type
        let filters = NotebookEntryFilters {
            entry_type: Some(NotebookEntryType::Finding),
            ..Default::default()
        };
        let filtered = list_entries(&conn, "case-1", &filters).unwrap();
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].entry_type, NotebookEntryTypeDto::Finding);

        // Filter by status
        let filters = NotebookEntryFilters {
            status: Some(EntryStatus::Final),
            ..Default::default()
        };
        let filtered = list_entries(&conn, "case-1", &filters).unwrap();
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].status, NotebookEntryStatusDto::Final);

        // Search
        let filters = NotebookEntryFilters {
            search: Some("searchable".to_string()),
            ..Default::default()
        };
        let filtered = list_entries(&conn, "case-1", &filters).unwrap();
        assert_eq!(filtered.len(), 1);
        assert!(filtered[0].body_markdown.contains("searchable"));
    }

    #[test]
    fn get_thread_returns_full_parent_chain_and_replies() {
        let conn = setup();
        let root = create_entry(
            &conn,
            "case-1",
            "a",
            &NotebookEntryTypeDto::Observation,
            "Root",
            "root body",
            &[],
            &NotebookEntryStatusDto::Draft,
            None,
        )
        .unwrap();
        let child1 = create_entry(
            &conn,
            "case-1",
            "a",
            &NotebookEntryTypeDto::Hypothesis,
            "Child 1",
            "child1 body",
            &[],
            &NotebookEntryStatusDto::Draft,
            Some(&root.id),
        )
        .unwrap();
        let child2 = create_entry(
            &conn,
            "case-1",
            "a",
            &NotebookEntryTypeDto::Finding,
            "Child 2",
            "child2 body",
            &[],
            &NotebookEntryStatusDto::Draft,
            Some(&child1.id),
        )
        .unwrap();

        // Starting from leaf (child2), should get root + child1 + child2
        let thread = get_thread(&conn, &child2.id).unwrap();
        assert_eq!(thread.len(), 3);
        assert_eq!(thread[0].id, root.id);
        assert_eq!(thread[1].id, child1.id);
        assert_eq!(thread[2].id, child2.id);

        // Starting from root should also work
        let thread = get_thread(&conn, &root.id).unwrap();
        assert_eq!(thread.len(), 3);
    }

    #[test]
    fn add_citation_and_retrieve() {
        let conn = setup();
        let entry = create_entry(
            &conn,
            "case-1",
            "a",
            &NotebookEntryTypeDto::Finding,
            "Entry with citations",
            "body",
            &[],
            &NotebookEntryStatusDto::Draft,
            None,
        )
        .unwrap();

        let citation = add_citation(
            &conn,
            &entry.id,
            &GraphNodeTypeDto::File,
            "node-file-1",
            "cmd.exe hash mismatch",
            Some("SHA256: abcd1234"),
        )
        .unwrap();

        assert_eq!(citation.entry_id, entry.id);
        assert_eq!(citation.target_node_type, GraphNodeTypeDto::File);
        assert_eq!(citation.target_node_id, "node-file-1");
        assert_eq!(citation.display_label, "cmd.exe hash mismatch");
        assert_eq!(citation.snippet.as_deref(), Some("SHA256: abcd1234"));

        // Verify via repo
        let repo = NotebookRepo::new(&conn);
        let citations = repo.list_citations_for_entry(&entry.id).unwrap();
        assert_eq!(citations.len(), 1);
        assert_eq!(citations[0].id, citation.id);
    }

    #[test]
    fn record_and_list_steps() {
        let conn = setup();
        let s1 = record_step(
            &conn,
            "case-1",
            "import",
            r#"{"source":"disk.img"}"#,
            "2026-06-14T12:00:00Z",
            5000,
            true,
            None,
            None,
        )
        .unwrap();
        let s2 = record_step(
            &conn,
            "case-1",
            "search",
            r#"{"query":"malware"}"#,
            "2026-06-14T12:01:00Z",
            150,
            true,
            None,
            Some("abc123hash"),
        )
        .unwrap();
        let s3 = record_step(
            &conn,
            "case-1",
            "artifact_extract",
            r#"{"family":"LNK"}"#,
            "2026-06-14T12:02:00Z",
            0,
            false,
            Some("E_PARSE_FAILED"),
            None,
        )
        .unwrap();

        // List all
        let all = list_steps(&conn, "case-1", &StepFilters::default()).unwrap();
        assert_eq!(all.len(), 3);

        assert_eq!(s1.step_kind, "import");
        assert_eq!(s1.duration_ms, 5000);
        assert!(s1.success);

        assert_eq!(s2.case_state_hash.as_deref(), Some("abc123hash"));

        assert!(!s3.success);
        assert_eq!(s3.error_code.as_deref(), Some("E_PARSE_FAILED"));

        // Filter by step_kind
        let filters = StepFilters {
            step_kind: Some("search".to_string()),
            ..Default::default()
        };
        let filtered = list_steps(&conn, "case-1", &filters).unwrap();
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].step_kind, "search");

        // Filter by success
        let filters = StepFilters {
            success: Some(false),
            ..Default::default()
        };
        let filtered = list_steps(&conn, "case-1", &filters).unwrap();
        assert_eq!(filtered.len(), 1);
        assert!(!filtered[0].success);
    }
}
