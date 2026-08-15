use tauri::{AppHandle, State};
use transport::{commands::SearchFilesRequest, dto::SearchFileResultPageDto, CommandError};

use crate::commands::command_support::snapshot_active_case;
use crate::events::event_bridge;
use crate::state::AppState;

/// Search files with explicit request parameters.
#[tauri::command]
pub async fn search_files_request(
    state: State<'_, AppState>,
    app: AppHandle,
    mut request: SearchFilesRequest,
) -> Result<SearchFileResultPageDto, CommandError> {
    request.validate().map_err(CommandError::invalid_input)?;
    let app_state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        // Short lock: snapshot the active case, then release before searching.
        let Some(active) = snapshot_active_case(&app_state)? else {
            return Ok(empty_search_result_page());
        };
        let connection = app_services::connection::open_case_db(&active.case_root.join("app.db"))
            .map_err(CommandError::from_typed_service_error)?;
        app_services::search_service::search_files_request_for_case(
            &connection,
            &active.case_root,
            &active.meta.id,
            &request,
            |report| event_bridge::emit_performance_report_ready(&app, report),
        )
        .map_err(CommandError::from_typed_service_error)
    })
    .await
    .map_err(CommandError::from_join_error)?
}

fn empty_search_result_page() -> SearchFileResultPageDto {
    SearchFileResultPageDto {
        total: 0,
        available: 0,
        truncated: false,
        took_ms: 0,
        items: vec![],
        coverage: Default::default(),
        next_cursor: None,
    }
}
