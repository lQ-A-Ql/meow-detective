use tauri::State;
use transport::{commands::GetArtifactRowsRequest, dto::ArtifactRowDto};

use crate::state::AppState;

#[tauri::command]
pub fn get_artifact_families(state: State<AppState>) -> Result<Vec<String>, String> {
    let guard = state.active_case.lock().map_err(|e| e.to_string())?;
    if guard.is_some() {
        // TODO: query artifact families from DB
        return Ok(vec![]);
    }
    Err("No active case".into())
}

#[tauri::command]
pub fn get_artifact_rows(
    state: State<AppState>,
    _family: Option<String>,
) -> Result<Vec<ArtifactRowDto>, String> {
    let guard = state.active_case.lock().map_err(|e| e.to_string())?;
    if guard.is_some() {
        // TODO: query artifact rows from DB
        return Ok(vec![]);
    }
    Err("No active case".into())
}

#[tauri::command]
pub fn get_artifact_rows_request(
    state: State<AppState>,
    _request: GetArtifactRowsRequest,
) -> Result<Vec<ArtifactRowDto>, String> {
    let guard = state.active_case.lock().map_err(|e| e.to_string())?;
    if guard.is_some() {
        return Ok(vec![]);
    }
    Err("No active case".into())
}
