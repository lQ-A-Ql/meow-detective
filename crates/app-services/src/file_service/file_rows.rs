use crate::file_service::{mapping::file_entry_to_dto, sort::sort_entries, FileServiceError};
use domain::FileEntryId;
use persistence_sqlite::repositories::file_repo::FileRepo;
use rusqlite::Connection;
use transport::{commands::GetFileRowsRequest, dto::FileRowsPageDto};

pub fn get_file_rows_for_request(
    conn: &Connection,
    request: &GetFileRowsRequest,
) -> Result<FileRowsPageDto, FileServiceError> {
    let mut request = request.clone();
    request.validate().map_err(FileServiceError::InvalidInput)?;
    let repo = FileRepo::new(conn);

    let mut entries = match request.parent_id.as_deref() {
        Some(parent_id) => {
            let parent = repo.find_by_id(&FileEntryId(parent_id.to_string()))?;
            match parent {
                Some(entry) if entry.entry_type == domain::EntryType::Directory => {
                    repo.find_children_visible(&entry.id, request.show_hidden)?
                }
                _ => Vec::new(),
            }
        }
        None => repo.find_root_entries_visible(request.show_hidden)?,
    };

    let total_count = entries.len() as u64;
    sort_entries(&mut entries, request.sort_key, request.sort_direction);

    let start = (request.offset as usize).min(entries.len());
    let end = start
        .saturating_add(request.limit as usize)
        .min(entries.len());
    let page = &entries[start..end];

    Ok(FileRowsPageDto {
        rows: page.iter().map(file_entry_to_dto).collect(),
        total_count,
        offset: request.offset,
        limit: request.limit,
        truncated: request.offset + (request.limit as u64) < total_count,
    })
}
