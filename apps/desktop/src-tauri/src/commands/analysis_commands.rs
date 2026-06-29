//! Data source analysis commands.

use app_services::{analysis_service, file_service};
use domain::FileEntryId;
use std::io::{Cursor, Read};
use tauri::State;
use transport::{
    commands::{
        ClassifyFilesRequest, GetAnalysisExtractionRequest, RunAnalysisExtractionRequest,
        RunEvidenceClassificationRequest,
    },
    dto::{
        AnalysisExtractionRunDto, AnalysisFileClassificationDto, AnalysisSystemInfoDto,
        BrowserHistorySummaryDto, CorrelationSnapshotDto, EmailExtractionSummaryDto,
        EvidenceClassificationSummaryDto, EvtxEventSummaryDto, RegistryExtractionSummaryDto,
        RegistryStructuredSummaryDto, V2GovernanceSnapshotDto, V3GovernanceSnapshotDto,
    },
    CommandError,
};

use super::command_support::{get_case_connection, require_active_case};
use crate::state::AppState;

fn resolve_sample_size(request: &ClassifyFilesRequest) -> Result<u32, CommandError> {
    let sample_size = request
        .sample_size
        .unwrap_or(analysis_service::DEFAULT_SAMPLE_SIZE);
    if sample_size == 0 || sample_size > analysis_service::MAX_SAMPLE_SIZE {
        return Err(CommandError::invalid_input(format!(
            "sampleSize must be between 1 and {}",
            analysis_service::MAX_SAMPLE_SIZE
        )));
    }
    Ok(sample_size)
}

fn limited_header_cursor<E>(
    file_id: &FileEntryId,
    max_bytes: usize,
    read_header: impl FnOnce(&FileEntryId, usize) -> Result<Vec<u8>, E>,
) -> Result<Box<dyn Read>, E> {
    let bytes = read_header(file_id, max_bytes)?;
    Ok(Box::new(Cursor::new(bytes)))
}

fn read_file_header_cursor_by_id(
    conn: &rusqlite::Connection,
    file_id: &FileEntryId,
    max_bytes: usize,
) -> Result<Box<dyn Read>, app_services::file_service::FileServiceError> {
    limited_header_cursor(file_id, max_bytes, |file_id, max_bytes| {
        file_service::read_file_header_by_id(conn, file_id, max_bytes)
    })
}

/// Get system information from the current case.
#[tauri::command]
pub async fn get_system_info(
    state: State<'_, AppState>,
) -> Result<AnalysisSystemInfoDto, CommandError> {
    let app_state = state.inner().clone();

    tauri::async_runtime::spawn_blocking(move || {
        require_active_case(&app_state)?;
        let conn = get_case_connection(&app_state)?;
        Ok(analysis_service::extract_system_info_for_case(
            &conn,
            |file_id, max_bytes| file_service::read_file_header_by_id(&conn, file_id, max_bytes),
        ))
    })
    .await
    .map_err(CommandError::from_join_error)?
}

/// Classify files by magic bytes.
#[tauri::command]
pub async fn classify_files(
    state: State<'_, AppState>,
    request: ClassifyFilesRequest,
) -> Result<Vec<AnalysisFileClassificationDto>, CommandError> {
    let sample_size = resolve_sample_size(&request)?;
    let app_state = state.inner().clone();

    tauri::async_runtime::spawn_blocking(move || {
        require_active_case(&app_state)?;
        let conn = get_case_connection(&app_state)?;
        analysis_service::classify_files_by_metadata(&conn, sample_size)
            .map_err(CommandError::from_service_error)
    })
    .await
    .map_err(CommandError::from_join_error)?
}

/// Get semantic evidence classification from metadata and existing artifacts.
#[tauri::command]
pub async fn get_evidence_classification_summary(
    state: State<'_, AppState>,
) -> Result<EvidenceClassificationSummaryDto, CommandError> {
    let app_state = state.inner().clone();

    tauri::async_runtime::spawn_blocking(move || {
        require_active_case(&app_state)?;
        let conn = get_case_connection(&app_state)?;
        analysis_service::get_evidence_classification_summary(&conn)
            .map_err(CommandError::from_service_error)
    })
    .await
    .map_err(CommandError::from_join_error)?
}

/// Run targeted evidence artifact extraction for semantic evidence categories.
#[tauri::command]
pub async fn run_evidence_classification(
    state: State<'_, AppState>,
    request: RunEvidenceClassificationRequest,
) -> Result<EvidenceClassificationSummaryDto, CommandError> {
    let app_state = state.inner().clone();

    tauri::async_runtime::spawn_blocking(move || {
        let active = require_active_case(&app_state)?;
        let conn = get_case_connection(&app_state)?;
        let categories = request
            .categories
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>();
        app_services::artifact_service::run_targeted_evidence_scan(
            &conn,
            &active.case_id,
            &categories,
            |file_id| {
                read_file_header_cursor_by_id(
                    &conn,
                    file_id,
                    infrastructure::constants::ARTIFACT_FILE_LIMIT_BYTES as usize,
                )
                .map_err(app_services::artifact_service::ArtifactServiceError::from)
            },
        )
        .map_err(CommandError::from_service_error)?;
        analysis_service::get_evidence_classification_summary(&conn)
            .map_err(CommandError::from_service_error)
    })
    .await
    .map_err(CommandError::from_join_error)?
}

/// Run v1 structured extraction for Registry, browser history, and email evidence.
#[tauri::command]
pub async fn run_analysis_extraction(
    state: State<'_, AppState>,
    request: RunAnalysisExtractionRequest,
) -> Result<AnalysisExtractionRunDto, CommandError> {
    let app_state = state.inner().clone();

    tauri::async_runtime::spawn_blocking(move || {
        let active = require_active_case(&app_state)?;
        let conn = get_case_connection(&app_state)?;
        let categories = request
            .categories
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>();
        analysis_service::run_analysis_extraction(&conn, &active.case_id, &categories, |file_id| {
            read_file_header_cursor_by_id(
                &conn,
                file_id,
                analysis_service::MAX_ANALYSIS_SOURCE_BYTES,
            )
        })
        .map_err(CommandError::from_service_error)
    })
    .await
    .map_err(CommandError::from_join_error)?
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

    tauri::async_runtime::spawn_blocking(move || {
        require_active_case(&app_state)?;
        let conn = get_case_connection(&app_state)?;
        analysis_service::get_registry_extraction_summary(&conn, req.offset, req.limit)
            .map_err(CommandError::from_service_error)
    })
    .await
    .map_err(CommandError::from_join_error)?
}

/// Get structured registry summary (SAM users, UserAssist, hive overview).
#[tauri::command]
pub async fn get_registry_structured_summary(
    state: State<'_, AppState>,
) -> Result<RegistryStructuredSummaryDto, CommandError> {
    let app_state = state.inner().clone();

    tauri::async_runtime::spawn_blocking(move || {
        require_active_case(&app_state)?;
        let conn = get_case_connection(&app_state)?;
        analysis_service::get_registry_structured_summary(&conn)
            .map_err(CommandError::from_service_error)
    })
    .await
    .map_err(CommandError::from_join_error)?
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

    tauri::async_runtime::spawn_blocking(move || {
        require_active_case(&app_state)?;
        let conn = get_case_connection(&app_state)?;
        analysis_service::get_browser_history_summary(&conn, req.offset, req.limit)
            .map_err(CommandError::from_service_error)
    })
    .await
    .map_err(CommandError::from_join_error)?
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

    tauri::async_runtime::spawn_blocking(move || {
        require_active_case(&app_state)?;
        let conn = get_case_connection(&app_state)?;
        analysis_service::get_email_extraction_summary(&conn, req.offset, req.limit)
            .map_err(CommandError::from_service_error)
    })
    .await
    .map_err(CommandError::from_join_error)?
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

    tauri::async_runtime::spawn_blocking(move || {
        require_active_case(&app_state)?;
        let conn = get_case_connection(&app_state)?;
        analysis_service::get_evtx_event_summary(&conn, req.offset, req.limit)
            .map_err(CommandError::from_service_error)
    })
    .await
    .map_err(CommandError::from_join_error)?
}

/// Get V2 governance, verification, benchmark, and release snapshot.
#[tauri::command]
pub async fn get_v2_governance_snapshot(
    state: State<'_, AppState>,
) -> Result<V2GovernanceSnapshotDto, CommandError> {
    let app_state = state.inner().clone();

    tauri::async_runtime::spawn_blocking(move || {
        let active = require_active_case(&app_state)?;
        let conn = get_case_connection(&app_state)?;
        app_services::v2_governance_service::get_v2_governance_snapshot(&conn, &active.case_id)
            .map_err(CommandError::from_service_error)
    })
    .await
    .map_err(CommandError::from_join_error)?
}

/// Get V3 governance snapshot: extends V2 with graph, platform coverage,
/// rule pack status, batch job status, and notebook stats.
#[tauri::command]
pub async fn get_v3_governance_snapshot(
    state: State<'_, AppState>,
) -> Result<V3GovernanceSnapshotDto, CommandError> {
    let app_state = state.inner().clone();

    tauri::async_runtime::spawn_blocking(move || {
        let active = require_active_case(&app_state)?;
        let conn = get_case_connection(&app_state)?;
        app_services::v3_governance_service::get_v3_governance_snapshot(&conn, &active.case_id)
            .map_err(CommandError::from_service_error)
    })
    .await
    .map_err(CommandError::from_join_error)?
}

/// Get V2 correlation snapshot for shared source-object evidence linking.
#[tauri::command]
pub async fn get_correlation_snapshot(
    state: State<'_, AppState>,
) -> Result<CorrelationSnapshotDto, CommandError> {
    let app_state = state.inner().clone();

    tauri::async_runtime::spawn_blocking(move || {
        require_active_case(&app_state)?;
        let conn = get_case_connection(&app_state)?;
        app_services::correlation::get_correlation_snapshot(&conn)
            .map_err(CommandError::from_service_error)
    })
    .await
    .map_err(CommandError::from_join_error)?
}

/// Generate analysis summary report.
#[tauri::command]
pub async fn generate_analysis_summary(state: State<'_, AppState>) -> Result<String, CommandError> {
    let app_state = state.inner().clone();

    tauri::async_runtime::spawn_blocking(move || {
        require_active_case(&app_state)?;
        let conn = get_case_connection(&app_state)?;
        let system_info =
            analysis_service::extract_system_info_for_case(&conn, |file_id, max_bytes| {
                file_service::read_file_header_by_id(&conn, file_id, max_bytes)
            });
        let classifications = analysis_service::classify_files_by_metadata(
            &conn,
            analysis_service::DEFAULT_SAMPLE_SIZE,
        )
        .map_err(CommandError::from_service_error)?;

        Ok(analysis_service::generate_analysis_summary(
            &system_info,
            &classifications,
        ))
    })
    .await
    .map_err(CommandError::from_join_error)?
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    #[test]
    fn sample_size_defaults_and_validates_bounds() {
        assert_eq!(
            resolve_sample_size(&ClassifyFilesRequest { sample_size: None }).unwrap(),
            analysis_service::DEFAULT_SAMPLE_SIZE
        );
        assert_eq!(
            resolve_sample_size(&ClassifyFilesRequest {
                sample_size: Some(1)
            })
            .unwrap(),
            1
        );
        assert!(resolve_sample_size(&ClassifyFilesRequest {
            sample_size: Some(0)
        })
        .is_err());
        assert!(resolve_sample_size(&ClassifyFilesRequest {
            sample_size: Some(analysis_service::MAX_SAMPLE_SIZE + 1)
        })
        .is_err());
    }

    #[test]
    fn limited_header_cursor_passes_limit_and_returns_bytes_reader() {
        let file_id = FileEntryId("file-1".to_string());
        let calls = RefCell::new(Vec::new());

        let mut reader = limited_header_cursor(&file_id, 7, |id, max_bytes| {
            calls.borrow_mut().push((id.clone(), max_bytes));
            Ok::<_, String>(b"bounded".to_vec())
        })
        .unwrap();

        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes).unwrap();

        assert_eq!(calls.into_inner(), vec![(file_id, 7)]);
        assert_eq!(bytes, b"bounded");
    }
}
