use tauri::State;
use transport::{commands::SearchFilesRequest, dto::SearchResultPageDto};

use crate::state::AppState;

#[tauri::command]
pub fn search_files(state: State<AppState>, query: String) -> Result<SearchResultPageDto, String> {
    let guard = state.active_case.lock().map_err(|e| e.to_string())?;
    let active = guard.as_ref().ok_or("No active case")?;
    let index_dir = active.case_root.join("indexes").join("tantivy");
    if !index_dir.exists() {
        return Ok(SearchResultPageDto {
            total: 0,
            took_ms: 0,
            items: vec![],
        });
    }
    app_services::search_service::search_files_real(&index_dir, &query)
}

#[tauri::command]
pub fn search_files_request(
    state: State<AppState>,
    request: SearchFilesRequest,
) -> Result<SearchResultPageDto, String> {
    let guard = state.active_case.lock().map_err(|e| e.to_string())?;
    let active = guard.as_ref().ok_or("No active case")?;
    let index_dir = active.case_root.join("indexes").join("tantivy");
    if !index_dir.exists() {
        return Ok(SearchResultPageDto {
            total: 0,
            took_ms: 0,
            items: vec![],
        });
    }
    app_services::search_service::search_files_real(&index_dir, &request.query)
}
