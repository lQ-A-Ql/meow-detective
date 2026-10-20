use domain::{EntryType, FileEntry, FileEntryId};
use persistence_sqlite::repositories::file_repo::FileRepo;
use rusqlite::Connection;
use transport::{commands::GetFileJumpContextRequest, dto::FileJumpContextDto};

use crate::file_service::{
    metadata::{
        lookup::{file_entry_to_dto, file_entry_to_tree_node},
        sorting::sort_entries,
    },
    FileServiceError,
};

const JUMP_CONTEXT_MAX_ANCESTOR_DEPTH: usize = 256;

pub fn get_file_jump_context(
    conn: &Connection,
    request: &GetFileJumpContextRequest,
) -> Result<FileJumpContextDto, FileServiceError> {
    let mut request = request.clone();
    request.validate().map_err(FileServiceError::InvalidInput)?;
    let repo = FileRepo::new(conn);
    let target = repo
        .find_by_id(&FileEntryId(request.file_id.clone()))?
        .ok_or_else(|| FileServiceError::not_found("file not found"))?;
    let directory = resolve_jump_directory(&repo, &target)
        .ok_or_else(|| FileServiceError::not_found("directory not found"))?;

    let requires_show_hidden =
        target.hidden || target.system || directory.hidden || directory.system;
    let effective_show_hidden = request.show_hidden || requires_show_hidden;
    let (ancestor_directories, ancestors_complete) =
        repo.find_ancestor_chain(&directory.id, JUMP_CONTEXT_MAX_ANCESTOR_DEPTH)?;
    let ancestor_directory_ids = ancestor_directories
        .iter()
        .map(|entry| entry.id.0.clone())
        .collect::<Vec<_>>();
    let ancestor_nodes = ancestor_directories
        .iter()
        .enumerate()
        .map(|(depth, entry)| file_entry_to_tree_node(entry, depth as u32, Some(true)))
        .collect::<Vec<_>>();
    let mut rows = jump_rows(&repo, &target, &directory, effective_show_hidden)?;
    sort_entries(&mut rows, request.sort_key, request.sort_direction);
    let index = rows
        .iter()
        .position(|entry| entry.id == target.id)
        .unwrap_or(0);
    let row_offset = ((index as u64) / request.page_limit as u64) * request.page_limit as u64;

    Ok(FileJumpContextDto {
        target: file_entry_to_dto(&target),
        directory: file_entry_to_dto(&directory),
        ancestor_directory_ids,
        ancestor_nodes,
        ancestors_complete,
        row_offset,
        requires_show_hidden,
    })
}

fn jump_rows(
    repo: &FileRepo<'_>,
    target: &FileEntry,
    directory: &FileEntry,
    show_hidden: bool,
) -> Result<Vec<FileEntry>, FileServiceError> {
    if target.entry_type == EntryType::Directory {
        return match target.parent_id.as_ref() {
            Some(parent_id) => Ok(repo.find_children_visible(parent_id, show_hidden)?),
            None => Ok(repo.find_root_entries_visible(show_hidden)?),
        };
    }
    Ok(repo.find_children_visible(&directory.id, show_hidden)?)
}

fn resolve_jump_directory(repo: &FileRepo<'_>, target: &FileEntry) -> Option<FileEntry> {
    if target.entry_type == EntryType::Directory {
        return Some(target.clone());
    }
    repo.find_by_id(target.parent_id.as_ref()?).ok()?
}
