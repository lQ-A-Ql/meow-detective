//! File browsing and preview commands.
//!
//! Provides Tauri commands for:
//! - File tree navigation
//! - File listing and details
//! - File handle management for preview
//! - File range reading for hex/text viewer

use app_services::file_service;
use tauri::State;
use transport::{
    commands::{GetFileChildrenRequest, GetFileRowsRequest},
    dto::{FileEntryRowDto, FileTreeNodeDto, ViewerHandleDto, ViewerRangeResponseDto},
    CommandError,
};

use crate::state::AppState;

/// Get children of a file tree node (lazy loading).
#[tauri::command]
pub async fn get_file_children(
    state: State<'_, AppState>,
    parent_id: String,
) -> Result<Vec<FileTreeNodeDto>, CommandError> {
    get_file_children_request(state, GetFileChildrenRequest { parent_id }).await
}

/// Get children of a file tree node with explicit request.
#[tauri::command]
pub async fn get_file_children_request(
    state: State<'_, AppState>,
    request: GetFileChildrenRequest,
) -> Result<Vec<FileTreeNodeDto>, CommandError> {
    let app_state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        // Short lock: extract db_path, then release
        let db_path = {
            let guard = app_state
                .active_case
                .lock()
                .map_err(|e| CommandError::from_lock_error("Case", e))?;
            match guard.as_ref() {
                Some(active) => active.db_path(),
                None => return Ok(vec![]),
            }
        };
        // Guard is now dropped — query with released lock
        let conn = persistence_sqlite::open_or_create(&db_path)
            .map_err(CommandError::from_service_error)?;
        file_service::get_file_children_lazy(&conn, &request.parent_id)
            .map(|result| result.children)
            .map_err(CommandError::from_service_error)
    })
    .await
    .map_err(CommandError::from_join_error)?
}

/// Get the complete file tree for the current case.
#[tauri::command]
pub async fn get_file_tree(
    state: State<'_, AppState>,
) -> Result<Vec<FileTreeNodeDto>, CommandError> {
    let app_state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        // Short lock: extract db_path, then release
        let db_path = {
            let guard = app_state
                .active_case
                .lock()
                .map_err(|e| CommandError::from_lock_error("Case", e))?;
            match guard.as_ref() {
                Some(active) => active.db_path(),
                None => return Ok(vec![]),
            }
        };
        // Guard is now dropped — query with released lock
        let conn = persistence_sqlite::open_or_create(&db_path)
            .map_err(CommandError::from_service_error)?;
        file_service::get_file_tree_real(&conn).map_err(CommandError::from_service_error)
    })
    .await
    .map_err(CommandError::from_join_error)?
}

/// Get file rows for display in table view.
#[tauri::command]
pub async fn get_file_rows(
    state: State<'_, AppState>,
) -> Result<Vec<FileEntryRowDto>, CommandError> {
    get_file_rows_request(state, GetFileRowsRequest::default()).await
}

/// Get file rows with explicit request parameters.
#[tauri::command]
pub async fn get_file_rows_request(
    state: State<'_, AppState>,
    request: GetFileRowsRequest,
) -> Result<Vec<FileEntryRowDto>, CommandError> {
    let app_state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        // Short lock: extract db_path, then release
        let db_path = {
            let guard = app_state
                .active_case
                .lock()
                .map_err(|e| CommandError::from_lock_error("Case", e))?;
            match guard.as_ref() {
                Some(active) => active.db_path(),
                None => return Ok(vec![]),
            }
        };
        // Guard is now dropped — query with released lock
        let conn = persistence_sqlite::open_or_create(&db_path)
            .map_err(CommandError::from_service_error)?;
        file_service::get_file_rows_for_request(&conn, &request)
            .map_err(CommandError::from_service_error)
    })
    .await
    .map_err(CommandError::from_join_error)?
}

/// Open a file handle for preview (returns handle ID and metadata).
#[tauri::command]
pub async fn open_file_handle(
    state: State<'_, AppState>,
    file_id: String,
) -> Result<ViewerHandleDto, CommandError> {
    let app_state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        // Short lock: extract db_path, then release
        let db_path = {
            let guard = app_state
                .active_case
                .lock()
                .map_err(|e| CommandError::from_lock_error("Case", e))?;
            let active = guard.as_ref().ok_or_else(CommandError::no_active_case)?;
            active.db_path()
        };
        // Guard is now dropped — query with released lock
        let conn = persistence_sqlite::open_or_create(&db_path)
            .map_err(CommandError::from_service_error)?;
        file_service::open_file_handle_real(&conn, &file_id)
            .map_err(CommandError::from_service_error)
    })
    .await
    .map_err(CommandError::from_join_error)?
}

/// Open a file handle with explicit request.
#[tauri::command]
pub async fn open_file_handle_request(
    state: State<'_, AppState>,
    request: transport::commands::OpenFileHandleRequest,
) -> Result<ViewerHandleDto, CommandError> {
    let app_state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        // Short lock: extract db_path, then release
        let db_path = {
            let guard = app_state
                .active_case
                .lock()
                .map_err(|e| CommandError::from_lock_error("Case", e))?;
            let active = guard.as_ref().ok_or_else(CommandError::no_active_case)?;
            active.db_path()
        };
        // Guard is now dropped — query with released lock
        let conn = persistence_sqlite::open_or_create(&db_path)
            .map_err(CommandError::from_service_error)?;
        file_service::open_file_handle_real(&conn, &request.file_id)
            .map_err(CommandError::from_service_error)
    })
    .await
    .map_err(CommandError::from_join_error)?
}

/// Read a range of bytes from a file (for hex/text viewer).
#[tauri::command]
pub async fn read_file_range(
    state: State<'_, AppState>,
    request: transport::dto::ViewerRangeRequestDto,
) -> Result<ViewerRangeResponseDto, CommandError> {
    let app_state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        // Short lock: extract db_path, then release
        let db_path = {
            let guard = app_state
                .active_case
                .lock()
                .map_err(|e| CommandError::from_lock_error("Case", e))?;
            match guard.as_ref() {
                Some(active) => active.db_path(),
                None => return Ok(file_service::read_file_range_real(&request)),
            }
        };
        // Guard is now dropped — query with released lock
        let conn = persistence_sqlite::open_or_create(&db_path)
            .map_err(CommandError::from_service_error)?;
        file_service::read_file_range_for_case(&conn, &request)
            .map_err(CommandError::from_service_error)
    })
    .await
    .map_err(CommandError::from_join_error)?
}
