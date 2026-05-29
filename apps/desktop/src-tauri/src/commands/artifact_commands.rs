use tauri::State;
use transport::{commands::GetArtifactRowsRequest, dto::ArtifactRowDto, CommandError};

use crate::state::AppState;

/// Get list of artifact families in the current case.
#[tauri::command]
pub async fn get_artifact_families(
    state: State<'_, AppState>,
) -> Result<Vec<String>, CommandError> {
    let app_state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        // Short lock: extract db_path, then release
        let db_path = {
            let guard = app_state
                .active_case
                .lock()
                .map_err(|e| CommandError::from_lock_error("Case", e))?;
            match guard.as_ref() {
                Some(active) => active.db_path(),
                None => return Ok(vec![]),
            }
        };
        // Guard is now dropped — query with released lock
        let conn = persistence_sqlite::open_or_create(&db_path)
            .map_err(CommandError::from_service_error)?;
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
        // Short lock: extract db_path, then release
        let db_path = {
            let guard = app_state
                .active_case
                .lock()
                .map_err(|e| CommandError::from_lock_error("Case", e))?;
            match guard.as_ref() {
                Some(active) => active.db_path(),
                None => return Ok(vec![]),
            }
        };
        // Guard is now dropped — query with released lock
        let conn = persistence_sqlite::open_or_create(&db_path)
            .map_err(CommandError::from_service_error)?;
        app_services::artifact_service::get_artifact_rows_from_db(
            &conn,
            request.family.as_deref(),
        )
        .map_err(CommandError::from_service_error)
    })
    .await
    .map_err(CommandError::from_join_error)?
}
