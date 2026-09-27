use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use app_services::bitlocker_service::DictionaryAttackProgress;
use tauri::State;
use transport::{dto::BitLockerDictionaryAttackDto, CommandError};

use crate::commands::command_support::require_active_case;
use crate::state::{AppState, TaskRegistrationError, TaskScope};

mod dictionary_worker;
use dictionary_worker::{run_dictionary_attack, set_failure, state_dto};

const TASK_PREFIX: &str = "bitlocker-dictionary";

#[tauri::command]
pub async fn start_bitlocker_dictionary_attack(
    state: State<'_, AppState>,
    data_source_id: String,
    partition_index: u32,
    dictionary_path: String,
) -> Result<BitLockerDictionaryAttackDto, CommandError> {
    let app_state = state.inner().clone();
    let active = require_active_case(&app_state)?;
    let case_id = active.case_id.clone();
    let task_id = task_id(&case_id, &data_source_id, partition_index);
    let existing_key = registry_key(&case_id, &data_source_id, partition_index);
    if app_state.task_manager.is_running(&task_id) {
        return Err(CommandError::conflict(
            "BitLocker dictionary attack is already running",
        ));
    }
    reconcile_previous_attack(&app_state, &existing_key);
    let total_bytes = app_services::bitlocker_service::validate_dictionary_path(
        std::path::Path::new(&dictionary_path),
    )
    .map_err(CommandError::from_typed_service_error)?;
    let registry_key = registry_key(&case_id, &data_source_id, partition_index);
    let queued = state_dto(
        task_id.clone(),
        "queued",
        DictionaryAttackProgress {
            tested_candidates: 0,
            bytes_processed: 0,
            total_bytes,
        },
        None,
    );
    app_state
        .bitlocker_dictionary_attacks
        .set(registry_key.clone(), queued.clone());
    let cancel_token = Arc::new(AtomicBool::new(false));
    let worker_cancel = Arc::clone(&cancel_token);
    let worker_state = app_state.clone();
    let worker_case_root = active.case_root.clone();
    let worker_case_id = case_id.clone();
    let worker_source_id = data_source_id.clone();
    let worker_registry_key = registry_key.clone();
    let worker_task_id = task_id.clone();
    let failure_task_id = task_id.clone();
    let worker_dictionary_path = PathBuf::from(dictionary_path);
    let scope = TaskScope::data_source(&case_id, &data_source_id, &task_id);
    let registration =
        app_state
            .task_manager
            .spawn_scoped_heavy(task_id, scope, cancel_token, move || {
                run_dictionary_attack(
                    worker_state,
                    worker_registry_key,
                    worker_task_id,
                    worker_case_root,
                    worker_case_id,
                    worker_source_id,
                    partition_index,
                    worker_dictionary_path,
                    worker_cancel,
                )
            });
    if let Err(error) = registration {
        if matches!(error, TaskRegistrationError::DuplicateTaskId(_)) {
            return Err(CommandError::conflict(
                "BitLocker dictionary attack is already running",
            ));
        }
        set_failure(
            &app_state,
            &registry_key,
            failure_task_id,
            "BITLOCKER_DICTIONARY_TASK_START_FAILED",
        );
        tracing::warn!(error = %error, "BitLocker dictionary task failed to start");
        return Err(CommandError::internal(
            "BitLocker dictionary task failed to start",
        ));
    }
    Ok(queued)
}

fn reconcile_previous_attack(app_state: &AppState, key: &str) {
    let Some(existing) = app_state.bitlocker_dictionary_attacks.get(key) else {
        return;
    };
    match existing.phase.as_str() {
        "cancelling" => app_state.bitlocker_dictionary_attacks.set(
            key,
            BitLockerDictionaryAttackDto {
                phase: "cancelled".to_string(),
                ..existing
            },
        ),
        "queued" | "running" => set_failure(
            app_state,
            key,
            existing.task_id,
            "BITLOCKER_DICTIONARY_TASK_FAILED",
        ),
        _ => {}
    }
}

#[tauri::command]
pub fn get_bitlocker_dictionary_attack_status(
    state: State<'_, AppState>,
    data_source_id: String,
    partition_index: u32,
) -> Result<Option<BitLockerDictionaryAttackDto>, CommandError> {
    let active = require_active_case(state.inner())?;
    let key = registry_key(&active.case_id, &data_source_id, partition_index);
    let status = state.inner().bitlocker_dictionary_attacks.get(&key);
    if let Some(mut current) = status {
        if matches!(current.phase.as_str(), "queued" | "running" | "cancelling")
            && !state.inner().task_manager.is_running(&current.task_id)
        {
            if current.phase == "cancelling" {
                current.phase = "cancelled".to_string();
            } else {
                current.phase = "failed".to_string();
                current.error = Some("BITLOCKER_DICTIONARY_TASK_FAILED".to_string());
            }
            state
                .inner()
                .bitlocker_dictionary_attacks
                .set(key, current.clone());
        }
        return Ok(Some(current));
    }
    Ok(None)
}

#[tauri::command]
pub fn cancel_bitlocker_dictionary_attack(
    state: State<'_, AppState>,
    data_source_id: String,
    partition_index: u32,
) -> Result<bool, CommandError> {
    let active = require_active_case(state.inner())?;
    let task = task_id(&active.case_id, &data_source_id, partition_index);
    let requested = state.inner().task_manager.cancel(&task);
    if requested {
        let key = registry_key(&active.case_id, &data_source_id, partition_index);
        if let Some(previous) = state.inner().bitlocker_dictionary_attacks.get(&key) {
            state.inner().bitlocker_dictionary_attacks.set(
                key,
                BitLockerDictionaryAttackDto {
                    phase: "cancelling".to_string(),
                    ..previous
                },
            );
        }
    }
    Ok(requested)
}

fn registry_key(case_id: &str, data_source_id: &str, partition_index: u32) -> String {
    crate::state::BitLockerDictionaryAttackRegistry::key(case_id, data_source_id, partition_index)
}

fn task_id(case_id: &str, data_source_id: &str, partition_index: u32) -> String {
    format!("{TASK_PREFIX}:{case_id}:{data_source_id}:{partition_index}")
}
