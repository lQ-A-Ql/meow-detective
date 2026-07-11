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
    let root = PathBuf::from(&request.case_root);
    let active_snapshot = {
        let guard = state
            .active_case
            .lock()
            .map_err(|e| CommandError::from_lock_error("Case", e))?;
        guard
            .as_ref()
            .map(|active| (active.case_root.clone(), active.meta.id.0.clone()))
    };
    let deleting_active_case = active_snapshot
        .as_ref()
        .map(|(active_root, _)| same_path(active_root, &root))
        .unwrap_or(false);

    if deleting_active_case {
        let timeout = Duration::from_secs(5);
        let case_id = active_snapshot
            .as_ref()
            .map(|(_, case_id)| case_id.as_str())
            .unwrap_or("");
        drain_active_case_jobs(&state, case_id, timeout);
    }

    let root_clone = root.clone();
    tauri::async_runtime::spawn_blocking(move || {
        case_service::delete_case_in(&root_clone).map_err(CommandError::from_typed_service_error)
    })
    .await
    .map_err(CommandError::from_join_error)??;

    if deleting_active_case {
        clear_deleted_active_case(&state, &app, active_snapshot.as_ref())?;
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
    active_snapshot: Option<&(PathBuf, String)>,
) -> Result<(), CommandError> {
    if let Some((_, case_id)) = active_snapshot {
        let _ = state.clear_runtime_cache_for_case(case_id);
        app_services::file_service::clear_e01_reader_cache();
        {
            let mut guard = state
                .active_case
                .lock()
                .map_err(|e| CommandError::from_lock_error("Case", e))?;
            *guard = None;
        }
        event_bridge::emit_case_closed(app, case_id);
    }
    state
        .clear_db_state()
        .map_err(CommandError::from_service_error)
}

#[tauri::command]
pub async fn delete_data_source(
    state: State<'_, AppState>,
    request: DeleteDataSourceRequest,
) -> Result<String, CommandError> {
    request.validate().map_err(CommandError::invalid_input)?;
    let app_state = state.inner().clone();
    let data_source_id = request.data_source_id.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let (db_path, case_root) = {
            let guard = app_state
                .active_case
                .lock()
                .map_err(|e| CommandError::from_lock_error("Case", e))?;
            let active = guard.as_ref().ok_or_else(CommandError::no_active_case)?;
            (active.db_path(), active.case_root.clone())
        };
        app_state.task_manager.cleanup_finished();
        let running_tasks = app_state.task_manager.task_count();
        if running_tasks > 0 {
            return Err(CommandError::conflict(format!(
                "Cannot delete data source while {running_tasks} background task(s) are running. Cancel or wait for imports to finish first."
            )));
        }
        let conn = app_services::connection::open_case_db(&db_path)
            .map_err(CommandError::from_typed_service_error)?;
        case_service::delete_data_source_in(&conn, &case_root, &data_source_id)
            .map_err(CommandError::from_typed_service_error)?;
        Ok("Data source deleted".to_string())
    })
    .await
    .map_err(CommandError::from_join_error)?
}
