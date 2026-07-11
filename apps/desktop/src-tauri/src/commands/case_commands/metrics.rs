use tauri::State;
use transport::{
    commands::RenameDataSourceRequest,
    dto::{CaseMetricsDto, DataSourceSummaryDto, RecentObjectDto},
    CommandError,
};

use crate::state::AppState;

#[tauri::command]
pub async fn get_case_metrics(state: State<'_, AppState>) -> Result<CaseMetricsDto, CommandError> {
    let app_state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let (db_path, case_root, case_id) = {
            let guard = app_state
                .active_case
                .lock()
                .map_err(|e| CommandError::from_lock_error("Case", e))?;
            match guard.as_ref() {
                Some(active) => (
                    active.db_path(),
                    active.case_root.clone(),
                    active.meta.id.clone(),
                ),
                None => {
                    return Ok(CaseMetricsDto {
                        data_source_count: 0,
                        indexed_file_count: 0,
                        timeline_event_count: 0,
                        artifact_count: 0,
                    })
                }
            }
        };
        let conn = app_services::connection::open_case_db(&db_path)
            .map_err(CommandError::from_typed_service_error)?;
        let metrics =
            app_services::case_service::get_case_metrics_for_case(&conn, &case_root, &case_id)
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
pub async fn get_recent_objects(
    state: State<'_, AppState>,
) -> Result<Vec<RecentObjectDto>, CommandError> {
    let app_state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let Some((db_path, case_root, case_id)) = active_case_query_context(&app_state)? else {
            return Ok(vec![]);
        };
        let conn = app_services::connection::open_case_db(&db_path)
            .map_err(CommandError::from_typed_service_error)?;
        app_services::file_service::get_recent_objects_for_case(&conn, &case_root, &case_id)
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
        let Some((db_path, case_root, case_id)) = active_case_query_context(&app_state)? else {
            return Ok(vec![]);
        };
        let conn = app_services::connection::open_case_db(&db_path)
            .map_err(CommandError::from_typed_service_error)?;
        app_services::file_service::get_data_sources_for_case(&conn, &case_root, &case_id)
            .map_err(CommandError::from_typed_service_error)
    })
    .await
    .map_err(CommandError::from_join_error)?
}

fn active_case_query_context(
    state: &AppState,
) -> Result<Option<(std::path::PathBuf, std::path::PathBuf, domain::CaseId)>, CommandError> {
    let guard = state
        .active_case
        .lock()
        .map_err(|e| CommandError::from_lock_error("Case", e))?;
    Ok(guard.as_ref().map(|active| {
        (
            active.db_path(),
            active.case_root.clone(),
            active.meta.id.clone(),
        )
    }))
}

#[tauri::command]
pub async fn rename_data_source(
    state: State<'_, AppState>,
    request: RenameDataSourceRequest,
) -> Result<(), CommandError> {
    request.validate().map_err(CommandError::invalid_input)?;
    let app_state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let db_path = {
            let guard = app_state
                .active_case
                .lock()
                .map_err(|e| CommandError::from_lock_error("Case", e))?;
            let active = guard.as_ref().ok_or_else(CommandError::no_active_case)?;
            active.db_path()
        };
        let conn = app_services::connection::open_case_db(&db_path)
            .map_err(CommandError::from_typed_service_error)?;
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
