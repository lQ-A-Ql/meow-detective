use app_services::bitlocker_service;
use domain::{CaseId, DataSourceId};
use tauri::State;
use transport::{dto::BitLockerVolumeStatusDto, CommandError};

use crate::state::AppState;

use super::super::support::run_active_case_command;

#[tauri::command]
pub async fn restore_persisted_bitlocker_key(
    state: State<'_, AppState>,
    data_source_id: String,
    partition_index: u32,
) -> Result<BitLockerVolumeStatusDto, CommandError> {
    let app_state = state.inner().clone();
    let preview_runtime = app_state.preview_runtime.clone();
    let runtime = app_state.bitlocker_runtime.clone();
    let key_store = app_state.bitlocker_key_store.clone();
    run_active_case_command(app_state, move |connection, active| {
        let context = bitlocker_service::BitLockerRuntimeContext::new(
            &preview_runtime,
            &runtime,
            key_store.as_ref(),
        );
        bitlocker_service::restore_persisted_bitlocker_key(
            connection,
            &active.case_root,
            &CaseId(active.case_id.clone()),
            &DataSourceId(data_source_id),
            partition_index,
            context,
        )
        .map_err(CommandError::from_typed_service_error)
    })
    .await
}

#[tauri::command]
pub async fn forget_persisted_bitlocker_key(
    state: State<'_, AppState>,
    data_source_id: String,
    partition_index: u32,
) -> Result<BitLockerVolumeStatusDto, CommandError> {
    let app_state = state.inner().clone();
    let preview_runtime = app_state.preview_runtime.clone();
    let runtime = app_state.bitlocker_runtime.clone();
    let key_store = app_state.bitlocker_key_store.clone();
    run_active_case_command(app_state, move |connection, active| {
        let context = bitlocker_service::BitLockerRuntimeContext::new(
            &preview_runtime,
            &runtime,
            key_store.as_ref(),
        );
        bitlocker_service::forget_persisted_bitlocker_key(
            connection,
            &active.case_root,
            &CaseId(active.case_id.clone()),
            &DataSourceId(data_source_id),
            partition_index,
            context,
        )
        .map_err(CommandError::from_typed_service_error)
    })
    .await
}
