use std::sync::atomic::AtomicBool;
use tauri::{AppHandle, State};
use transport::{
    commands::{
        GetTimelineEventByIdRequest, GetTimelineFacetsRequest, GetTimelineRequest,
        RebuildTimelineProjectionRequest,
    },
    dto::{
        TimelineEventDto, TimelineFacetsDto, TimelineProjectionStateDto,
        TimelineProjectionStatusDto,
    },
    paging::PageResponse,
    CommandError,
};

use super::command_support::{get_case_read_connection, require_active_case, snapshot_active_case};
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
        let conn = get_case_read_connection(&app_state)?;

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

/// Return server-side timeline totals, distributions, and histogram buckets.
#[tauri::command]
pub async fn get_timeline_facets(
    state: State<'_, AppState>,
    request: Option<GetTimelineFacetsRequest>,
) -> Result<TimelineFacetsDto, CommandError> {
    let app_state = state.inner().clone();
    let mut req = request.unwrap_or(GetTimelineFacetsRequest {
        time_start: None,
        time_end: None,
        event_type: None,
        bucket_count: 60,
    });
    req.validate().map_err(CommandError::invalid_input)?;
    tauri::async_runtime::spawn_blocking(move || {
        if snapshot_active_case(&app_state)?.is_none() {
            return Ok(TimelineFacetsDto {
                total_events: 0,
                start_ts: None,
                end_ts: None,
                event_types: Vec::new(),
                data_sources: Vec::new(),
                histogram: Vec::new(),
            });
        }
        let active = require_active_case(&app_state)?;
        let conn = get_case_read_connection(&app_state)?;
        app_services::timeline_service::get_timeline_facets_for_case(
            &conn,
            &active.case_root,
            &active.meta.id,
            req.time_start.as_deref(),
            req.time_end.as_deref(),
            req.event_type.as_deref(),
            req.bucket_count,
        )
        .map_err(CommandError::from_typed_service_error)
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
        let conn = get_case_read_connection(&app_state)?;
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

/// Read case-wide timeline projection readiness status.
#[tauri::command]
pub async fn get_timeline_projection_status(
    state: State<'_, AppState>,
) -> Result<TimelineProjectionStatusDto, CommandError> {
    let app_state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        if snapshot_active_case(&app_state)?.is_none() {
            return Ok(TimelineProjectionStatusDto {
                state: TimelineProjectionStateDto::Complete,
                sources: Vec::new(),
            });
        }
        let active = require_active_case(&app_state)?;
        let conn = get_case_read_connection(&app_state)?;
        app_services::timeline_service::get_timeline_projection_status(
            &conn,
            &active.case_root,
            &active.meta.id,
        )
        .map_err(CommandError::from_typed_service_error)
    })
    .await
    .map_err(CommandError::from_join_error)?
}

/// Rebuild the timeline projection for one source or all not-ready sources.
#[tauri::command]
pub async fn rebuild_timeline_projection(
    state: State<'_, AppState>,
    app: AppHandle,
    request: Option<RebuildTimelineProjectionRequest>,
) -> Result<TimelineProjectionStatusDto, CommandError> {
    let app_state = state.inner().clone();
    let data_source_id = request.and_then(|req| req.data_source_id);
    tauri::async_runtime::spawn_blocking(move || {
        if snapshot_active_case(&app_state)?.is_none() {
            return Err(CommandError::no_active_case());
        }
        let active = require_active_case(&app_state)?;
        let conn = get_case_read_connection(&app_state)?;
        let cancel = AtomicBool::new(false);
        let status = app_services::timeline_service::rebuild_timeline_projection(
            &conn,
            &active.case_root,
            &active.meta.id,
            data_source_id.as_deref(),
            app_services::timeline_service::TimelineProjectionPolicy::default(),
            &cancel,
        )
        .map_err(CommandError::from_typed_service_error)?;
        let event_count = status
            .sources
            .iter()
            .map(|source| source.inserted_events)
            .sum();
        event_bridge::emit_timeline_updated(&app, event_count);
        Ok(status)
    })
    .await
    .map_err(CommandError::from_join_error)?
}
