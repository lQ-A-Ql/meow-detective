use crate::commands::command_support::{
    get_case_connection, get_case_read_connection, require_active_case,
};
use crate::post_import_scheduler::{cancel_source, retry_admission, Admission, QueuedCapability};
use crate::state::AppState;
use app_services::capability_service;
use tauri::State;
use transport::dto::{CapabilityKindDto, CapabilityMatrixDto, CapabilityStateDto};
use transport::CommandError;

/// Read the derived per-source capability matrix for the active case.
///
/// L0 is read-only: the matrix is derived from persisted markers; no import
/// or post-processing state changes.
#[tauri::command]
pub async fn get_source_capabilities(
    state: State<'_, AppState>,
) -> Result<CapabilityMatrixDto, CommandError> {
    let app_state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let active = require_active_case(&app_state)?;
        let conn = get_case_read_connection(&app_state)?;
        capability_service::derive_capability_matrix(&conn, &active.case_root, &active.meta.id)
            .map_err(|error| CommandError::internal(error.to_string()))
    })
    .await
    .map_err(CommandError::from_join_error)?
}

/// Parse the caller-supplied capability wire string into a schedulable kind.
/// Only `timeline` and `artifacts` are executable by the post-import scheduler;
/// the other capabilities are derived at read time and have no worker path.
fn parse_schedulable(wire: &str) -> Result<(QueuedCapability, CapabilityKindDto), CommandError> {
    match wire {
        "timeline" => Ok((QueuedCapability::Timeline, CapabilityKindDto::Timeline)),
        "artifacts" => Ok((QueuedCapability::Artifacts, CapabilityKindDto::Artifacts)),
        other => Err(CommandError::invalid_input(format!(
            "capability '{other}' cannot be retried; only timeline and artifacts are schedulable"
        ))),
    }
}

/// Map the service-layer source check onto a command error: unknown source →
/// typed not-found; anything else → internal.
fn ensure_source(
    conn: &rusqlite::Connection,
    active: &crate::commands::command_support::ActiveCaseSnapshot,
    data_source_id: &str,
) -> Result<(), CommandError> {
    capability_service::ensure_source_belongs(conn, &active.meta.id, data_source_id).map_err(
        |error| match error {
            app_services::capability_service::CapabilityServiceError::SourceNotFound(_) => {
                CommandError::not_found("data source")
            }
            other => CommandError::internal(other.to_string()),
        },
    )
}

/// P0 user retry: mark the row `queued` and admit a high-priority task.
///
/// Already-queued/running dedup returns a typed conflict (RebuildInProgress
/// semantic); a full per-case P0 slot returns a typed busy conflict instead
/// of growing the queue without bound. Returns the refreshed matrix.
#[tauri::command]
pub async fn retry_source_capability(
    state: State<'_, AppState>,
    app: tauri::AppHandle,
    data_source_id: String,
    capability: String,
) -> Result<CapabilityMatrixDto, CommandError> {
    let app_state = state.inner().clone();
    let app = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let active = require_active_case(&app_state)?;
        let (schedulable, kind) = parse_schedulable(&capability)?;
        let conn = get_case_connection(&app_state)?;
        ensure_source(&conn, &active, &data_source_id)?;
        capability_service::mark_capability_state(
            &conn,
            &active.meta.id,
            &data_source_id,
            kind,
            CapabilityStateDto::Queued,
            None,
            Some("user retry".to_string()),
            None,
            false,
        )
        .map_err(|error| CommandError::internal(error.to_string()))?;
        match retry_admission(
            &app_state,
            Some(&app),
            &active.meta.id.0,
            &active.case_root,
            &data_source_id,
            schedulable,
        ) {
            Admission::Accepted => {}
            Admission::AlreadyQueued => {
                return Err(CommandError::conflict(
                    "capability rebuild already in progress",
                ));
            }
            Admission::Busy => {
                return Err(CommandError::conflict(
                    "retry queue for this case is full; try again shortly",
                ));
            }
        }
        let read = get_case_read_connection(&app_state)?;
        capability_service::derive_capability_matrix(&read, &active.case_root, &active.meta.id)
            .map_err(|error| CommandError::internal(error.to_string()))
    })
    .await
    .map_err(CommandError::from_join_error)?
}

/// Cancel a queued or running post-processing task. A running task observes
/// the token at the next chunk boundary; the row is written `cancelled`.
#[tauri::command]
pub async fn cancel_source_capability(
    state: State<'_, AppState>,
    data_source_id: String,
    capability: String,
) -> Result<(), CommandError> {
    let app_state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let active = require_active_case(&app_state)?;
        let (schedulable, kind) = parse_schedulable(&capability)?;
        let conn = get_case_connection(&app_state)?;
        ensure_source(&conn, &active, &data_source_id)?;
        cancel_source(&active.meta.id.0, &data_source_id, schedulable);
        capability_service::mark_capability_state(
            &conn,
            &active.meta.id,
            &data_source_id,
            kind,
            CapabilityStateDto::Cancelled,
            None,
            Some("cancelled by user".to_string()),
            None,
            false,
        )
        .map_err(|error| CommandError::internal(error.to_string()))?;
        Ok(())
    })
    .await
    .map_err(CommandError::from_join_error)?
}
