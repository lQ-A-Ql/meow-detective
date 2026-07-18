use app_services::case_service;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tauri::{AppHandle, State};
use transport::{
    commands::{DeleteCaseRequest, DeleteDataSourceRequest},
    CommandError,
};

use super::{
    close::drain_active_case_jobs,
    recent::{read_recent_cases, save_recent_cases},
    transition::{active_case_identity, clear_active_case_if_matches, ActiveCaseIdentity},
};
use crate::{events::event_bridge, state::AppState};

fn same_path(left: &Path, right: &Path) -> bool {
    match (left.canonicalize(), right.canonicalize()) {
        (Ok(left), Ok(right)) => left == right,
        _ => left == right,
    }
}

#[tauri::command]
pub async fn delete_case(
    state: State<'_, AppState>,
    app: AppHandle,
    request: DeleteCaseRequest,
) -> Result<String, CommandError> {
    request.validate().map_err(CommandError::invalid_input)?;
    let app_state = state.inner().clone();
    let _lifecycle = app_state.case_lifecycle.lock().await;
    let root = PathBuf::from(&request.case_root);
    let active_snapshot = active_case_identity(&app_state)?;
    let deleting_active_case = active_snapshot
        .as_ref()
        .map(|active| same_path(&active.case_root, &root))
        .unwrap_or(false);

    if deleting_active_case {
        let timeout = Duration::from_secs(5);
        let case_id = active_snapshot
            .as_ref()
            .map(|active| active.case_id.as_str())
            .unwrap_or("");
        drain_active_case_jobs(&app_state, case_id, timeout)?;
        let drained = app_state
            .retire_preview_case(case_id, timeout)
            .map_err(CommandError::from_service_error)?;
        if !drained {
            app_state.task_manager.reactivate_case(case_id);
            let _ = app_state.reactivate_preview_case(case_id);
            return Err(CommandError::timeout(
                "Timed out waiting for active preview reads to finish",
            ));
        }
    }

    let root_clone = root.clone();
    let delete_result = tauri::async_runtime::spawn_blocking(move || {
        case_service::delete_case_in(&root_clone).map_err(CommandError::from_typed_service_error)
    })
    .await
    .map_err(CommandError::from_join_error)?;
    if let Err(error) = delete_result {
        if deleting_active_case {
            if let Some(active) = &active_snapshot {
                app_state.task_manager.reactivate_case(&active.case_id);
                let _ = app_state.reactivate_preview_case(&active.case_id);
            }
        }
        return Err(error);
    }

    if deleting_active_case {
        if let Some(active) = active_snapshot.as_ref() {
            clear_deleted_active_case(&app_state, &app, active)?;
        }
    }

    let mut recent = read_recent_cases().unwrap_or_else(|error| {
        tracing::warn!("Failed to read recent cases, starting fresh: {}", error);
        Vec::new()
    });
    recent.retain(|item| {
        item.case_root != request.case_root && !same_path(Path::new(&item.case_root), &root)
    });
    save_recent_cases(&recent)?;

    Ok("Case deleted".to_string())
}

fn clear_deleted_active_case(
    state: &AppState,
    app: &AppHandle,
    active: &ActiveCaseIdentity,
) -> Result<(), CommandError> {
    if !clear_active_case_if_matches(state, active)? {
        return Err(CommandError::conflict(
            "Active case changed while it was being deleted",
        ));
    }
    let _ = state.clear_runtime_cache_for_case(&active.case_id);
    app_services::file_service::clear_e01_reader_cache_for_case(&active.case_id);
    event_bridge::emit_case_closed(app, &active.case_id);
    Ok(())
}

#[tauri::command]
pub async fn delete_data_source(
    state: State<'_, AppState>,
    request: DeleteDataSourceRequest,
) -> Result<String, CommandError> {
    request.validate().map_err(CommandError::invalid_input)?;
    let app_state = state.inner().clone();
    let _lifecycle = app_state.case_lifecycle.lock().await;
    let worker_state = app_state.clone();
    let data_source_id = request.data_source_id.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let (db_path, case_root) = {
            let guard = worker_state
                .active_case
                .lock()
                .map_err(|e| CommandError::from_lock_error("Case", e))?;
            let active = guard.as_ref().ok_or_else(CommandError::no_active_case)?;
            (active.db_path(), active.case_root.clone())
        };
        let case_id = crate::commands::command_support::require_active_case(&worker_state)?
            .meta
            .id;
        let case_wide_tasks = worker_state
            .task_manager
            .task_count_for_case_wide(&case_id.0);
        if case_wide_tasks > 0 {
            return Err(CommandError::conflict(format!(
                "Cannot delete data source while {case_wide_tasks} case-level import task(s) are running. Cancel or wait for imports to finish first."
            )));
        }
        let timeout = Duration::from_secs(5);
        let _ = worker_state.task_manager.retire_source_and_drain(
            &case_id.0,
            &data_source_id,
            timeout,
        );
        let running_tasks = worker_state
            .task_manager
            .task_count_for_data_source(&case_id.0, &data_source_id);
        if running_tasks > 0 {
            worker_state
                .task_manager
                .reactivate_source(&case_id.0, &data_source_id);
            return Err(CommandError::timeout(format!(
                "Timed out waiting for {running_tasks} background task(s) for the data source to stop"
            )));
        }
        let conn = app_services::connection::open_case_db(&db_path)
            .map_err(CommandError::from_typed_service_error)?;
        let drained = worker_state
            .retire_preview_source(&case_id.0, &data_source_id, timeout)
            .map_err(CommandError::from_service_error)?;
        if !drained {
            worker_state
                .task_manager
                .reactivate_source(&case_id.0, &data_source_id);
            let _ = worker_state.reactivate_preview_source(&case_id.0, &data_source_id);
            return Err(CommandError::timeout(
                "Timed out waiting for data-source preview reads to finish",
            ));
        }
        app_services::file_service::clear_e01_reader_cache_for_case(&case_id.0);
        if let Err(error) =
            case_service::delete_data_source_in(&conn, &case_root, &data_source_id)
                .map_err(CommandError::from_typed_service_error)
        {
            worker_state
                .task_manager
                .reactivate_source(&case_id.0, &data_source_id);
            let _ = worker_state.reactivate_preview_source(&case_id.0, &data_source_id);
            return Err(error);
        }
        Ok("Data source deleted".to_string())
    })
    .await
    .map_err(CommandError::from_join_error)?
}
