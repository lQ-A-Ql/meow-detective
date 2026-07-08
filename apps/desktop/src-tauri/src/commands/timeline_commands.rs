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
            });
        }
        let active = require_active_case(&app_state)?;
        let conn = get_case_connection(&app_state)?;

        let has_filters =
            req.time_start.is_some() || req.time_end.is_some() || req.event_type.is_some();
        if has_filters {
            let result =
                app_services::timeline_service::query_timeline_filtered_for_case_instrumented(
                    &conn,
                    &active.case_root,
                    &active.meta.id,
                    req.offset,
                    req.limit,
                    req.time_start.as_deref(),
                    req.time_end.as_deref(),
                    req.event_type.as_deref(),
                )
                .map_err(CommandError::from_typed_service_error)?;
            event_bridge::emit_performance_report_ready(&app, &result.performance_report);
            Ok(result.page)
        } else {
            let result = app_services::timeline_service::query_timeline_for_case_instrumented(
                &conn,
                &active.case_root,
                &active.meta.id,
                req.offset,
                req.limit,
            )
            .map_err(CommandError::from_typed_service_error)?;
            event_bridge::emit_performance_report_ready(&app, &result.performance_report);
            Ok(result.page)
        }
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
