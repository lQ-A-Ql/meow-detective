use tauri::{AppHandle, State};
use transport::{
    commands::{
        GetNestedEvidenceLineageRequest, MaterializeNestedEvidenceRequest, RenameDataSourceRequest,
    },
    dto::{CaseMetricsDto, DataSourceSummaryDto, NestedEvidenceLineageDto, RecentObjectDto},
    CommandError,
};

use crate::state::AppState;

#[tauri::command]
pub async fn get_case_metrics(state: State<'_, AppState>) -> Result<CaseMetricsDto, CommandError> {
    let app_state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let Some(active) = crate::commands::command_support::snapshot_active_case(&app_state)?
        else {
            return Ok(CaseMetricsDto {
                data_source_count: 0,
                indexed_file_count: 0,
                timeline_event_count: 0,
                artifact_count: 0,
            });
        };
        let conn = crate::commands::command_support::get_case_read_connection(&app_state)?;
        let metrics = app_services::case_service::get_case_metrics_for_case(
            &conn,
            &active.case_root,
            &active.meta.id,
        )
        .map_err(CommandError::from_typed_service_error)?;
        Ok(CaseMetricsDto {
            data_source_count: metrics.data_source_count,
            indexed_file_count: metrics.indexed_file_count,
            timeline_event_count: metrics.timeline_event_count,
            artifact_count: metrics.artifact_count,
        })
    })
    .await
    .map_err(CommandError::from_join_error)?
}

#[tauri::command]
pub async fn get_nested_evidence_lineage(
    state: State<'_, AppState>,
    request: GetNestedEvidenceLineageRequest,
) -> Result<Vec<NestedEvidenceLineageDto>, CommandError> {
    request.validate().map_err(CommandError::invalid_input)?;
    let app_state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let Some(active) = crate::commands::command_support::snapshot_active_case(&app_state)?
        else {
            return Ok(vec![]);
        };
        let conn = crate::commands::command_support::get_case_read_connection(&app_state)?;
        app_services::nested_evidence_service::list_for_parent(
            &conn,
            &active.meta.id,
            &request.parent_data_source_id,
        )
        .map_err(CommandError::from_typed_service_error)
    })
    .await
    .map_err(CommandError::from_join_error)?
}

#[tauri::command]
pub async fn materialize_nested_evidence(
    state: State<'_, AppState>,
    app: AppHandle,
    request: MaterializeNestedEvidenceRequest,
) -> Result<transport::dto::EvidenceTaskDto, CommandError> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        crate::evidence_task_jobs::start(
            state,
            app,
            transport::dto::EvidenceTaskRequestDto::NestedImport(request),
        )
    })
    .await
    .map_err(CommandError::from_join_error)?
}
#[tauri::command]
pub async fn get_recent_objects(
    state: State<'_, AppState>,
) -> Result<Vec<RecentObjectDto>, CommandError> {
    let app_state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let Some(active) = crate::commands::command_support::snapshot_active_case(&app_state)?
        else {
            return Ok(vec![]);
        };
        let conn = crate::commands::command_support::get_case_read_connection(&app_state)?;
        app_services::file_service::get_recent_objects_for_case(
            &conn,
            &active.case_root,
            &active.meta.id,
        )
        .map_err(CommandError::from_typed_service_error)
    })
    .await
    .map_err(CommandError::from_join_error)?
}

#[tauri::command]
pub async fn get_data_sources(
    state: State<'_, AppState>,
) -> Result<Vec<DataSourceSummaryDto>, CommandError> {
    let app_state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let Some(active) = crate::commands::command_support::snapshot_active_case(&app_state)?
        else {
            return Ok(vec![]);
        };
        let conn = crate::commands::command_support::get_case_read_connection(&app_state)?;
        app_services::file_service::get_data_sources_for_case(
            &conn,
            &active.case_root,
            &active.meta.id,
        )
        .map_err(CommandError::from_typed_service_error)
    })
    .await
    .map_err(CommandError::from_join_error)?
}

#[tauri::command]
pub async fn rename_data_source(
    state: State<'_, AppState>,
    request: RenameDataSourceRequest,
) -> Result<(), CommandError> {
    request.validate().map_err(CommandError::invalid_input)?;
    let app_state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let _active = crate::commands::command_support::require_active_case(&app_state)?;
        let conn = crate::commands::command_support::get_case_connection(&app_state)?;
        app_services::file_service::rename_data_source_real(
            &conn,
            &request.data_source_id,
            &request.name,
        )
        .map_err(CommandError::from_typed_service_error)
    })
    .await
    .map_err(CommandError::from_join_error)?
}
