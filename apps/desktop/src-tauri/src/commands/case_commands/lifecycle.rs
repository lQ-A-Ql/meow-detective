use app_services::{case_service, job_service};
use std::path::PathBuf;
use tauri::{AppHandle, State};
use transport::{
    commands::{CreateCaseRequest, OpenCaseRequest},
    dto::CaseSummaryDto,
    CommandError,
};

use super::recent::remember_recent_case;
use crate::{events::event_bridge, state::AppState};

fn init_case_db(state: &AppState) -> Result<(), CommandError> {
    // AppState methods are typed `Result<_, String>` today, so this stays on the
    // substring-matching fallback path.
    state
        .init_db_pragmas()
        .map_err(CommandError::from_service_error)
}

fn clear_previous_preview_runtime(state: &AppState) -> Result<(), CommandError> {
    let previous_case_id = state
        .active_case
        .lock()
        .map_err(|error| CommandError::from_lock_error("Case", error))?
        .as_ref()
        .map(|active| active.meta.id.0.clone());
    if let Some(case_id) = previous_case_id {
        state
            .clear_preview_runtime_for_case(&case_id)
            .map_err(CommandError::from_service_error)?;
        app_services::file_service::clear_e01_reader_cache_for_case(&case_id);
    }
    Ok(())
}

pub(super) fn meta_to_dto(meta: &domain::CaseMeta) -> CaseSummaryDto {
    CaseSummaryDto {
        id: meta.id.0.clone(),
        name: meta.name.clone(),
        number: meta.number.clone(),
        examiner: meta.examiner.clone(),
        created_at: meta.created_at.to_rfc3339(),
        updated_at: meta.updated_at.to_rfc3339(),
    }
}

#[tauri::command]
pub fn create_case(
    state: State<AppState>,
    app: AppHandle,
    request: CreateCaseRequest,
) -> Result<CaseSummaryDto, CommandError> {
    request.validate().map_err(CommandError::invalid_input)?;
    let root = PathBuf::from(&request.case_root);
    let active = case_service::create_case(&root, &request.name, request.examiner.as_deref())
        .map_err(CommandError::from_typed_service_error)?;
    let active_case_root = active.case_root.clone();
    let dto = meta_to_dto(&active.meta);
    let setup_result = (|| {
        clear_previous_preview_runtime(&state)?;
        {
            let mut guard = state
                .active_case
                .lock()
                .map_err(|e| CommandError::from_lock_error("Case", e))?;
            *guard = Some(active);
        }
        state
            .reactivate_preview_case(&dto.id)
            .map_err(CommandError::from_service_error)?;
        init_case_db(&state)?;
        remember_recent_case(&active_case_root, &dto)
    })();
    if let Err(error) = setup_result {
        match state.active_case.lock() {
            Ok(mut guard) => *guard = None,
            Err(lock_error) => {
                tracing::error!("Failed to clear active case during rollback: {lock_error}");
            }
        }
        let _ = state.clear_db_state();
        let _ = state.clear_preview_runtime_for_case(&dto.id);
        app_services::file_service::clear_e01_reader_cache();
        if let Err(cleanup_error) = case_service::delete_case(&active_case_root) {
            tracing::error!(
                "Failed to roll back case creation at {}: {}",
                active_case_root.display(),
                cleanup_error
            );
        }
        return Err(error);
    }
    event_bridge::emit_case_opened(&app, &dto.id, &dto.name);
    Ok(dto)
}

#[tauri::command]
pub fn open_case(
    state: State<AppState>,
    app: AppHandle,
    request: OpenCaseRequest,
) -> Result<CaseSummaryDto, CommandError> {
    request.validate().map_err(CommandError::invalid_input)?;
    let root = PathBuf::from(&request.case_root);
    let active = case_service::open_case(&root).map_err(CommandError::from_typed_service_error)?;
    let dto = meta_to_dto(&active.meta);
    clear_previous_preview_runtime(&state)?;
    {
        let mut guard = state
            .active_case
            .lock()
            .map_err(|e| CommandError::from_lock_error("Case", e))?;
        *guard = Some(active);
    }
    state
        .reactivate_preview_case(&dto.id)
        .map_err(CommandError::from_service_error)?;
    init_case_db(&state)?;
    recover_interrupted_jobs(&state);
    remember_recent_case(&root, &dto)?;
    event_bridge::emit_case_opened(&app, &dto.id, &dto.name);
    Ok(dto)
}

fn recover_interrupted_jobs(state: &AppState) {
    // Recovery is best-effort so a stale job cannot prevent the case from opening.
    match state.get_connection() {
        Ok(conn) => match job_service::recover_interrupted_jobs(&conn) {
            Ok(recovery) => {
                if !recovery.recovered_job_ids.is_empty() {
                    tracing::info!(
                        "Recovered {} interrupted job(s): {:?}",
                        recovery.recovered_job_ids.len(),
                        recovery.recovered_job_ids
                    );
                }
            }
            Err(error) => {
                tracing::warn!("Failed to recover interrupted jobs on case open: {error}");
            }
        },
        Err(error) => {
            tracing::warn!("Failed to get connection for job recovery on case open: {error}");
        }
    }
}

#[tauri::command]
pub fn create_analysis_demo_case(
    state: State<AppState>,
    app: AppHandle,
) -> Result<CaseSummaryDto, CommandError> {
    let case_root = std::env::temp_dir().join("Meow_Detective-analysis-demo");
    if case_root.exists() {
        std::fs::remove_dir_all(&case_root).map_err(|e| {
            CommandError::internal(format!("Failed to reset analysis demo case: {e}"))
        })?;
    }
    std::fs::create_dir_all(&case_root)
        .map_err(|e| CommandError::internal(format!("Failed to create analysis demo root: {e}")))?;

    let active = case_service::create_case(&case_root, "Analysis Demo", Some("Codex Demo"))
        .map_err(CommandError::from_typed_service_error)?;
    app_services::analysis_service::seed_analysis_demo_data(&active)
        .map_err(CommandError::from_typed_service_error)?;
    let dto = meta_to_dto(&active.meta);
    clear_previous_preview_runtime(&state)?;
    {
        let mut guard = state
            .active_case
            .lock()
            .map_err(|e| CommandError::from_lock_error("Case", e))?;
        *guard = Some(active);
    }
    state
        .reactivate_preview_case(&dto.id)
        .map_err(CommandError::from_service_error)?;
    init_case_db(&state)?;
    remember_recent_case(&case_root.join("Analysis Demo"), &dto)?;
    event_bridge::emit_case_opened(&app, &dto.id, &dto.name);
    Ok(dto)
}

#[tauri::command]
pub fn get_current_case(state: State<AppState>) -> Result<Option<CaseSummaryDto>, CommandError> {
    let guard = state
        .active_case
        .lock()
        .map_err(|e| CommandError::from_lock_error("Case", e))?;
    match guard.as_ref() {
        Some(active) => Ok(Some(meta_to_dto(&active.meta))),
        None => Ok(None),
    }
}
