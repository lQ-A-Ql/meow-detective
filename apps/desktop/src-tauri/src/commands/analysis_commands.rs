//! Data source analysis commands.

use app_services::analysis_service;
use domain::DataSourceId;
use tauri::State;
use transport::{
    commands::{
        ClassifyFilesRequest, GetAnalysisExtractionRequest, GetAnalysisSourceRequest,
        RunAnalysisExtractionRequest, RunEvidenceClassificationRequest,
    },
    dto::{
        AnalysisExtractionRunDto, AnalysisFileClassificationDto, AnalysisSystemInfoDto,
        BrowserHistorySummaryDto, CorrelationSnapshotDto, EmailExtractionSummaryDto,
        EvidenceClassificationSummaryDto, EvtxEventSummaryDto, LinuxArtifactSummaryDto,
        RegistryExtractionSummaryDto, RegistryStructuredSummaryDto, V2GovernanceSnapshotDto,
        V3GovernanceSnapshotDto,
    },
    CommandError,
};

use crate::state::AppState;

mod support;
use support::*;

/// Get system information from the current case.
#[tauri::command]
pub async fn get_system_info(
    state: State<'_, AppState>,
    request: GetAnalysisSourceRequest,
) -> Result<AnalysisSystemInfoDto, CommandError> {
    validate_source_request(&request)?;
    let app_state = state.inner().clone();
    let data_source_id = DataSourceId(request.data_source_id);

    run_active_case_command(app_state, move |case_conn, active| {
        analysis_service::get_source_system_info(
            case_conn,
            &active.case_root,
            &active.meta.id,
            &data_source_id,
        )
        .map_err(CommandError::from_typed_service_error)
    })
    .await
}

/// Classify files by magic bytes.
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

/// Get semantic evidence classification from metadata and existing artifacts.
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

/// Run targeted evidence artifact extraction for semantic evidence categories.
#[tauri::command]
pub async fn run_evidence_classification(
    state: State<'_, AppState>,
    request: RunEvidenceClassificationRequest,
) -> Result<EvidenceClassificationSummaryDto, CommandError> {
    request.validate().map_err(CommandError::invalid_input)?;
    let app_state = state.inner().clone();
    let data_source_id = DataSourceId(request.data_source_id);
    let requested_categories = request.categories;

    run_active_case_command(app_state, move |case_conn, active| {
        let categories = requested_categories
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>();
        analysis_service::run_source_evidence_scan(
            case_conn,
            &active.case_root,
            &active.meta.id,
            &data_source_id,
            &categories,
        )
        .map_err(CommandError::from_typed_service_error)
    })
    .await
}

/// Run structured extraction for platform-specific analysis categories.
#[tauri::command]
pub async fn run_analysis_extraction(
    state: State<'_, AppState>,
    request: RunAnalysisExtractionRequest,
) -> Result<AnalysisExtractionRunDto, CommandError> {
    request.validate().map_err(CommandError::invalid_input)?;
    let app_state = state.inner().clone();
    let data_source_id = DataSourceId(request.data_source_id);
    let requested_categories = request.categories;

    run_active_case_command(app_state, move |case_conn, active| {
        let categories = requested_categories
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>();
        analysis_service::run_source_analysis_extraction(
            case_conn,
            &active.case_root,
            &active.meta.id,
            &data_source_id,
            &categories,
        )
        .map_err(CommandError::from_typed_service_error)
    })
    .await
}

/// Get Registry key/value extraction summary.
#[tauri::command]
pub async fn get_registry_extraction_summary(
    state: State<'_, AppState>,
    request: Option<GetAnalysisExtractionRequest>,
) -> Result<RegistryExtractionSummaryDto, CommandError> {
    let app_state = state.inner().clone();
    let mut req = request.unwrap_or_default();
    req.validate().map_err(CommandError::invalid_input)?;
    let data_source_id = DataSourceId(req.data_source_id);
    let (offset, limit) = (req.offset, req.limit);

    run_active_case_command(app_state, move |case_conn, active| {
        analysis_service::get_source_registry_summary(
            case_conn,
            &active.case_root,
            &active.meta.id,
            &data_source_id,
            offset,
            limit,
        )
        .map_err(CommandError::from_typed_service_error)
    })
    .await
}

/// Get structured registry summary (SAM users, UserAssist, hive overview).
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

/// Get browser history/download extraction summary.
#[tauri::command]
pub async fn get_browser_history_summary(
    state: State<'_, AppState>,
    request: Option<GetAnalysisExtractionRequest>,
) -> Result<BrowserHistorySummaryDto, CommandError> {
    let app_state = state.inner().clone();
    let mut req = request.unwrap_or_default();
    req.validate().map_err(CommandError::invalid_input)?;
    let data_source_id = DataSourceId(req.data_source_id);
    let (offset, limit) = (req.offset, req.limit);

    run_active_case_command(app_state, move |case_conn, active| {
        analysis_service::get_source_browser_summary(
            case_conn,
            &active.case_root,
            &active.meta.id,
            &data_source_id,
            offset,
            limit,
        )
        .map_err(CommandError::from_typed_service_error)
    })
    .await
}

/// Get EML/EMLX extraction summary.
#[tauri::command]
pub async fn get_email_extraction_summary(
    state: State<'_, AppState>,
    request: Option<GetAnalysisExtractionRequest>,
) -> Result<EmailExtractionSummaryDto, CommandError> {
    let app_state = state.inner().clone();
    let mut req = request.unwrap_or_default();
    req.validate().map_err(CommandError::invalid_input)?;
    let data_source_id = DataSourceId(req.data_source_id);
    let (offset, limit) = (req.offset, req.limit);

    run_active_case_command(app_state, move |case_conn, active| {
        analysis_service::get_source_email_summary(
            case_conn,
            &active.case_root,
            &active.meta.id,
            &data_source_id,
            offset,
            limit,
        )
        .map_err(CommandError::from_typed_service_error)
    })
    .await
}

/// Get structured EVTX event summary for boot/shutdown, Security, and Application logs.
#[tauri::command]
pub async fn get_evtx_event_summary(
    state: State<'_, AppState>,
    request: Option<GetAnalysisExtractionRequest>,
) -> Result<EvtxEventSummaryDto, CommandError> {
    let app_state = state.inner().clone();
    let mut req = request.unwrap_or_default();
    req.validate().map_err(CommandError::invalid_input)?;
    let data_source_id = DataSourceId(req.data_source_id);
    let (offset, limit) = (req.offset, req.limit);

    run_active_case_command(app_state, move |case_conn, active| {
        analysis_service::get_source_evtx_summary(
            case_conn,
            &active.case_root,
            &active.meta.id,
            &data_source_id,
            offset,
            limit,
        )
        .map_err(CommandError::from_typed_service_error)
    })
    .await
}

/// Get Linux artifact summary: systemd journal, wtmp/btmp logins, bash history,
/// apt/dpkg package events, cron jobs, and sudo/auth events.
#[tauri::command]
pub async fn get_linux_artifact_summary(
    state: State<'_, AppState>,
    request: Option<GetAnalysisExtractionRequest>,
) -> Result<LinuxArtifactSummaryDto, CommandError> {
    let app_state = state.inner().clone();
    let mut req = request.unwrap_or_default();
    req.validate().map_err(CommandError::invalid_input)?;
    let data_source_id = DataSourceId(req.data_source_id);
    let (offset, limit) = (req.offset, req.limit);

    run_active_case_command(app_state, move |case_conn, active| {
        analysis_service::get_source_linux_summary(
            case_conn,
            &active.case_root,
            &active.meta.id,
            &data_source_id,
            offset,
            limit,
        )
        .map_err(CommandError::from_typed_service_error)
    })
    .await
}

/// Get V2 governance, verification, benchmark, and release snapshot.
#[tauri::command]
pub async fn get_v2_governance_snapshot(
    state: State<'_, AppState>,
) -> Result<V2GovernanceSnapshotDto, CommandError> {
    let app_state = state.inner().clone();

    run_active_case_command(app_state, |conn, active| {
        app_services::v2_governance_service::get_v2_governance_snapshot_for_case(
            conn,
            &active.case_root,
            &active.case_id,
        )
        .map_err(CommandError::from_typed_service_error)
    })
    .await
}

/// Get V3 governance snapshot: extends V2 with graph, platform coverage,
/// rule pack status, batch job status, and notebook stats.
#[tauri::command]
pub async fn get_v3_governance_snapshot(
    state: State<'_, AppState>,
) -> Result<V3GovernanceSnapshotDto, CommandError> {
    let app_state = state.inner().clone();

    run_active_case_command(app_state, |conn, active| {
        app_services::v3_governance_service::get_v3_governance_snapshot_for_case(
            conn,
            &active.case_root,
            &active.case_id,
        )
        .map_err(CommandError::from_typed_service_error)
    })
    .await
}

/// Get V2 correlation snapshot for shared source-object evidence linking.
#[tauri::command]
pub async fn get_correlation_snapshot(
    state: State<'_, AppState>,
) -> Result<CorrelationSnapshotDto, CommandError> {
    let app_state = state.inner().clone();

    run_active_case_command(app_state, |conn, active| {
        app_services::correlation::get_correlation_snapshot_for_case(
            conn,
            &active.case_root,
            &domain::CaseId(active.case_id.clone()),
        )
        .map_err(CommandError::from_typed_service_error)
    })
    .await
}

/// Generate analysis summary report.
#[tauri::command]
pub async fn generate_analysis_summary(
    state: State<'_, AppState>,
    request: GetAnalysisSourceRequest,
) -> Result<String, CommandError> {
    validate_source_request(&request)?;
    let app_state = state.inner().clone();
    let data_source_id = DataSourceId(request.data_source_id);

    run_active_case_command(app_state, move |case_conn, active| {
        analysis_service::generate_source_analysis_summary(
            case_conn,
            &active.case_root,
            &active.meta.id,
            &data_source_id,
        )
        .map_err(CommandError::from_typed_service_error)
    })
    .await
}

#[cfg(test)]
#[path = "../../tests/unit/commands/analysis_commands_test.rs"]
mod tests;
