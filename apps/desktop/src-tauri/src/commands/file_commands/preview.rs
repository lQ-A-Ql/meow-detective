use crate::state::AppState;
use app_services::file_service;
use tauri::State;
use transport::{
    dto::{DocumentPreviewDto, ImagePreviewDto, TextPreviewDto},
    CommandError,
};

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

#[tauri::command]
pub async fn get_image_metadata(
    state: State<'_, AppState>,
    file_id: String,
) -> Result<transport::dto::ImageMetadataDto, CommandError> {
    let app_state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let connection = crate::commands::command_support::get_case_connection(&app_state)?;
        let active = crate::commands::command_support::require_active_case(&app_state)?;
        file_service::image_metadata_for_source_case_with_bitlocker(
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
