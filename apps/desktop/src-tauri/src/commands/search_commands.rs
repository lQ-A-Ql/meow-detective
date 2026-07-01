use app_services::step_recorder;
use tauri::{AppHandle, State};
use transport::{commands::SearchFilesRequest, dto::SearchResultPageDto, CommandError};

use crate::events::event_bridge;
use crate::state::AppState;

/// Search files in the current case's index.
#[tauri::command]
pub async fn search_files(
    state: State<'_, AppState>,
    app: AppHandle,
    query: String,
) -> Result<SearchResultPageDto, CommandError> {
    search_files_request(
        state,
        app,
        SearchFilesRequest {
            query,
            offset: 0,
            limit: infrastructure::constants::SEARCH_PAGE_SIZE as u32,
        },
    )
    .await
}

/// Search files with explicit request parameters.
#[tauri::command]
pub async fn search_files_request(
    state: State<'_, AppState>,
    app: AppHandle,
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
    let query_for_step = request.query.clone();
    let app_state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        // Short lock: extract index_dir and case info, then release
        let (index_dir, db_path, case_id) = {
            let guard = app_state
                .active_case
                .lock()
                .map_err(|e| CommandError::from_lock_error("Case", e))?;
            match guard.as_ref() {
                Some(active) => (
                    active.case_root.join("indexes").join("tantivy"),
                    Some(active.db_path()),
                    Some(active.meta.id.0.clone()),
                ),
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
        let start = std::time::Instant::now();
        let result = app_services::search_service::search_files_real_instrumented(
            &index_dir,
            &request.query,
            request.offset,
            request.limit,
        )
        .map_err(CommandError::from_typed_service_error)?;
        let elapsed_ms = start.elapsed().as_millis() as u32;
        event_bridge::emit_performance_report_ready(&app, &result.performance_report);

        // Record investigation step for provenance
        if let (Some(db_path), Some(case_id)) = (&db_path, &case_id) {
            if let Ok(conn) = app_services::connection::open_case_db(db_path) {
                let params_json = serde_json::json!({
                    "query": &query_for_step,
                    "offset": request.offset,
                    "limit": request.limit,
                    "totalHits": result.page.total,
                })
                .to_string();
                let _ = step_recorder::record_step(
                    &conn,
                    case_id,
                    "search",
                    &params_json,
                    elapsed_ms,
                    true,
                    None,
                );
            }
        }

        Ok(result.page)
    })
    .await
    .map_err(CommandError::from_join_error)?
}
