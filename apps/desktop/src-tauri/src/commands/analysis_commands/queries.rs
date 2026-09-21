use app_services::analysis_service;
use domain::DataSourceId;
use tauri::State;
use transport::{
    commands::{ClassifyFilesRequest, GetAnalysisSourceRequest, GetAndroidPackagesRequest},
    dto::{
        AnalysisFileClassificationDto, AnalysisSystemInfoDto, AndroidAnalysisRunDto,
        AndroidDeviceInfoDto, AndroidPackageSummaryDto, EvidenceClassificationSummaryDto,
        FileClassificationBoardDto, KubernetesClusterSummaryDto,
    },
    CommandError,
};

use super::support::{resolve_sample_size, run_active_case_command, validate_source_request};
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

    run_active_case_command(app_state, move |case_conn, active| {
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

    run_active_case_command(app_state, move |case_conn, active| {
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
    state: State<'_, AppState>,
    request: GetAnalysisSourceRequest,
) -> Result<AndroidAnalysisRunDto, CommandError> {
    validate_source_request(&request)?;
    let app_state = state.inner().clone();
    let source_runtime = analysis_service::AnalysisSourceReadRuntime::with_bitlocker_runtime(
        app_state.bitlocker_runtime.clone(),
    );
    let data_source_id = DataSourceId(request.data_source_id);

    run_active_case_command(app_state, move |case_conn, active| {
        analysis_service::run_source_android_analysis(
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
pub async fn get_android_device_info(
    state: State<'_, AppState>,
    request: GetAnalysisSourceRequest,
) -> Result<AndroidDeviceInfoDto, CommandError> {
    validate_source_request(&request)?;
    let app_state = state.inner().clone();
    let data_source_id = DataSourceId(request.data_source_id);

    run_active_case_command(app_state, move |case_conn, active| {
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

    run_active_case_command(app_state, move |case_conn, active| {
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
pub async fn classify_files(
    state: State<'_, AppState>,
    request: ClassifyFilesRequest,
) -> Result<Vec<AnalysisFileClassificationDto>, CommandError> {
    let sample_size = resolve_sample_size(&request)?;
    let app_state = state.inner().clone();
    let data_source_id = DataSourceId(request.data_source_id);

    run_active_case_command(app_state, move |case_conn, active| {
        analysis_service::classify_source_files(
            case_conn,
            &active.case_root,
            &active.meta.id,
            &data_source_id,
            sample_size,
        )
        .map_err(CommandError::from_typed_service_error)
    })
    .await
}

/// Two-level file classification board: magic families with scenario buckets.
#[tauri::command]
pub async fn get_file_classification_board(
    state: State<'_, AppState>,
    request: ClassifyFilesRequest,
) -> Result<FileClassificationBoardDto, CommandError> {
    let magic_read_limit = resolve_sample_size(&request)?;
    let app_state = state.inner().clone();
    let source_runtime = analysis_service::AnalysisSourceReadRuntime::with_bitlocker_runtime(
        app_state.bitlocker_runtime.clone(),
    );
    let data_source_id = DataSourceId(request.data_source_id);

    run_active_case_command(app_state, move |case_conn, active| {
        analysis_service::get_file_classification_board(
            case_conn,
            &active.case_root,
            &active.meta.id,
            &data_source_id,
            magic_read_limit,
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

    run_active_case_command(app_state, move |case_conn, active| {
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
