use tauri::State;
use transport::{
    commands::GetArtifactRowsRequest,
    dto::{ArtifactRowDto, FamilyCountDto},
    CommandError,
};

use super::command_support::{get_case_connection, snapshot_active_case};
use crate::state::AppState;

/// Get list of artifact families in the current case.
#[tauri::command]
pub async fn get_artifact_families(
    state: State<'_, AppState>,
) -> Result<Vec<String>, CommandError> {
    let app_state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        if snapshot_active_case(&app_state)?.is_none() {
            return Ok(vec![]);
        }
        let conn = get_case_connection(&app_state)?;
        app_services::artifact_service::get_artifact_families_from_db(&conn)
            .map_err(CommandError::from_service_error)
    })
    .await
    .map_err(CommandError::from_join_error)?
}

/// Get artifact rows, optionally filtered by family.
#[tauri::command]
pub async fn get_artifact_rows(
    state: State<'_, AppState>,
    family: Option<String>,
) -> Result<Vec<ArtifactRowDto>, CommandError> {
    get_artifact_rows_request(state, GetArtifactRowsRequest { family }).await
}

/// Get artifact rows with explicit request parameters.
#[tauri::command]
pub async fn get_artifact_rows_request(
    state: State<'_, AppState>,
    request: GetArtifactRowsRequest,
) -> Result<Vec<ArtifactRowDto>, CommandError> {
    let app_state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        if snapshot_active_case(&app_state)?.is_none() {
            return Ok(vec![]);
        }
        let conn = get_case_connection(&app_state)?;
        app_services::artifact_service::get_artifact_rows_from_db(&conn, request.family.as_deref())
            .map_err(CommandError::from_service_error)
    })
    .await
    .map_err(CommandError::from_join_error)?
}

/// Get per-family artifact counts for the current case.
#[tauri::command]
pub async fn get_artifact_family_counts(
    state: State<'_, AppState>,
) -> Result<Vec<FamilyCountDto>, CommandError> {
    let app_state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        if snapshot_active_case(&app_state)?.is_none() {
            return Ok(vec![]);
        }
        let conn = get_case_connection(&app_state)?;
        app_services::artifact_service::get_artifact_family_counts(&conn)
            .map_err(CommandError::from_service_error)
    })
    .await
    .map_err(CommandError::from_join_error)?
}
