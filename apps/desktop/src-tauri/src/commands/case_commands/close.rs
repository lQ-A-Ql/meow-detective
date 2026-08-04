use app_services::case_service;
use std::time::Duration;
use tauri::{AppHandle, State};
use transport::CommandError;

use super::transition::{active_case_identity, clear_active_case_if_matches};
use crate::{events::event_bridge, state::AppState};

pub(super) fn drain_active_case_jobs(
    state: &AppState,
    case_id: &str,
    timeout: Duration,
) -> Result<(), CommandError> {
    let _ = state.task_manager.retire_case_and_drain(case_id, timeout);
    let remaining_tasks = state.task_manager.task_count_for_case(case_id);
    if remaining_tasks > 0 {
        state.task_manager.reactivate_case(case_id);
        return Err(CommandError::timeout(format!(
            "Timed out waiting for {remaining_tasks} background task(s) in the active case to stop"
        )));
    }

    match state.get_connection() {
        Ok(conn) => {
            let drain = case_service::close_case_drain(&conn, case_id, timeout.as_millis() as u64)
                .map_err(CommandError::from_typed_service_error)?;
            if !drain.fully_drained {
                log_degraded_close(&drain, timeout);
                state.task_manager.reactivate_case(case_id);
                return Err(CommandError::timeout(format!(
                    "Timed out waiting for {} persisted job(s) in the active case to stop",
                    drain.pending_jobs.len()
                )));
            }
        }
        Err(error) => {
            state.task_manager.reactivate_case(case_id);
            return Err(CommandError::from_service_error(error));
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
pub async fn close_case(state: State<'_, AppState>, app: AppHandle) -> Result<(), CommandError> {
    let app_state = state.inner().clone();
    let _lifecycle = app_state.case_lifecycle.lock().await;
    let timeout = Duration::from_secs(5);
    let Some(identity) = active_case_identity(&app_state)? else {
        return Ok(());
    };
    drain_active_case_jobs(&app_state, &identity.case_id, timeout)?;
    let drained = app_state
        .retire_preview_case(&identity.case_id, timeout)
        .map_err(CommandError::from_service_error)?;
    if !drained {
        app_state.task_manager.reactivate_case(&identity.case_id);
        let _ = app_state.reactivate_preview_case(&identity.case_id);
        return Err(CommandError::timeout(
            "Timed out waiting for active preview reads to finish",
        ));
    }
    if let Err(error) = app_state.cleanup_mounts_for_case(&identity.case_id) {
        app_state.task_manager.reactivate_case(&identity.case_id);
        let _ = app_state.reactivate_preview_case(&identity.case_id);
        return Err(CommandError::from_service_error(error));
    }

    if !clear_active_case_if_matches(&app_state, &identity)? {
        app_state.task_manager.reactivate_case(&identity.case_id);
        let _ = app_state.reactivate_preview_case(&identity.case_id);
        return Err(CommandError::conflict(
            "Active case changed while it was being closed",
        ));
    }

    let _ = app_state.clear_runtime_cache_for_case(&identity.case_id);
    app_services::file_service::clear_e01_reader_cache_for_case(&identity.case_id);
    event_bridge::emit_case_closed(&app, &identity.case_id);
    Ok(())
}
