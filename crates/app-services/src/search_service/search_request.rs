//! Orchestration for the `search_files_request` use case: pick the cursor or
//! offset search path and report the performance sample. Investigation-step
//! persistence is an explicit best-effort audit operation.

use rusqlite::Connection;
use std::path::Path;
use transport::commands::SearchFilesRequest;
use transport::dto::{PerformanceReportDto, SearchFileResultPageDto};

use super::instrumented::{
    search_files_for_case_cursor_instrumented, search_files_for_case_instrumented,
};
use super::SearchError;
use crate::step_recorder;

/// Execute a file search request against an active case.
///
/// Cursor continuations and first pages (`offset == 0`) use the cursor search
/// path; later offsets use the plain offset path. The performance report is
/// handed to `on_performance_report` after a successful search. The query is
/// read-only; call [`record_search_step`] separately for optional audit data.
pub fn search_files_request_for_case(
    case_conn: &Connection,
    case_root: &Path,
    case_id: &domain::CaseId,
    request: &SearchFilesRequest,
    on_performance_report: impl FnOnce(&PerformanceReportDto),
) -> Result<SearchFileResultPageDto, SearchError> {
    let result = if request.cursor.is_some() || request.offset == 0 {
        search_files_for_case_cursor_instrumented(case_conn, case_root, case_id, request)
    } else {
        search_files_for_case_instrumented(case_conn, case_root, case_id, request)
    }?;
    on_performance_report(&result.performance_report);
    Ok(result.page)
}

pub fn record_search_step(
    case_root: &Path,
    case_id: &domain::CaseId,
    request: &SearchFilesRequest,
    total_hits: u64,
    elapsed_ms: u32,
) {
    if let Ok(conn) = crate::connection::open_existing_case_db(&case_root.join("app.db")) {
        let params_json = serde_json::json!({
            "query": &request.query,
            "offset": request.offset,
            "limit": request.limit,
            "cursorContinuation": request.cursor.is_some(),
            "totalHits": total_hits,
        })
        .to_string();
        let _ = step_recorder::record_step(
            &conn,
            case_root,
            step_recorder::CaseStepInput {
                case_id: &case_id.0,
                step_kind: "search",
                params_json: &params_json,
                duration_ms: elapsed_ms,
                success: true,
                error_code: None,
            },
        );
    }
}

#[cfg(test)]
#[path = "../../tests/unit/search_service/search_request.rs"]
mod tests;
