use app_services::bitlocker_service;
use domain::{CaseId, DataSourceId};
use tauri::State;
use transport::{dto::BitLockerCatalogImportDto, dto::BitLockerVolumeStatusDto, CommandError};
use volume_bitlocker::Passphrase;

use crate::state::AppState;

use super::support::run_active_case_command;

#[tauri::command]
pub async fn inspect_bitlocker_volume(
    state: State<'_, AppState>,
    data_source_id: String,
    partition_index: u32,
) -> Result<BitLockerVolumeStatusDto, CommandError> {
    let app_state = state.inner().clone();
    let preview_runtime = app_state.preview_runtime.clone();
    let runtime = app_state.bitlocker_runtime.clone();
    run_active_case_command(app_state, move |connection, active| {
        let runtimes = bitlocker_service::BitLockerRuntimeContext::new(&preview_runtime, &runtime);
        bitlocker_service::inspect_bitlocker_volume(
            connection,
            &active.case_root,
            &CaseId(active.case_id.clone()),
            &DataSourceId(data_source_id),
            partition_index,
            runtimes,
        )
        .map_err(CommandError::from_typed_service_error)
    })
    .await
}

#[tauri::command]
pub async fn unlock_bitlocker_with_password(
    state: State<'_, AppState>,
    data_source_id: String,
    partition_index: u32,
    credential: String,
) -> Result<BitLockerVolumeStatusDto, CommandError> {
    let credential = required_credential(credential)?;
    let app_state = state.inner().clone();
    let preview_runtime = app_state.preview_runtime.clone();
    let runtime = app_state.bitlocker_runtime.clone();
    run_active_case_command(app_state, move |connection, active| {
        let runtimes = bitlocker_service::BitLockerRuntimeContext::new(&preview_runtime, &runtime);
        bitlocker_service::unlock_bitlocker_with_password(
            connection,
            &active.case_root,
            &CaseId(active.case_id.clone()),
            &DataSourceId(data_source_id),
            partition_index,
            credential,
            runtimes,
        )
        .map_err(CommandError::from_typed_service_error)
    })
    .await
}

#[tauri::command]
pub async fn unlock_bitlocker_with_recovery_password(
    state: State<'_, AppState>,
    data_source_id: String,
    partition_index: u32,
    credential: String,
) -> Result<BitLockerVolumeStatusDto, CommandError> {
    let credential = required_credential(credential)?;
    let app_state = state.inner().clone();
    let preview_runtime = app_state.preview_runtime.clone();
    let runtime = app_state.bitlocker_runtime.clone();
    run_active_case_command(app_state, move |connection, active| {
        let runtimes = bitlocker_service::BitLockerRuntimeContext::new(&preview_runtime, &runtime);
        bitlocker_service::unlock_bitlocker_with_recovery_password(
            connection,
            &active.case_root,
            &CaseId(active.case_id.clone()),
            &DataSourceId(data_source_id),
            partition_index,
            credential,
            runtimes,
        )
        .map_err(CommandError::from_typed_service_error)
    })
    .await
}

#[tauri::command]
pub async fn import_unlocked_bitlocker_catalog(
    state: State<'_, AppState>,
    data_source_id: String,
    partition_index: u32,
) -> Result<BitLockerCatalogImportDto, CommandError> {
    let app_state = state.inner().clone();
    let preview_runtime = app_state.preview_runtime.clone();
    let bitlocker_runtime = app_state.bitlocker_runtime.clone();
    run_active_case_command(app_state, move |connection, active| {
        let runtimes =
            bitlocker_service::BitLockerRuntimeContext::new(&preview_runtime, &bitlocker_runtime);
        bitlocker_service::import_unlocked_bitlocker_catalog(
            connection,
            &active.case_root,
            &CaseId(active.case_id.clone()),
            &DataSourceId(data_source_id),
            partition_index,
            runtimes,
        )
        .map_err(CommandError::from_typed_service_error)
    })
    .await
}

#[tauri::command]
pub async fn lock_bitlocker_volume(
    state: State<'_, AppState>,
    data_source_id: String,
    partition_index: u32,
) -> Result<BitLockerVolumeStatusDto, CommandError> {
    let app_state = state.inner().clone();
    let preview_runtime = app_state.preview_runtime.clone();
    let bitlocker_runtime = app_state.bitlocker_runtime.clone();
    run_active_case_command(app_state, move |connection, active| {
        let runtimes =
            bitlocker_service::BitLockerRuntimeContext::new(&preview_runtime, &bitlocker_runtime);
        bitlocker_service::lock_bitlocker_volume(
            connection,
            &active.case_root,
            &CaseId(active.case_id.clone()),
            &DataSourceId(data_source_id),
            partition_index,
            runtimes,
        )
        .map_err(CommandError::from_typed_service_error)
    })
    .await
}

fn required_credential(value: String) -> Result<Passphrase, CommandError> {
    let credential = Passphrase::new(value);
    if credential.is_empty() {
        return Err(CommandError::invalid_input(
            "A BitLocker credential is required",
        ));
    }
    Ok(credential)
}
