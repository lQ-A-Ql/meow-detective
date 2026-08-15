use app_services::file_service;
use tauri::State;
use transport::{
    commands::OpenFileHandleRequest,
    dto::{
        DocumentPreviewDto, ImagePreviewDto, TextPreviewDto, ViewerHandleDto,
        ViewerRangeRequestDto, ViewerRangeResponseDto,
    },
    CommandError,
};

use crate::state::AppState;

use super::support::run_active_case_command;

/// Open a file handle for preview (returns handle ID and metadata).
async fn open_file_handle(
    state: State<'_, AppState>,
    file_id: String,
) -> Result<ViewerHandleDto, CommandError> {
    let app_state = state.inner().clone();
    let preview_runtime = app_state.preview_runtime.clone();
    let bitlocker_runtime = app_state.bitlocker_runtime.clone();
    run_active_case_command(app_state, move |connection, active| {
        file_service::open_preview_session_for_case_with_bitlocker(
            &bitlocker_runtime,
            &preview_runtime,
            connection,
            &active.case_root,
            &active.meta.id,
            &file_id,
        )
        .map_err(CommandError::from_typed_service_error)
    })
    .await
}

/// Open a file handle with explicit request.
#[tauri::command]
pub async fn open_file_handle_request(
    state: State<'_, AppState>,
    request: OpenFileHandleRequest,
) -> Result<ViewerHandleDto, CommandError> {
    request.validate().map_err(CommandError::invalid_input)?;
    open_file_handle(state, request.file_id).await
}

/// Close an opaque preview handle and release its prepared reader.
#[tauri::command]
pub fn close_file_handle(
    state: State<'_, AppState>,
    handle_id: String,
) -> Result<bool, CommandError> {
    if handle_id.trim().is_empty() {
        return Err(CommandError::invalid_input("handleId is required"));
    }
    let active = crate::commands::command_support::require_active_case(state.inner())?;
    file_service::close_preview_session_for_case(
        &state.preview_runtime,
        &active.meta.id,
        &handle_id,
    )
    .map_err(CommandError::from_typed_service_error)
}

/// Read a range of bytes from a file (for hex/text viewer).
#[tauri::command]
pub async fn read_file_range(
    state: State<'_, AppState>,
    mut request: ViewerRangeRequestDto,
) -> Result<ViewerRangeResponseDto, CommandError> {
    request.validate().map_err(CommandError::invalid_input)?;
    let app_state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || read_file_range_for_state(&app_state, &request))
        .await
        .map_err(CommandError::from_join_error)?
}

/// Get text preview for a file with encoding detection.
#[tauri::command]
pub async fn get_text_preview(
    state: State<'_, AppState>,
    file_id: String,
    max_bytes: Option<usize>,
) -> Result<TextPreviewDto, CommandError> {
    let app_state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let connection = crate::commands::command_support::get_case_connection(&app_state)?;
        text_preview_for_file(&app_state, &connection, &file_id, max_bytes)
    })
    .await
    .map_err(CommandError::from_join_error)?
}

/// Get a base64-encoded image preview for a file.
#[tauri::command]
pub async fn get_image_preview(
    state: State<'_, AppState>,
    file_id: String,
) -> Result<ImagePreviewDto, CommandError> {
    let app_state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let connection = crate::commands::command_support::get_case_connection(&app_state)?;
        image_preview_for_file(&app_state, &connection, &file_id)
    })
    .await
    .map_err(CommandError::from_join_error)?
}

/// Get a structured document preview (PDF, Office Open XML, SQLite).
#[tauri::command]
pub async fn get_document_preview(
    state: State<'_, AppState>,
    file_id: String,
) -> Result<DocumentPreviewDto, CommandError> {
    let app_state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let connection = crate::commands::command_support::get_case_connection(&app_state)?;
        let active = crate::commands::command_support::require_active_case(&app_state)?;
        file_service::document_preview_for_source_case_with_bitlocker(
            &app_state.bitlocker_runtime,
            &connection,
            &active.case_root,
            &active.meta.id,
            &file_id,
        )
        .map_err(CommandError::from_typed_service_error)
    })
    .await
    .map_err(CommandError::from_join_error)?
}

pub(super) fn read_file_range_for_state(
    state: &AppState,
    request: &ViewerRangeRequestDto,
) -> Result<ViewerRangeResponseDto, CommandError> {
    let connection = crate::commands::command_support::get_case_connection(state)?;
    let active = crate::commands::command_support::require_active_case(state)?;
    file_service::read_preview_session_range_for_case_with_bitlocker(
        &state.bitlocker_runtime,
        &state.preview_runtime,
        &connection,
        &active.case_root,
        &active.meta.id,
        request,
    )
    .map_err(CommandError::from_typed_service_error)
}

pub(super) fn image_preview_for_file(
    state: &AppState,
    connection: &rusqlite::Connection,
    file_id: &str,
) -> Result<ImagePreviewDto, CommandError> {
    let active = crate::commands::command_support::require_active_case(state)?;
    file_service::image_preview_for_source_case_with_bitlocker(
        &state.bitlocker_runtime,
        connection,
        &active.case_root,
        &active.meta.id,
        file_id,
    )
    .map_err(CommandError::from_typed_service_error)
}

pub(super) fn text_preview_for_file(
    state: &AppState,
    connection: &rusqlite::Connection,
    file_id: &str,
    max_bytes: Option<usize>,
) -> Result<TextPreviewDto, CommandError> {
    let active = crate::commands::command_support::require_active_case(state)?;
    file_service::text_preview_for_source_case_with_bitlocker(
        &state.bitlocker_runtime,
        connection,
        &active.case_root,
        &active.meta.id,
        file_id,
        max_bytes,
    )
    .map_err(CommandError::from_typed_service_error)
}
