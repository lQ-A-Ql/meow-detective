use tauri::State;
use transport::{
    dto::{
        GetLedgerProofRequest, GetLedgerRequest, LedgerBatchDto, LedgerProofDto, LedgerSnapshotDto,
    },
    CommandError,
};

use super::command_support::{get_case_connection, require_active_case};
use crate::state::AppState;

#[tauri::command]
pub async fn get_ledger_snapshot(
    state: State<'_, AppState>,
    request: GetLedgerRequest,
) -> Result<LedgerSnapshotDto, CommandError> {
    let app_state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let active = require_active_case(&app_state)?;
        let conn = get_case_connection(&app_state)?;
        app_services::ledger_service::get_snapshot(&conn, &active.case_id, request)
            .map_err(CommandError::from_typed_service_error)
    })
    .await
    .map_err(CommandError::from_join_error)?
}

#[tauri::command]
pub async fn seal_ledger_batch(
    state: State<'_, AppState>,
) -> Result<Option<LedgerBatchDto>, CommandError> {
    let app_state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let active = require_active_case(&app_state)?;
        let conn = get_case_connection(&app_state)?;
        app_services::ledger_service::seal_next_batch(&conn, &active.case_id)
            .map_err(CommandError::from_typed_service_error)
    })
    .await
    .map_err(CommandError::from_join_error)?
}

#[tauri::command]
pub async fn get_ledger_proof(
    state: State<'_, AppState>,
    request: GetLedgerProofRequest,
) -> Result<LedgerProofDto, CommandError> {
    let app_state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let active = require_active_case(&app_state)?;
        let conn = get_case_connection(&app_state)?;
        app_services::ledger_service::get_proof(&conn, &active.case_id, request)
            .map_err(CommandError::from_typed_service_error)
    })
    .await
    .map_err(CommandError::from_join_error)?
}
