use tauri::{AppHandle, State};
use transport::{
    commands::{GetTimelineEventByIdRequest, GetTimelineRequest},
    dto::TimelineEventDto,
    paging::PageResponse,
    CommandError,
};

use super::command_support::{get_case_connection, require_active_case, snapshot_active_case};
use crate::events::event_bridge;
use crate::state::AppState;

/// Get timeline events with optional filtering.
#[tauri::command]
pub async fn get_timeline_events(
    state: State<'_, AppState>,
    app: AppHandle,
    request: Option<GetTimelineRequest>,
) -> Result<PageResponse<TimelineEventDto>, CommandError> {
    let app_state = state.inner().clone();
    let mut req = request.unwrap_or_default();
    req.validate().map_err(CommandError::invalid_input)?;
    tauri::async_runtime::spawn_blocking(move || {
        if snapshot_active_case(&app_state)?.is_none() {
            return Ok(PageResponse {
                total: 0,
                items: vec![],
                next_cursor: None,
            });
        }
        let active = require_active_case(&app_state)?;
        let conn = get_case_connection(&app_state)?;

        let query = app_services::timeline_service::TimelineQuery {
            offset: req.offset,
            limit: req.limit,
            time_start: req.time_start.as_deref(),
            time_end: req.time_end.as_deref(),
            event_type: req.event_type.as_deref(),
            cursor: req.cursor.as_deref(),
        };
        let result = app_services::timeline_service::query_timeline_filtered_for_case_instrumented(
            &conn,
            &active.case_root,
            &active.meta.id,
            query,
        )
        .map_err(CommandError::from_typed_service_error)?;
        event_bridge::emit_performance_report_ready(&app, &result.performance_report);
        Ok(result.page)
    })
    .await
    .map_err(CommandError::from_join_error)?
}

/// Resolve a single timeline event by id.
#[tauri::command]
pub async fn get_timeline_event_by_id(
    state: State<'_, AppState>,
    request: GetTimelineEventByIdRequest,
) -> Result<TimelineEventDto, CommandError> {
    request.validate().map_err(CommandError::invalid_input)?;
    let app_state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        if snapshot_active_case(&app_state)?.is_none() {
            return Err(CommandError::no_active_case());
        }
        let active = require_active_case(&app_state)?;
        let conn = get_case_connection(&app_state)?;
        app_services::timeline_service::get_timeline_event_by_id_for_case(
            &conn,
            &active.case_root,
            &active.meta.id,
            &request.event_id,
        )
        .map_err(CommandError::from_typed_service_error)?
        .ok_or_else(|| CommandError::not_found("Timeline event"))
    })
    .await
    .map_err(CommandError::from_join_error)?
}
