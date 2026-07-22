use app_services::analysis_service;
use domain::DataSourceId;
use tauri::State;
use transport::{
    commands::{
        GetAnalysisExtractionRequest, GetAnalysisSourceRequest, GetEvtxEventSummaryRequest,
    },
    dto::{
        BrowserHistorySummaryDto, EmailExtractionSummaryDto, EvtxEventSummaryDto,
        LinuxArtifactSummaryDto, RegistryExtractionSummaryDto, RegistryStructuredSummaryDto,
    },
    CommandError,
};

use super::super::support::{run_active_case_command, validate_source_request};
use crate::state::AppState;

#[tauri::command]
pub async fn get_registry_extraction_summary(
    state: State<'_, AppState>,
    request: Option<GetAnalysisExtractionRequest>,
) -> Result<RegistryExtractionSummaryDto, CommandError> {
    let app_state = state.inner().clone();
    let mut request = request.unwrap_or_default();
    request.validate().map_err(CommandError::invalid_input)?;
    let data_source_id = DataSourceId(request.data_source_id);

    run_active_case_command(app_state, move |case_conn, active| {
        analysis_service::get_source_registry_summary(
            case_conn,
            &active.case_root,
            &active.meta.id,
            &data_source_id,
            request.offset,
            request.limit,
        )
        .map_err(CommandError::from_typed_service_error)
    })
    .await
}

#[tauri::command]
pub async fn get_registry_structured_summary(
    state: State<'_, AppState>,
    request: GetAnalysisSourceRequest,
) -> Result<RegistryStructuredSummaryDto, CommandError> {
    validate_source_request(&request)?;
    let app_state = state.inner().clone();
    let data_source_id = DataSourceId(request.data_source_id);

    run_active_case_command(app_state, move |case_conn, active| {
        analysis_service::get_source_registry_structured_summary(
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
pub async fn get_browser_history_summary(
    state: State<'_, AppState>,
    request: Option<GetAnalysisExtractionRequest>,
) -> Result<BrowserHistorySummaryDto, CommandError> {
    let mut request = request.unwrap_or_default();
    request.validate().map_err(CommandError::invalid_input)?;
    let data_source_id = DataSourceId(request.data_source_id);
    let app_state = state.inner().clone();

    run_active_case_command(app_state, move |case_conn, active| {
        analysis_service::get_source_browser_summary(
            case_conn,
            &active.case_root,
            &active.meta.id,
            &data_source_id,
            request.offset,
            request.limit,
        )
        .map_err(CommandError::from_typed_service_error)
    })
    .await
}

#[tauri::command]
pub async fn get_email_extraction_summary(
    state: State<'_, AppState>,
    request: Option<GetAnalysisExtractionRequest>,
) -> Result<EmailExtractionSummaryDto, CommandError> {
    let mut request = request.unwrap_or_default();
    request.validate().map_err(CommandError::invalid_input)?;
    let data_source_id = DataSourceId(request.data_source_id);
    let app_state = state.inner().clone();

    run_active_case_command(app_state, move |case_conn, active| {
        analysis_service::get_source_email_summary(
            case_conn,
            &active.case_root,
            &active.meta.id,
            &data_source_id,
            request.offset,
            request.limit,
        )
        .map_err(CommandError::from_typed_service_error)
    })
    .await
}

#[tauri::command]
pub async fn get_evtx_event_summary(
    state: State<'_, AppState>,
    request: Option<GetEvtxEventSummaryRequest>,
) -> Result<EvtxEventSummaryDto, CommandError> {
    let mut request = request.unwrap_or_default();
    request.validate().map_err(CommandError::invalid_input)?;
    let data_source_id = DataSourceId(request.data_source_id);
    let app_state = state.inner().clone();

    run_active_case_command(app_state, move |case_conn, active| {
        analysis_service::get_source_evtx_summary(
            case_conn,
            &active.case_root,
            &active.meta.id,
            &data_source_id,
            request.view,
            request.offset,
            request.limit,
        )
        .map_err(CommandError::from_typed_service_error)
    })
    .await
}

#[tauri::command]
pub async fn get_linux_artifact_summary(
    state: State<'_, AppState>,
    request: Option<GetAnalysisExtractionRequest>,
) -> Result<LinuxArtifactSummaryDto, CommandError> {
    let mut request = request.unwrap_or_default();
    request.validate().map_err(CommandError::invalid_input)?;
    let data_source_id = DataSourceId(request.data_source_id);
    let app_state = state.inner().clone();

    run_active_case_command(app_state, move |case_conn, active| {
        analysis_service::get_source_linux_summary(
            case_conn,
            &active.case_root,
            &active.meta.id,
            &data_source_id,
            request.offset,
            request.limit,
        )
        .map_err(CommandError::from_typed_service_error)
    })
    .await
}
