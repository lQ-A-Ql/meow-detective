use tauri::State;
use transport::{
    commands::GetTimelineRequest, dto::TimelineEventDto, paging::PageResponse, CommandError,
};

use crate::state::AppState;

/// Get timeline events with optional filtering.
#[tauri::command]
pub async fn get_timeline_events(
    state: State<'_, AppState>,
    request: Option<GetTimelineRequest>,
) -> Result<PageResponse<TimelineEventDto>, CommandError> {
    let app_state = state.inner().clone();
    let req = request.unwrap_or_default();
    tauri::async_runtime::spawn_blocking(move || {
        // Short lock: extract db_path, then release
        let db_path = {
            let guard = app_state
                .active_case
                .lock()
                .map_err(|e| CommandError::from_lock_error("Case", e))?;
            match guard.as_ref() {
                Some(active) => active.db_path(),
                None => {
                    return Ok(PageResponse {
                        total: 0,
                        items: vec![],
                    })
                }
            }
        };
        // Guard is now dropped — query with released lock
        let conn = persistence_sqlite::open_or_create(&db_path)
            .map_err(CommandError::from_service_error)?;

        // Use filtered query if any filters are provided
        let has_filters = req.time_start.is_some() || req.time_end.is_some() || req.event_type.is_some();
        if has_filters {
            app_services::timeline_service::query_timeline_filtered(
                &conn,
                req.offset,
                req.limit,
                req.time_start.as_deref(),
                req.time_end.as_deref(),
                req.event_type.as_deref(),
            )
            .map_err(CommandError::from_service_error)
        } else {
            app_services::timeline_service::query_timeline(&conn, req.offset, req.limit)
                .map_err(CommandError::from_service_error)
        }
    })
    .await
    .map_err(CommandError::from_join_error)?
}
