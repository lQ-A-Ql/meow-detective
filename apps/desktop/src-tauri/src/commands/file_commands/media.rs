use app_services::file_service::{self, MediaPreviewPlan};
use tauri::State;
use transport::{
    dto::{MediaPreviewModeDto, MediaRangeRequestDto, MediaRangeResponseDto, MediaUrlDto},
    CommandError,
};

use crate::state::AppState;

/// Get a bounded media URL for video/audio playback.
#[tauri::command]
pub async fn get_media_url(
    state: State<'_, AppState>,
    file_id: String,
) -> Result<MediaUrlDto, CommandError> {
    let app_state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let connection = crate::commands::command_support::get_case_connection(&app_state)?;
        media_data_url_for_file(&app_state, &connection, &file_id)
    })
    .await
    .map_err(CommandError::from_join_error)?
}

/// Read a bounded raw byte range for media preview.
#[tauri::command]
pub async fn read_media_range(
    state: State<'_, AppState>,
    mut request: MediaRangeRequestDto,
) -> Result<MediaRangeResponseDto, CommandError> {
    request.validate().map_err(CommandError::invalid_input)?;
    let app_state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let connection = crate::commands::command_support::get_case_connection(&app_state)?;
        media_range_for_file(&app_state, &connection, &request)
    })
    .await
    .map_err(CommandError::from_join_error)?
}

pub(super) fn media_data_url_for_file(
    state: &AppState,
    connection: &rusqlite::Connection,
    file_id: &str,
) -> Result<MediaUrlDto, CommandError> {
    let active = crate::commands::command_support::require_active_case(state)?;
    let plan = file_service::media_preview_plan_for_source_case(
        connection,
        &active.case_root,
        &active.meta.id,
        file_id,
    )
    .map_err(CommandError::from_typed_service_error)?;

    match plan {
        MediaPreviewPlan::Inline(dto) => Ok(dto),
        MediaPreviewPlan::Protocol {
            mime_type,
            size,
            can_read_ranges,
        } => protocol_media_url(state, file_id, mime_type, size, can_read_ranges),
    }
}

pub(super) fn media_range_for_file(
    state: &AppState,
    connection: &rusqlite::Connection,
    request: &MediaRangeRequestDto,
) -> Result<MediaRangeResponseDto, CommandError> {
    // Stage 3 compatibility: scoped Tauri media handles are desktop runtime
    // state. Stage 4 can move this adapter behind an app-service port.
    let file_id = crate::media_protocol::resolve_scoped_media_handle(state, &request.handle_id)
        .map_err(CommandError::security)?;
    let active = crate::commands::command_support::require_active_case(state)?;
    file_service::media_range_for_source_case(
        connection,
        &active.case_root,
        &active.meta.id,
        &file_id,
        request,
    )
    .map_err(CommandError::from_typed_service_error)
}

fn protocol_media_url(
    state: &AppState,
    file_id: &str,
    mime_type: String,
    size: u64,
    can_read_ranges: bool,
) -> Result<MediaUrlDto, CommandError> {
    // Stage 3 compatibility: protocol URL construction depends on Tauri-owned
    // short-lived handles and therefore cannot yet be a pure app-service call.
    let scoped_handle = crate::media_protocol::create_scoped_media_handle(state, file_id)
        .map_err(CommandError::security)?;
    Ok(MediaUrlDto {
        mode: MediaPreviewModeDto::Protocol,
        url: Some(crate::media_protocol::media_protocol_url(&scoped_handle)),
        handle_id: Some(scoped_handle),
        mime_type,
        size,
        can_read_ranges,
    })
}
