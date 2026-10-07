use std::path::Path;

use domain::{CaseId, DataSourceId, EntryType, FileEntryId};
use persistence_sqlite::repositories::file_repo::FileRepo;
use rusqlite::Connection;
use transport::dto::FileRowsPageDto;

use super::{file_metadata::scoped_row, shared::open_source_for_data_source};
use crate::{file_service::FileServiceError, source_db::GlobalFileId};

pub fn list_file_entries_page_for_case(
    case_conn: &Connection,
    case_root: &Path,
    case_id: &CaseId,
    source_id: &DataSourceId,
    parent_id: Option<&str>,
    offset: u64,
    limit: u32,
) -> Result<FileRowsPageDto, FileServiceError> {
    if limit == 0 || limit > 500 || offset > i64::MAX as u64 {
        return Err(FileServiceError::invalid_input("Invalid file page range"));
    }
    let conn = open_source_for_data_source(case_conn, case_root, case_id, source_id)?;
    let repo = FileRepo::new(&conn);
    let (entries, total_count) = if let Some(parent_id) = parent_id {
        let parent = GlobalFileId::parse(&FileEntryId(parent_id.into()))?;
        if parent.data_source_id != *source_id {
            return Err(FileServiceError::security(
                "Directory source does not match",
            ));
        }
        let entry = repo
            .find_by_id(&parent.local_id)?
            .ok_or_else(|| FileServiceError::not_found("Directory does not exist"))?;
        scoped_row(&entry, source_id)?;
        if entry.entry_type != EntryType::Directory {
            return Err(FileServiceError::invalid_input("Parent is not a directory"));
        }
        (
            repo.find_children_page(&parent.local_id, offset, limit)?,
            repo.count_children(&parent.local_id)?,
        )
    } else {
        (
            repo.find_root_entries_page(offset, limit)?,
            repo.count_root_entries()?,
        )
    };
    let rows = entries
        .iter()
        .map(|entry| scoped_row(entry, source_id))
        .collect::<Result<Vec<_>, _>>()?;
    let truncated = offset.saturating_add(rows.len() as u64) < total_count;
    Ok(FileRowsPageDto {
        rows,
        total_count,
        offset,
        limit,
        truncated,
    })
}
