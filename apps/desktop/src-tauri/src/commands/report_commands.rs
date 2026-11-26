use super::command_support::{get_case_read_connection, require_active_case};
use crate::{
    analysis_task_jobs::{self, AnalysisTaskOutcome, AnalysisTaskRequest},
    state::AppState,
};
use tauri::{AppHandle, State};
use transport::{
    commands::{ExportReportRequest, SaveReportRequest},
    dto::ReportHistoryItemDto,
    CommandError,
};

#[tauri::command]
pub async fn get_report_history(
    state: State<'_, AppState>,
    case_id: String,
) -> Result<Vec<ReportHistoryItemDto>, CommandError> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let active = require_active_case(&state)?;
        if active.case_id != case_id {
            return Err(CommandError::conflict("案件已经切换"));
        }
        let connection = get_case_read_connection(&state)?;
        app_services::report::get_report_history(&connection, &case_id)
            .map_err(CommandError::from_typed_service_error)
    })
    .await
    .map_err(CommandError::from_join_error)?
}

#[tauri::command]
pub async fn export_report(
    app: AppHandle,
    state: State<'_, AppState>,
    request: ExportReportRequest,
) -> Result<ReportHistoryItemDto, CommandError> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let active = require_active_case(&state)?;
        if request.case_id != active.case_id {
            return Err(CommandError::conflict("案件已经切换"));
        }
        let case_id = active.case_id.clone();
        let case_root = active.case_root.clone();
        let source_id = request.data_source_id.clone();
        analysis_task_jobs::run_with_outcome(
            &state,
            &app,
            AnalysisTaskRequest {
                case_id: &case_id,
                case_root: &case_root,
                data_source_id: source_id.as_deref(),
                kind: "Report export",
            },
            move |connection, cancel| {
                let result = app_services::report::export_report_for_case(
                    app_services::report::ReportExportContext {
                        connection,
                        case: &active.meta,
                        case_root: &active.case_root,
                        request: &request,
                        cancel: &cancel,
                    },
                )
                .map_err(CommandError::from_typed_service_error)?;
                Ok(AnalysisTaskOutcome::with_counts(result, 0, 0))
            },
        )
    })
    .await
    .map_err(CommandError::from_join_error)?
}

#[tauri::command]
pub async fn save_report(
    state: State<'_, AppState>,
    request: SaveReportRequest,
) -> Result<(), CommandError> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let active = require_active_case(&state)?;
        if request.case_id != active.case_id {
            return Err(CommandError::conflict("案件已经切换"));
        }
        let connection = get_case_read_connection(&state)?;
        app_services::report::save_report_for_case(&connection, &active.case_root, &request)
            .map_err(CommandError::from_typed_service_error)
    })
    .await
    .map_err(CommandError::from_join_error)?
}
