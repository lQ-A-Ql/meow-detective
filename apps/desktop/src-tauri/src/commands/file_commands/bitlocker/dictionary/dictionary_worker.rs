use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use app_services::bitlocker_service::{
    self, DictionaryAttackOutcome, DictionaryAttackProgress, DictionaryAttackRequest,
};
use domain::{CaseId, DataSourceId};
use transport::{
    dto::{BitLockerDictionaryAttackDto, BitLockerDictionaryBackendDto},
    ServiceErrorCategory,
};

use crate::state::AppState;

#[allow(clippy::too_many_arguments)]
pub(super) fn run_dictionary_attack(
    app_state: AppState,
    registry_key: String,
    task_id: String,
    case_root: PathBuf,
    case_id: String,
    data_source_id: String,
    partition_index: u32,
    dictionary_path: PathBuf,
    backend: BitLockerDictionaryBackendDto,
    cancel_token: Arc<AtomicBool>,
) -> Result<(), String> {
    let starting_phase = if app_state
        .bitlocker_dictionary_attacks
        .get(&registry_key)
        .is_some_and(|state| state.phase == "cancelling")
    {
        "cancelling"
    } else {
        "running"
    };
    set_phase(
        &app_state,
        &registry_key,
        task_id.clone(),
        starting_phase,
        DictionaryAttackProgress {
            tested_candidates: 0,
            bytes_processed: 0,
            total_bytes: std::fs::metadata(&dictionary_path)
                .map(|metadata| metadata.len())
                .unwrap_or_default(),
        },
        None,
    );
    let connection = match app_services::connection::open_case_db(&case_root.join("app.db")) {
        Ok(connection) => connection,
        Err(_) => {
            let code = "BITLOCKER_DICTIONARY_CASE_DB_FAILED";
            set_failure(&app_state, &registry_key, task_id, code);
            return Err(code.to_string());
        }
    };
    let preview_runtime = app_state.preview_runtime.clone();
    let bitlocker_runtime = app_state.bitlocker_runtime.clone();
    let key_store = app_state.bitlocker_key_store.clone();
    let progress_key = registry_key.clone();
    let progress_task = task_id.clone();
    let runtimes = bitlocker_service::BitLockerRuntimeContext::new(
        &preview_runtime,
        &bitlocker_runtime,
        key_store.as_ref(),
    );
    let result = bitlocker_service::try_password_dictionary(
        DictionaryAttackRequest {
            case_conn: &connection,
            case_root: &case_root,
            case_id: &CaseId(case_id),
            data_source_id: &DataSourceId(data_source_id),
            partition_index,
            dictionary_path: &dictionary_path,
            backend,
            runtimes,
            cancel_token: &cancel_token,
        },
        |progress| {
            let cancelling = app_state
                .bitlocker_dictionary_attacks
                .get(&progress_key)
                .is_some_and(|state| state.phase == "cancelling");
            let phase = if cancelling { "cancelling" } else { "running" };
            set_phase(
                &app_state,
                &progress_key,
                progress_task.clone(),
                phase,
                progress,
                None,
            );
        },
    );
    settle_dictionary_result(&app_state, &registry_key, task_id, result)
}

fn settle_dictionary_result(
    app_state: &AppState,
    registry_key: &str,
    task_id: String,
    result: Result<DictionaryAttackOutcome, bitlocker_service::BitLockerServiceError>,
) -> Result<(), String> {
    let (phase, progress) = match result {
        Ok(DictionaryAttackOutcome::Found { progress }) => ("found", progress),
        Ok(DictionaryAttackOutcome::Exhausted { progress }) => ("exhausted", progress),
        Ok(DictionaryAttackOutcome::Cancelled { progress }) => ("cancelled", progress),
        Err(error) => {
            let code = error.code().unwrap_or("BITLOCKER_DICTIONARY_FAILED");
            set_failure(app_state, registry_key, task_id, code);
            return Err(code.to_string());
        }
    };
    set_phase(app_state, registry_key, task_id, phase, progress, None);
    Ok(())
}

pub(super) fn set_failure(app_state: &AppState, key: &str, task_id: String, code: &str) {
    let previous = app_state.bitlocker_dictionary_attacks.get(key);
    let progress = previous.map_or(
        DictionaryAttackProgress {
            tested_candidates: 0,
            bytes_processed: 0,
            total_bytes: 0,
        },
        |state| DictionaryAttackProgress {
            tested_candidates: state.tested_candidates,
            bytes_processed: state.bytes_processed,
            total_bytes: state.total_bytes,
        },
    );
    set_phase(
        app_state,
        key,
        task_id,
        "failed",
        progress,
        Some(code.to_string()),
    );
}

fn set_phase(
    app_state: &AppState,
    key: &str,
    task_id: String,
    phase: &str,
    progress: DictionaryAttackProgress,
    error: Option<String>,
) {
    let backend = app_state
        .bitlocker_dictionary_attacks
        .get(key)
        .map(|state| state.backend)
        .unwrap_or(BitLockerDictionaryBackendDto::Cpu);
    app_state.bitlocker_dictionary_attacks.set(
        key.to_string(),
        state_dto(task_id, phase, progress, error, backend),
    );
}

pub(super) fn state_dto(
    task_id: String,
    phase: &str,
    progress: DictionaryAttackProgress,
    error: Option<String>,
    backend: BitLockerDictionaryBackendDto,
) -> BitLockerDictionaryAttackDto {
    BitLockerDictionaryAttackDto {
        task_id,
        phase: phase.to_string(),
        backend,
        tested_candidates: progress.tested_candidates,
        bytes_processed: progress.bytes_processed,
        total_bytes: progress.total_bytes,
        error,
    }
}
