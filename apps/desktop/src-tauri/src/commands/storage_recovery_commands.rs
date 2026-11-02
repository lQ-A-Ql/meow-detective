use crate::{
    commands::command_support::{get_case_connection, require_active_case},
    state::AppState,
};
use app_services::storage_recovery_service::{self, RecoveryDisposition};
use tauri::State;
use transport::{
    dto::{
        RepairStorageOperationRequestDto, StorageRecoveryDispositionDto, StorageRecoveryItemDto,
        StorageRecoveryReportDto,
    },
    CommandError,
};

fn disposition(value: RecoveryDisposition) -> StorageRecoveryDispositionDto {
    match value {
        RecoveryDisposition::Ready => StorageRecoveryDispositionDto::Ready,
        RecoveryDisposition::RecoveryRequired => StorageRecoveryDispositionDto::RecoveryRequired,
        RecoveryDisposition::RepairRequired => StorageRecoveryDispositionDto::RepairRequired,
    }
}

#[tauri::command]
pub async fn get_storage_recovery_report(
    state: State<'_, AppState>,
) -> Result<StorageRecoveryReportDto, CommandError> {
    let active = require_active_case(state.inner())?;
    let connection = get_case_connection(state.inner())?;
    let report =
        storage_recovery_service::scan_case(&connection, &active.case_root, &active.case_id)
            .map_err(|error| CommandError::internal(error.to_string()))?;
    Ok(StorageRecoveryReportDto {
        disposition: disposition(report.disposition),
        items: report
            .items
            .into_iter()
            .map(|item| StorageRecoveryItemDto {
                operation_id: item.operation_id,
                source_id: item.source_id,
                disposition: disposition(item.disposition),
                state: item.state.map(|state| format!("{state:?}")),
                detail: item.detail,
            })
            .collect(),
    })
}

#[tauri::command]
pub async fn repair_storage_operation(
    state: State<'_, AppState>,
    request: RepairStorageOperationRequestDto,
) -> Result<(), CommandError> {
    if request.operation_id.trim().is_empty() || request.reason.trim().is_empty() {
        return Err(CommandError::invalid_input(
            "operationId and reason are required",
        ));
    }
    let _active = require_active_case(state.inner())?;
    let connection = get_case_connection(state.inner())?;
    storage_recovery_service::repair_operation(&connection, &request.operation_id, &request.reason)
        .map_err(|error| CommandError::internal(error.to_string()))
}
