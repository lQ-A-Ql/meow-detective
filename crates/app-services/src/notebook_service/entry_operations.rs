use domain::{EntryStatus, NotebookEntry};
use persistence_sqlite::repositories::notebook_repo::{NotebookEntryFilters, NotebookRepo};
use rusqlite::Connection;
use transport::dto::{NotebookEntryDto, NotebookEntryStatusDto, NotebookEntryTypeDto};
use uuid::Uuid;

use super::dto_conversion::{entry_to_dto, entry_type_from_dto, status_from_dto};
use super::NotebookError;

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
        parent_id: parent_id.map(str::to_string),
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
