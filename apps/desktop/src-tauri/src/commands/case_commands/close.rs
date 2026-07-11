use app_services::case_service;
use std::time::Duration;
use tauri::{AppHandle, State};
use transport::CommandError;

use crate::{events::event_bridge, state::AppState};

pub(super) fn drain_active_case_jobs(state: &AppState, case_id: &str, timeout: Duration) {
    state.task_manager.cancel_all();
    let _ = state.task_manager.wait_all(timeout);

    match state.get_connection() {
        Ok(conn) => {
            if let Err(error) =
                case_service::close_case_drain(&conn, case_id, timeout.as_millis() as u64)
            {
                tracing::warn!("Failed to drain jobs during case delete: {}", error);
            }
        }
        Err(error) => {
            tracing::warn!("Failed to get connection for case delete drain: {}", error);
        }
    }
}

fn drain_case_for_close(state: &AppState, timeout: Duration) -> Result<(), CommandError> {
    state.task_manager.cancel_all();
    let _ = state.task_manager.wait_all(timeout);

    match state.get_connection() {
        Ok(conn) => {
            let case_id = {
                let guard = state
                    .active_case
                    .lock()
                    .map_err(|e| CommandError::from_lock_error("Case", e))?;
                guard
                    .as_ref()
                    .map(|active| active.meta.id.0.clone())
                    .unwrap_or_default()
            };
            match case_service::close_case_drain(&conn, &case_id, timeout.as_millis() as u64) {
                Ok(drain) => log_degraded_close(&drain, timeout),
                Err(error) => {
                    tracing::warn!("Failed to drain jobs during case close: {}", error);
                }
            }
        }
        Err(error) => {
            tracing::warn!("Failed to get connection for case close drain: {}", error);
        }
    }
    Ok(())
}

fn log_degraded_close(drain: &app_services::case_service::DrainResult, timeout: Duration) {
    if drain.fully_drained {
        return;
    }
    tracing::warn!(
        "Degraded case close - {} job(s) did not drain within {}ms: {:?}",
        drain.pending_jobs.len(),
        timeout.as_millis(),
        drain.pending_jobs,
    );
    for warning in &drain.warnings {
        tracing::warn!("{}", warning);
    }
}

#[tauri::command]
pub fn close_case(state: State<AppState>, app: AppHandle) -> Result<(), CommandError> {
    let timeout = Duration::from_secs(5);
    drain_case_for_close(&state, timeout)?;

    let closed_case_id = state
        .active_case
        .lock()
        .map_err(|e| CommandError::from_lock_error("Case", e))?
        .as_ref()
        .map(|active| active.meta.id.0.clone());

    // Clear active case state only after all drain logic finishes.
    state
        .clear_db_state()
        .map_err(CommandError::from_service_error)?;

    if let Some(case_id) = &closed_case_id {
        let _ = state.clear_runtime_cache_for_case(case_id);
    }
    app_services::file_service::clear_e01_reader_cache();
    if let Some(case_id) = closed_case_id {
        event_bridge::emit_case_closed(&app, &case_id);
    }
    Ok(())
}
