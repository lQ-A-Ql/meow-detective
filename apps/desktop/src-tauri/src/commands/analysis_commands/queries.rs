use app_services::analysis_service;
use domain::DataSourceId;
use tauri::{AppHandle, State};
use transport::{
    commands::{GetAnalysisSourceRequest, GetAndroidPackagesRequest},
    dto::{
        AnalysisSystemInfoDto, AndroidAnalysisRunDto, AndroidDeviceInfoDto,
        AndroidPackageSummaryDto, EvidenceClassificationSummaryDto, KubernetesClusterSummaryDto,
    },
    CommandError,
};

use super::support::{run_read_case_command, validate_source_request};
use crate::analysis_task_jobs::{self, AnalysisTaskRequest};
use crate::commands::command_support::require_active_case;
use crate::state::AppState;

#[tauri::command]
pub async fn get_system_info(
    state: State<'_, AppState>,
    request: GetAnalysisSourceRequest,
) -> Result<AnalysisSystemInfoDto, CommandError> {
    validate_source_request(&request)?;
    let app_state = state.inner().clone();
    let source_runtime = analysis_service::AnalysisSourceReadRuntime::with_bitlocker_runtime(
        app_state.bitlocker_runtime.clone(),
    );
    let data_source_id = DataSourceId(request.data_source_id);

    run_read_case_command(app_state, move |case_conn, active| {
        analysis_service::get_source_system_info(
            case_conn,
            &active.case_root,
            &active.meta.id,
            &data_source_id,
            &source_runtime,
        )
        .map_err(CommandError::from_typed_service_error)
    })
    .await
}

#[tauri::command]
pub async fn get_kubernetes_cluster_summary(
    state: State<'_, AppState>,
    request: GetAnalysisSourceRequest,
) -> Result<KubernetesClusterSummaryDto, CommandError> {
    validate_source_request(&request)?;
    let app_state = state.inner().clone();
    let data_source_id = DataSourceId(request.data_source_id);

    run_read_case_command(app_state, move |case_conn, active| {
        app_services::cluster_service::get_source_kubernetes_cluster_summary(
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
pub async fn run_android_analysis(
    app: AppHandle,
    state: State<'_, AppState>,
    request: GetAnalysisSourceRequest,
) -> Result<AndroidAnalysisRunDto, CommandError> {
    validate_source_request(&request)?;
    let app_state = state.inner().clone();
    let source_runtime = analysis_service::AnalysisSourceReadRuntime::with_bitlocker_runtime(
        app_state.bitlocker_runtime.clone(),
    );
    let data_source_id = DataSourceId(request.data_source_id);

    tauri::async_runtime::spawn_blocking(move || {
        let active = require_active_case(&app_state)?;
        let case_id = active.meta.id.0.clone();
        let case_root = active.case_root.clone();
        let worker_case_id = case_id.clone();
        let worker_case_root = case_root.clone();
        let worker_data_source_id = data_source_id.clone();
        analysis_task_jobs::run(
            &app_state,
            &app,
            AnalysisTaskRequest {
                case_id: &case_id,
                case_root: &case_root,
                data_source_id: Some(&data_source_id.0),
                kind: "Android analysis",
            },
            move |case_conn, cancel_token| {
                analysis_service::run_source_android_analysis_with_cancel(
                    case_conn,
                    &worker_case_root,
                    &domain::CaseId(worker_case_id),
                    &worker_data_source_id,
                    &source_runtime,
                    &cancel_token,
                )
                .map_err(CommandError::from_typed_service_error)
            },
        )
    })
    .await
    .map_err(CommandError::from_join_error)?
}

#[tauri::command]
pub async fn get_android_device_info(
    state: State<'_, AppState>,
    request: GetAnalysisSourceRequest,
) -> Result<AndroidDeviceInfoDto, CommandError> {
    validate_source_request(&request)?;
    let app_state = state.inner().clone();
    let data_source_id = DataSourceId(request.data_source_id);

    run_read_case_command(app_state, move |case_conn, active| {
        analysis_service::get_source_android_device_info(
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
pub async fn get_android_package_summary(
    state: State<'_, AppState>,
    mut request: GetAndroidPackagesRequest,
) -> Result<AndroidPackageSummaryDto, CommandError> {
    request.validate().map_err(CommandError::invalid_input)?;
    let app_state = state.inner().clone();
    let source_runtime = analysis_service::AnalysisSourceReadRuntime::with_bitlocker_runtime(
        app_state.bitlocker_runtime.clone(),
    );
    let data_source_id = DataSourceId(request.data_source_id);

    run_read_case_command(app_state, move |case_conn, active| {
        analysis_service::get_source_android_package_summary(
            case_conn,
            &active.case_root,
            &active.meta.id,
            &data_source_id,
            request.offset,
            request.limit,
            &source_runtime,
        )
        .map_err(CommandError::from_typed_service_error)
    })
    .await
}

#[tauri::command]
pub async fn get_evidence_classification_summary(
    state: State<'_, AppState>,
    request: GetAnalysisSourceRequest,
) -> Result<EvidenceClassificationSummaryDto, CommandError> {
    validate_source_request(&request)?;
    let app_state = state.inner().clone();
    let data_source_id = DataSourceId(request.data_source_id);

    run_read_case_command(app_state, move |case_conn, active| {
        analysis_service::get_source_evidence_summary(
            case_conn,
            &active.case_root,
            &active.meta.id,
            &data_source_id,
        )
        .map_err(CommandError::from_typed_service_error)
    })
    .await
}
