//! Data source analysis commands.

use app_services::{analysis_service, file_service};
use tauri::State;
use transport::{
    commands::{ClassifyFilesRequest, RunEvidenceClassificationRequest},
    dto::{AnalysisFileClassificationDto, AnalysisSystemInfoDto, EvidenceClassificationSummaryDto},
    CommandError,
};

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

/// Get system information from the current case.
#[tauri::command]
pub async fn get_system_info(
    state: State<'_, AppState>,
) -> Result<AnalysisSystemInfoDto, CommandError> {
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

        let conn = persistence_sqlite::open_or_create(&db_path)
            .map_err(CommandError::from_service_error)?;
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
        let db_path = {
            let guard = app_state
                .active_case
                .lock()
                .map_err(|e| CommandError::from_lock_error("Case", e))?;
            let active = guard.as_ref().ok_or_else(CommandError::no_active_case)?;
            active.db_path()
        };

        let conn = persistence_sqlite::open_or_create(&db_path)
            .map_err(CommandError::from_service_error)?;
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
        let db_path = {
            let guard = app_state
                .active_case
                .lock()
                .map_err(|e| CommandError::from_lock_error("Case", e))?;
            let active = guard.as_ref().ok_or_else(CommandError::no_active_case)?;
            active.db_path()
        };

        let conn = persistence_sqlite::open_or_create(&db_path)
            .map_err(CommandError::from_service_error)?;
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
        let (case_id, db_path) = {
            let guard = app_state
                .active_case
                .lock()
                .map_err(|e| CommandError::from_lock_error("Case", e))?;
            let active = guard.as_ref().ok_or_else(CommandError::no_active_case)?;
            (active.meta.id.0.clone(), active.db_path())
        };

        let conn = persistence_sqlite::open_or_create(&db_path)
            .map_err(CommandError::from_service_error)?;
        let categories = request
            .categories
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>();
        app_services::artifact_service::run_targeted_evidence_scan(
            &conn,
            &case_id,
            &categories,
            |file_id| file_service::open_file_content_by_id(&conn, file_id),
        )
        .map_err(CommandError::from_service_error)?;
        analysis_service::get_evidence_classification_summary(&conn)
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
        let db_path = {
            let guard = app_state
                .active_case
                .lock()
                .map_err(|e| CommandError::from_lock_error("Case", e))?;
            let active = guard.as_ref().ok_or_else(CommandError::no_active_case)?;
            active.db_path()
        };

        let conn = persistence_sqlite::open_or_create(&db_path)
            .map_err(CommandError::from_service_error)?;
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
}
