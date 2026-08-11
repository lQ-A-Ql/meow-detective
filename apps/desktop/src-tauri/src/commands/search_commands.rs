use app_services::step_recorder;
use tauri::{AppHandle, State};
use transport::{commands::SearchFilesRequest, dto::SearchFileResultPageDto, CommandError};

use crate::events::event_bridge;
use crate::state::AppState;

/// Search files in the current case's index.
#[tauri::command]
pub async fn search_files(
    state: State<'_, AppState>,
    app: AppHandle,
    query: String,
) -> Result<SearchFileResultPageDto, CommandError> {
    search_files_request(
        state,
        app,
        SearchFilesRequest {
            query,
            match_path: false,
            entry_type: Default::default(),
            extensions: Vec::new(),
            data_source_ids: Vec::new(),
            sort_key: Default::default(),
            sort_direction: Default::default(),
            offset: 0,
            limit: infrastructure::constants::SEARCH_PAGE_SIZE as u32,
            cursor: None,
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
) -> Result<SearchFileResultPageDto, CommandError> {
    request.validate().map_err(CommandError::invalid_input)?;
    let query_for_step = request.query.clone();
    let app_state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        // Short lock: extract index_dir and case info, then release
        let (case_root, db_path, case_id) = {
            let guard = app_state
                .active_case
                .lock()
                .map_err(|e| CommandError::from_lock_error("Case", e))?;
            match guard.as_ref() {
                Some(active) => (
                    active.case_root.clone(),
                    Some(active.db_path()),
                    Some(active.meta.id.0.clone()),
                ),
                None => {
                    return Ok(empty_search_result_page());
                }
            }
        };
        // Guard is now dropped — search with released lock
        let start = std::time::Instant::now();
        let Some(case_id_string) = case_id.clone() else {
            return Ok(empty_search_result_page());
        };
        let conn = app_services::connection::open_case_db(&case_root.join("app.db"))
            .map_err(CommandError::from_typed_service_error)?;
        let result = if request.cursor.is_some() || request.offset == 0 {
            app_services::search_service::search_files_for_case_cursor_instrumented(
                &conn,
                &case_root,
                &domain::CaseId(case_id_string.clone()),
                &request,
            )
        } else {
            app_services::search_service::search_files_for_case_instrumented(
                &conn,
                &case_root,
                &domain::CaseId(case_id_string.clone()),
                &request,
            )
        }
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
                    "cursorContinuation": request.cursor.is_some(),
                    "totalHits": result.page.total,
                })
                .to_string();
                let _ = step_recorder::record_step(
                    &conn,
                    &case_root,
                    step_recorder::CaseStepInput {
                        case_id,
                        step_kind: "search",
                        params_json: &params_json,
                        duration_ms: elapsed_ms,
                        success: true,
                        error_code: None,
                    },
                );
            }
        }

        Ok(result.page)
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
