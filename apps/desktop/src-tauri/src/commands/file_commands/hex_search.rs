//! Whole-file hex search commands.

use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use domain::CaseId;
use tauri::State;
use transport::{
    dto::{HexSearchFileRequestDto, HexSearchRequestDto, HexSearchStatusDto},
    CommandError, ServiceErrorCategory,
};

use crate::commands::command_support::{get_case_read_connection, require_active_case};
use crate::state::hex_search::HexSearchAdmission;
use crate::state::{AppState, TaskScope};

mod worker;
use worker::{run_hex_search, HexSearchJob};

#[tauri::command]
pub async fn start_hex_search(
    state: State<'_, AppState>,
    request: HexSearchRequestDto,
) -> Result<HexSearchStatusDto, CommandError> {
    if request.file_id.trim().is_empty() {
        return Err(CommandError::invalid_input("fileId is required"));
    }
    let pattern = request.normalized_pattern().map_err(invalid_pattern)?;
    let max_matches = request.max_matches();
    let app_state = state.inner().clone();
    let active = require_active_case(&app_state)?;
    let data_source_id = resolve_data_source_id(
        &app_state,
        &active.case_id,
        active.case_root.clone(),
        &request.file_id,
    )
    .await?;
    let case_id = active.case_id;
    let file_id = request.file_id;
    let status = match app_state
        .hex_search_jobs
        .admit(&case_id, &file_id, request.mode, &pattern)
    {
        HexSearchAdmission::Reuse(status) => return Ok(status),
        HexSearchAdmission::Busy => {
            return Err(CommandError::conflict(
                "Another hex search is already running",
            ))
        }
        HexSearchAdmission::Start(status) => status,
        HexSearchAdmission::Replace {
            cancelled_task_id,
            status,
        } => {
            app_state.task_manager.cancel(&cancelled_task_id);
            status
        }
    };
    let task_id = status.task_id.clone();
    let cancel_token = Arc::new(AtomicBool::new(false));
    let scope = TaskScope::data_source(&case_id, &data_source_id, &task_id);
    let worker_state = app_state.clone();
    let job = HexSearchJob {
        case_root: active.case_root,
        case_id: case_id.clone(),
        file_id: file_id.clone(),
        task_id: task_id.clone(),
        pattern,
        max_matches,
        cancel_token: Arc::clone(&cancel_token),
    };
    let registration =
        app_state
            .task_manager
            .spawn_scoped(task_id.clone(), scope, cancel_token, move || {
                run_hex_search(worker_state, job)
            });
    if let Err(error) = registration {
        tracing::warn!(error = %error, "Hex search task failed to start");
        mark_failed(&app_state, &case_id, &file_id, &task_id);
        return Err(CommandError::internal("Hex search task failed to start"));
    }
    Ok(status)
}

#[tauri::command]
pub fn get_hex_search_status(
    state: State<'_, AppState>,
    request: HexSearchFileRequestDto,
) -> Result<Option<HexSearchStatusDto>, CommandError> {
    request.validate().map_err(CommandError::invalid_input)?;
    let app_state = state.inner();
    let active = require_active_case(app_state)?;
    let task_manager = Arc::clone(&app_state.task_manager);
    Ok(app_state
        .hex_search_jobs
        .reconcile(&active.case_id, &request.file_id, |task_id| {
            task_manager.is_running(task_id)
        }))
}

#[tauri::command]
pub fn cancel_hex_search(
    state: State<'_, AppState>,
    request: HexSearchFileRequestDto,
) -> Result<bool, CommandError> {
    request.validate().map_err(CommandError::invalid_input)?;
    let app_state = state.inner();
    let active = require_active_case(app_state)?;
    let Some(current) = app_state
        .hex_search_jobs
        .get(&active.case_id, &request.file_id)
    else {
        return Ok(false);
    };
    if current.phase.is_terminal() {
        return Ok(false);
    }
    let requested = app_state.task_manager.cancel(&current.task_id);
    if requested {
        app_state.hex_search_jobs.mark_cancelling(
            &active.case_id,
            &request.file_id,
            &current.task_id,
        );
    }
    Ok(requested)
}

async fn resolve_data_source_id(
    app_state: &AppState,
    case_id: &str,
    case_root: PathBuf,
    file_id: &str,
) -> Result<String, CommandError> {
    let app_state = app_state.clone();
    let case_id = CaseId(case_id.to_string());
    let file_id = file_id.to_string();
    tauri::async_runtime::spawn_blocking(move || -> Result<String, CommandError> {
        let connection = get_case_read_connection(&app_state)?;
        let (global_id, _source_connection) = app_services::file_service::open_source_for_file_id(
            &connection,
            &case_root,
            &case_id,
            &file_id,
        )
        .map_err(source_resolution_error)?;
        Ok(global_id.data_source_id.0)
    })
    .await
    .map_err(CommandError::from_join_error)?
}

fn source_resolution_error(error: app_services::file_service::FileServiceError) -> CommandError {
    let (code, message) = match error.category() {
        transport::ErrorCategory::Unsupported => (
            "unsupported_source",
            "This source cannot be searched in hex view",
        ),
        transport::ErrorCategory::Validation => {
            ("source_unavailable", "The file is unavailable in this case")
        }
        _ => (
            "hex_search_failed",
            "The file could not be opened for hex search",
        ),
    };
    let mut command_error = CommandError::from_typed_service_error(error);
    command_error.code = code.to_string();
    command_error.message = message.to_string();
    command_error
}

fn invalid_pattern(_message: String) -> CommandError {
    let mut error = CommandError::invalid_input("Search pattern is invalid");
    error.code = "invalid_pattern".to_string();
    error
}

fn mark_failed(app_state: &AppState, case_id: &str, file_id: &str, task_id: &str) {
    app_state
        .hex_search_jobs
        .update_if_current(case_id, file_id, task_id, |status| {
            status.phase = transport::dto::HexSearchPhaseDto::Failed;
            status.error = Some("hex_search_failed".to_string());
        });
}
