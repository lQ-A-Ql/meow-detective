//! Plugin analysis module commands (M2.5): generic per-plugin module listing
//! and paged family entries. Thin wrappers over `analysis_service`.

use super::super::support::{run_active_case_command, validate_source_request};
use crate::state::AppState;
use app_services::analysis_service;
use domain::DataSourceId;
use tauri::State;
use transport::{
    commands::{GetAnalysisSourceRequest, GetPluginFamilyEntriesRequest},
    dto::{PluginFamilyEntriesDto, PluginModuleDto},
    CommandError,
};

#[tauri::command]
pub async fn list_plugin_modules(
    state: State<'_, AppState>,
    request: GetAnalysisSourceRequest,
) -> Result<Vec<PluginModuleDto>, CommandError> {
    validate_source_request(&request)?;
    let app_state = state.inner().clone();
    let data_source_id = DataSourceId(request.data_source_id);

    run_active_case_command(app_state, move |case_conn, active| {
        analysis_service::get_source_plugin_modules(
            case_conn,
            &active.case_root,
            &active.meta.id,
            &data_source_id,
        )
        .map_err(CommandError::from_typed_service_error)
    })
    .await
}

#[tauri::command]
pub async fn get_plugin_family_entries(
    state: State<'_, AppState>,
    request: Option<GetPluginFamilyEntriesRequest>,
) -> Result<PluginFamilyEntriesDto, CommandError> {
    let mut request = request.unwrap_or_default();
    request.validate().map_err(CommandError::invalid_input)?;
    let data_source_id = DataSourceId(request.data_source_id);
    let app_state = state.inner().clone();

    run_active_case_command(app_state, move |case_conn, active| {
        analysis_service::get_source_plugin_family_entries(
            case_conn,
            &active.case_root,
            &active.meta.id,
            &data_source_id,
            &request.plugin_id,
            &request.family,
            request.offset,
            request.limit,
        )
        .map_err(CommandError::from_typed_service_error)
    })
    .await
}
