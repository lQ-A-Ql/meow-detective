use app_services::file_service;
use tauri::State;
use transport::{
    commands::{
        GetFileChildrenRequest, GetFileJumpContextRequest, GetFileRowsRequest, GetFileTreeRequest,
    },
    dto::{FileChildrenDto, FileEntryRowDto, FileJumpContextDto, FileRowsPageDto, FileTreeNodeDto},
    CommandError,
};

use crate::state::AppState;

use super::support::{run_active_case_command, run_optional_active_case_command};

/// Get children of a file tree node (lazy loading).
#[tauri::command]
pub async fn get_file_children(
    state: State<'_, AppState>,
    parent_id: String,
) -> Result<Vec<FileTreeNodeDto>, CommandError> {
    let page = get_file_children_request(
        state,
        GetFileChildrenRequest {
            parent_id,
            offset: 0,
            limit: infrastructure::constants::MAX_PAGE_LIMIT,
            show_hidden: false,
        },
    )
    .await?;
    Ok(page.children)
}

/// Get children of a file tree node with explicit request.
#[tauri::command]
pub async fn get_file_children_request(
    state: State<'_, AppState>,
    mut request: GetFileChildrenRequest,
) -> Result<FileChildrenDto, CommandError> {
    request.validate().map_err(CommandError::invalid_input)?;
    let empty = FileChildrenDto {
        children: vec![],
        total_count: 0,
        offset: Some(request.offset),
        limit: Some(request.limit),
        truncated: Some(false),
    };

    run_optional_active_case_command(state.inner().clone(), empty, move |connection, active| {
        file_service::get_file_children_for_case(
            connection,
            &active.case_root,
            &active.meta.id,
            &request.parent_id,
            request.offset,
            request.limit,
            request.show_hidden,
        )
        .map_err(CommandError::from_typed_service_error)
    })
    .await
}

/// Get the complete file tree for the current case.
#[tauri::command]
pub async fn get_file_tree(
    state: State<'_, AppState>,
) -> Result<Vec<FileTreeNodeDto>, CommandError> {
    get_file_tree_request(state, GetFileTreeRequest::default()).await
}

/// Get the complete file tree for the current case with explicit visibility.
#[tauri::command]
pub async fn get_file_tree_request(
    state: State<'_, AppState>,
    request: GetFileTreeRequest,
) -> Result<Vec<FileTreeNodeDto>, CommandError> {
    run_optional_active_case_command(state.inner().clone(), vec![], move |connection, active| {
        file_service::get_file_tree_for_case(
            connection,
            &active.case_root,
            &active.meta.id,
            request.show_hidden,
        )
        .map_err(CommandError::from_typed_service_error)
    })
    .await
}

/// Get file rows for display in table view.
#[tauri::command]
pub async fn get_file_rows(
    state: State<'_, AppState>,
) -> Result<Vec<FileEntryRowDto>, CommandError> {
    let page = get_file_rows_request(state, GetFileRowsRequest::default()).await?;
    Ok(page.rows)
}

/// Get file rows with explicit request parameters.
#[tauri::command]
pub async fn get_file_rows_request(
    state: State<'_, AppState>,
    mut request: GetFileRowsRequest,
) -> Result<FileRowsPageDto, CommandError> {
    request.validate().map_err(CommandError::invalid_input)?;
    let empty = FileRowsPageDto {
        rows: vec![],
        total_count: 0,
        offset: request.offset,
        limit: request.limit,
        truncated: false,
    };

    run_optional_active_case_command(state.inner().clone(), empty, move |connection, active| {
        file_service::get_file_rows_for_case(
            connection,
            &active.case_root,
            &active.meta.id,
            &request,
        )
        .map_err(CommandError::from_typed_service_error)
    })
    .await
}

/// Resolve a file jump target into directory context and row page offset.
#[tauri::command]
pub async fn get_file_jump_context(
    state: State<'_, AppState>,
    mut request: GetFileJumpContextRequest,
) -> Result<FileJumpContextDto, CommandError> {
    request.validate().map_err(CommandError::invalid_input)?;

    run_active_case_command(state.inner().clone(), move |connection, active| {
        file_service::get_file_jump_context_for_case(
            connection,
            &active.case_root,
            &active.meta.id,
            &request,
        )
        .map_err(|error| {
            if error.to_string().contains("not found") {
                CommandError::not_found("File")
            } else {
                CommandError::from_typed_service_error(error)
            }
        })
    })
    .await
}
