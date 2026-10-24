use crate::state::AppState;
use tauri::State;
use transport::{
    dto::{
        CarvedArtifactPageDto, CarvedExportDto, CarvedExportRequestDto, CarvedRangeDto,
        CarvedRangeRequestDto, CarvedSearchRequestDto,
    },
    CommandError,
};

use super::command_support::run_case_read_command;

#[tauri::command]
pub async fn read_carved_artifact_range(
    state: State<'_, AppState>,
    request: CarvedRangeRequestDto,
) -> Result<CarvedRangeDto, CommandError> {
    let app_state = state.inner().clone();
    run_case_read_command(app_state, move |conn, active| {
        app_services::carving_service::read_carved_artifact_range(
            conn,
            &active.case_root,
            &active.meta.id,
            &request,
        )
    })
    .await
}
#[tauri::command]
pub async fn export_carved_artifact(
    state: State<'_, AppState>,
    request: CarvedExportRequestDto,
) -> Result<CarvedExportDto, CommandError> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let active = crate::commands::command_support::require_active_case(&state)?;
        let conn =
            app_services::connection::open_existing_case_db(&active.case_root.join("app.db"))
                .map_err(CommandError::from_typed_service_error)?;
        app_services::carving_service::export_carved_artifact(
            &conn,
            &active.case_root,
            &domain::CaseId(active.case_id),
            &request,
        )
    })
    .await
    .map_err(CommandError::from_join_error)?
}
#[tauri::command]
pub async fn search_carved_artifacts(
    state: State<'_, AppState>,
    request: CarvedSearchRequestDto,
) -> Result<CarvedArtifactPageDto, CommandError> {
    let app_state = state.inner().clone();
    run_case_read_command(app_state, move |conn, active| {
        app_services::carving_service::search_carved_artifacts(
            conn,
            &active.case_root,
            &active.meta.id,
            &request,
        )
    })
    .await
}
