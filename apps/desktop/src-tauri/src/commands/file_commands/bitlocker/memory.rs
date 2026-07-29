use std::path::PathBuf;

use app_services::bitlocker_service;
use domain::{CaseId, DataSourceId};
use tauri::State;
use transport::{dto::BitLockerVolumeStatusDto, CommandError};

use crate::state::AppState;

use super::super::support::run_active_case_command;

#[tauri::command]
pub async fn unlock_bitlocker_with_memory_image(
    state: State<'_, AppState>,
    data_source_id: String,
    partition_index: u32,
    memory_image_path: String,
) -> Result<BitLockerVolumeStatusDto, CommandError> {
    let memory_image_path = validate_memory_image_path(memory_image_path)?;
    let app_state = state.inner().clone();
    let preview_runtime = app_state.preview_runtime.clone();
    let runtime = app_state.bitlocker_runtime.clone();
    let key_store = app_state.bitlocker_key_store.clone();
    run_active_case_command(app_state, move |connection, active| {
        let runtimes = bitlocker_service::BitLockerRuntimeContext::new(
            &preview_runtime,
            &runtime,
            key_store.as_ref(),
        );
        bitlocker_service::unlock_bitlocker_with_memory_image(
            connection,
            &active.case_root,
            &CaseId(active.case_id.clone()),
            &DataSourceId(data_source_id),
            partition_index,
            &memory_image_path,
            runtimes,
        )
        .map_err(CommandError::from_typed_service_error)
    })
    .await
}

fn validate_memory_image_path(memory_image_path: String) -> Result<PathBuf, CommandError> {
    let path = PathBuf::from(memory_image_path);
    if !path.is_file() || std::fs::File::open(&path).is_err() {
        return Err(CommandError::invalid_input(
            "The selected memory image is not a readable file",
        ));
    }
    Ok(path)
}

#[cfg(test)]
#[path = "../../../../tests/unit/commands/file_commands/bitlocker_memory.rs"]
mod tests;
