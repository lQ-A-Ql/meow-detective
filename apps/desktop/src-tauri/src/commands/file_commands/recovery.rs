use std::path::{Path, PathBuf};

use app_services::deleted_recovery;
use domain::DataSourceId;
use tauri::State;

use transport::{
    commands::{
        ExportDeletedRecoveryRequest, ListDeletedRecoveriesRequest,
        ReadDeletedRecoveryRangeRequest, RunDeletedRecoveryRequest,
        SearchDeletedRecoveriesByHashRequest,
    },
    dto::{
        DeletedRecoveryContentRangeDto, DeletedRecoveryExportDto, DeletedRecoveryHashSearchDto,
        DeletedRecoveryPageDto, DeletedRecoveryRunDto,
    },
    CommandError,
};

use super::support::{run_active_case_command, write_file_extract_audit};
use crate::state::AppState;

#[tauri::command]
pub async fn list_deleted_recoveries(
    state: State<'_, AppState>,
    mut request: ListDeletedRecoveriesRequest,
) -> Result<DeletedRecoveryPageDto, CommandError> {
    request.validate().map_err(CommandError::invalid_input)?;
    let app_state = state.inner().clone();
    let data_source_id = DataSourceId(request.data_source_id);

    run_active_case_command(app_state, move |case_conn, active| {
        deleted_recovery::list_deleted_recoveries(
            case_conn,
            &active.case_root,
            &active.meta.id,
            &data_source_id,
            request.partition_index,
            request.offset,
            request.limit,
        )
        .map_err(CommandError::from_typed_service_error)
    })
    .await
}

#[tauri::command]
pub async fn search_deleted_recoveries_by_hash(
    state: State<'_, AppState>,
    mut request: SearchDeletedRecoveriesByHashRequest,
) -> Result<DeletedRecoveryHashSearchDto, CommandError> {
    request.validate().map_err(CommandError::invalid_input)?;
    let app_state = state.inner().clone();
    let algorithm = request.algorithm().map_err(CommandError::invalid_input)?;
    let data_source_id = DataSourceId(request.data_source_id);
    let normalized_hash = request.hash;

    run_active_case_command(app_state, move |case_conn, active| {
        deleted_recovery::search_deleted_recoveries_by_hash(
            case_conn,
            &active.case_root,
            &active.meta.id,
            &data_source_id,
            algorithm,
            &normalized_hash,
        )
        .map_err(CommandError::from_typed_service_error)
    })
    .await
}

#[tauri::command]
pub async fn run_deleted_recovery(
    state: State<'_, AppState>,
    request: RunDeletedRecoveryRequest,
) -> Result<DeletedRecoveryRunDto, CommandError> {
    request.validate().map_err(CommandError::invalid_input)?;
    let app_state = state.inner().clone();
    let bitlocker_runtime = app_state.bitlocker_runtime.clone();
    let data_source_id = DataSourceId(request.data_source_id);

    run_active_case_command(app_state, move |case_conn, active| {
        deleted_recovery::DeletedRecoveryContext::new(
            case_conn,
            &active.case_root,
            &active.meta.id,
            &data_source_id,
        )
        .with_bitlocker_runtime(bitlocker_runtime)
        .run(request.partition_index)
        .map_err(CommandError::from_typed_service_error)
    })
    .await
}

#[tauri::command]
pub async fn read_deleted_recovery_range(
    state: State<'_, AppState>,
    mut request: ReadDeletedRecoveryRangeRequest,
) -> Result<DeletedRecoveryContentRangeDto, CommandError> {
    request.validate().map_err(CommandError::invalid_input)?;
    let app_state = state.inner().clone();
    let bitlocker_runtime = app_state.bitlocker_runtime.clone();
    let data_source_id = DataSourceId(request.data_source_id);

    run_active_case_command(app_state, move |case_conn, active| {
        deleted_recovery::DeletedRecoveryContext::new(
            case_conn,
            &active.case_root,
            &active.meta.id,
            &data_source_id,
        )
        .with_bitlocker_runtime(bitlocker_runtime)
        .read_range(&request.recovery_id, request.offset, request.length)
        .map_err(CommandError::from_typed_service_error)
    })
    .await
}

#[tauri::command]
pub async fn export_deleted_recovery(
    state: State<'_, AppState>,
    request: ExportDeletedRecoveryRequest,
) -> Result<DeletedRecoveryExportDto, CommandError> {
    request.validate().map_err(CommandError::invalid_input)?;
    let app_state = state.inner().clone();
    let bitlocker_runtime = app_state.bitlocker_runtime.clone();
    let audit_state = app_state.clone();
    let audit_recovery_id = request.recovery_id.clone();
    let audit_destination = request.destination_path.clone();
    let overwrite = request.overwrite;
    let data_source_id = DataSourceId(request.data_source_id);
    let destination = PathBuf::from(request.destination_path);

    let outcome = run_active_case_command(app_state, move |case_conn, active| {
        deleted_recovery::DeletedRecoveryContext::new(
            case_conn,
            &active.case_root,
            &active.meta.id,
            &data_source_id,
        )
        .with_bitlocker_runtime(bitlocker_runtime)
        .export(&request.recovery_id, &destination, request.overwrite)
        .map_err(CommandError::from_typed_service_error)
    })
    .await;
    audit_recovery_export_outcome(
        &audit_state,
        &audit_recovery_id,
        &audit_destination,
        overwrite,
        &outcome,
    );
    outcome
}

fn audit_recovery_export_outcome(
    state: &AppState,
    recovery_id: &str,
    destination: &str,
    overwrite: bool,
    outcome: &Result<DeletedRecoveryExportDto, CommandError>,
) {
    let destination_file_name = Path::new(destination)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("unknown");
    let details = match outcome {
        Ok(export) => serde_json::json!({
            "operation": "deletedRecoveryExport",
            "status": "ok",
            "overwrite": overwrite,
            "destinationFileName": destination_file_name,
            "bytesWritten": export.bytes_written,
            "sha256": export.sha256,
        }),
        Err(error) => serde_json::json!({
            "operation": "deletedRecoveryExport",
            "status": "failed",
            "overwrite": overwrite,
            "destinationFileName": destination_file_name,
            "errorCode": error.code,
            "errorCategory": error.category,
        }),
    };
    write_file_extract_audit(state, recovery_id, details);
}
