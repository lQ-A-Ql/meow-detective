use tauri::State;
use transport::{commands::SearchFilesRequest, dto::SearchResultPageDto, CommandError};

use crate::state::AppState;

/// Search files in the current case's index.
#[tauri::command]
pub async fn search_files(
    state: State<'_, AppState>,
    query: String,
) -> Result<SearchResultPageDto, CommandError> {
    search_files_request(
        state,
        SearchFilesRequest {
            query,
            offset: 0,
            limit: 50,
        },
    )
    .await
}

/// Search files with explicit request parameters.
#[tauri::command]
pub async fn search_files_request(
    state: State<'_, AppState>,
    mut request: SearchFilesRequest,
) -> Result<SearchResultPageDto, CommandError> {
    request.validate().map_err(CommandError::invalid_input)?;
    // Validate query length
    if request.query.len() > infrastructure::constants::MAX_QUERY_LENGTH {
        return Err(CommandError::invalid_input(format!(
            "Query too long (max {} characters)",
            infrastructure::constants::MAX_QUERY_LENGTH
        )));
    }
    let app_state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        // Short lock: extract index_dir, then release
        let index_dir = {
            let guard = app_state
                .active_case
                .lock()
                .map_err(|e| CommandError::from_lock_error("Case", e))?;
            match guard.as_ref() {
                Some(active) => active.case_root.join("indexes").join("tantivy"),
                None => {
                    return Ok(SearchResultPageDto {
                        total: 0,
                        took_ms: 0,
                        items: vec![],
                    })
                }
            }
        };
        // Guard is now dropped — search with released lock
        if !index_dir.exists() {
            return Ok(SearchResultPageDto {
                total: 0,
                took_ms: 0,
                items: vec![],
            });
        }
        app_services::search_service::search_files_real(
            &index_dir,
            &request.query,
            request.offset,
            request.limit,
        )
        .map_err(CommandError::from_service_error)
    })
    .await
    .map_err(CommandError::from_join_error)?
}
