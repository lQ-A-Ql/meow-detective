use crate::file_service::{
    metadata::{lookup::file_entry_to_dto, sorting::sort_entries},
    FileServiceError,
};
use domain::FileEntryId;
use persistence_source::repositories::file_repo::{FileRepo, FileSortDirection, FileSortKey};
use rusqlite::Connection;
use transport::{commands::GetFileRowsRequest, dto::FileRowsPageDto};

pub fn get_file_rows_for_request(
    conn: &Connection,
    request: &GetFileRowsRequest,
) -> Result<FileRowsPageDto, FileServiceError> {
    if let Some((case_root, source_id)) = crate::file_service::fjall_read::source_context(conn) {
        if let Some(page) = crate::file_service::fjall_read::rows(&case_root, &source_id, request)?
        {
            return Ok(page);
        }
    }
    let mut request = request.clone();
    request.validate().map_err(FileServiceError::InvalidInput)?;
    let repo = FileRepo::new(conn);

    let parent_entry = match request.parent_id.as_deref() {
        Some(parent_id) => {
            let parent = repo.find_by_id(&FileEntryId(parent_id.to_string()))?;
            match parent {
                Some(entry) if entry.entry_type == domain::EntryType::Directory => Some(entry),
                _ => {
                    return Ok(FileRowsPageDto {
                        rows: Vec::new(),
                        total_count: 0,
                        offset: request.offset,
                        limit: request.limit,
                        truncated: false,
                    })
                }
            }
        }
        None => None,
    };
    let parent_id = parent_entry.as_ref().map(|entry| &entry.id);
    let total_count = repo.count_entries_visible(parent_id, request.show_hidden)?;
    let sort_key = match request.sort_key {
        transport::commands::FileSortKeyDto::Name => FileSortKey::Name,
        transport::commands::FileSortKeyDto::Size => FileSortKey::Size,
        transport::commands::FileSortKeyDto::ModifiedAt => FileSortKey::ModifiedAt,
        transport::commands::FileSortKeyDto::Ext => FileSortKey::Ext,
    };
    let sort_direction = match request.sort_direction {
        transport::commands::FileSortDirectionDto::Asc => FileSortDirection::Asc,
        transport::commands::FileSortDirectionDto::Desc => FileSortDirection::Desc,
    };
    let page = if repo.entries_need_natural_sort(parent_id, request.show_hidden, sort_key)? {
        let mut entries = match parent_id {
            Some(parent_id) => repo.find_children_visible(parent_id, request.show_hidden)?,
            None => repo.find_root_entries_visible(request.show_hidden)?,
        };
        sort_entries(&mut entries, request.sort_key, request.sort_direction);
        let start = (request.offset as usize).min(entries.len());
        let end = start
            .saturating_add(request.limit as usize)
            .min(entries.len());
        entries[start..end].to_vec()
    } else {
        repo.find_entries_page_visible_ordered(
            parent_id,
            request.offset,
            request.limit,
            request.show_hidden,
            sort_key,
            sort_direction,
        )?
    };

    Ok(FileRowsPageDto {
        rows: page.iter().map(file_entry_to_dto).collect(),
        total_count,
        offset: request.offset,
        limit: request.limit,
        truncated: request.offset + (request.limit as u64) < total_count,
    })
}
