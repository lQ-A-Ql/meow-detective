use tauri::State;
use transport::CommandError;

use crate::commands::command_support::{
    get_case_connection, require_active_case, write_emulation_edit_audit_log, EmulationAuditEvent,
};
use crate::state::AppState;

#[tauri::command]
pub async fn get_emulation_bypass_accounts(
    state: State<'_, AppState>,
    data_source_id: String,
    partition_index: u32,
) -> Result<Vec<transport::dto::EmulationBypassAccountDto>, CommandError> {
    if data_source_id.trim().is_empty() {
        return Err(CommandError::invalid_input("data source id is required"));
    }
    let app_state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let active = require_active_case(&app_state)?;
        let connection = get_case_connection(&app_state)?;
        app_services::emulation_bypass::list_bypass_accounts(
            &app_services::emulation_bypass::BypassCaseContext {
                case_conn: &connection,
                case_root: &active.case_root,
                case_id: &active.meta.id,
                data_source_id: &domain::DataSourceId(data_source_id),
            },
            partition_index,
        )
        .map_err(CommandError::from_typed_service_error)
    })
    .await
    .map_err(CommandError::from_join_error)?
}

#[tauri::command]
pub async fn apply_emulation_bypass(
    state: State<'_, AppState>,
    request: transport::dto::EmulationBypassApplyRequestDto,
) -> Result<transport::dto::EmulationBypassResultDto, CommandError> {
    if request.session_id.trim().is_empty() {
        return Err(CommandError::invalid_input("session id is required"));
    }
    let app_state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let active = require_active_case(&app_state)?;
        let connection = get_case_connection(&app_state)?;
        let result = app_state
            .emulation_registry
            .apply_bypass(
                &crate::emulation_registry::BypassCaseRef {
                    case_conn: &connection,
                    case_root: &active.case_root,
                    case_id: &active.meta.id,
                },
                &request.session_id,
                request.partition_index,
                request.rid,
                request.action,
            )
            .map_err(CommandError::from_typed_service_error)?;
        write_emulation_edit_audit_log(
            &app_state,
            EmulationAuditEvent::Bypass,
            &request.session_id,
            &result.data_source_id,
            serde_json::json!({
                "partitionIndex": result.partition_index,
                "rid": result.rid,
                "username": result.username,
                "action": format!("{:?}", request.action),
                "passwordCleared": result.password_cleared,
                "accountEnabled": result.account_enabled,
                "alreadyPasswordless": result.already_passwordless,
            }),
        );
        Ok(result)
    })
    .await
    .map_err(CommandError::from_join_error)?
}

#[tauri::command]
pub async fn cleanup_emulation_osdata(
    state: State<'_, AppState>,
    request: transport::dto::EmulationOsdataCleanupRequestDto,
) -> Result<transport::dto::EmulationOsdataCleanupDto, CommandError> {
    if request.session_id.trim().is_empty() {
        return Err(CommandError::invalid_input("session id is required"));
    }
    let app_state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let active = require_active_case(&app_state)?;
        let connection = get_case_connection(&app_state)?;
        let result = app_state
            .emulation_registry
            .cleanup_osdata(
                &crate::emulation_registry::BypassCaseRef {
                    case_conn: &connection,
                    case_root: &active.case_root,
                    case_id: &active.meta.id,
                },
                &request.session_id,
                request.partition_index,
            )
            .map_err(CommandError::from_typed_service_error)?;
        write_emulation_edit_audit_log(
            &app_state,
            EmulationAuditEvent::OsdataCleanup,
            &request.session_id,
            &result.data_source_id,
            serde_json::json!({
                "partitionIndex": result.partition_index,
                "state": format!("{:?}", result.state),
                "editsApplied": result.edits_applied,
            }),
        );
        Ok(result)
    })
    .await
    .map_err(CommandError::from_join_error)?
}

#[tauri::command]
pub async fn get_emulation_linux_accounts(
    state: State<'_, AppState>,
    data_source_id: String,
    partition_index: u32,
) -> Result<Vec<transport::dto::EmulationLinuxAccountDto>, CommandError> {
    if data_source_id.trim().is_empty() {
        return Err(CommandError::invalid_input("data source id is required"));
    }
    let app_state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let active = require_active_case(&app_state)?;
        let connection = get_case_connection(&app_state)?;
        app_services::emulation_linux_bypass::list_linux_accounts(
            &app_services::emulation_bypass::BypassCaseContext {
                case_conn: &connection,
                case_root: &active.case_root,
                case_id: &active.meta.id,
                data_source_id: &domain::DataSourceId(data_source_id),
            },
            partition_index,
        )
        .map_err(CommandError::from_typed_service_error)
    })
    .await
    .map_err(CommandError::from_join_error)?
}

#[tauri::command]
pub async fn apply_emulation_linux_bypass(
    state: State<'_, AppState>,
    request: transport::dto::EmulationLinuxBypassRequestDto,
) -> Result<transport::dto::EmulationLinuxBypassResultDto, CommandError> {
    if request.session_id.trim().is_empty() {
        return Err(CommandError::invalid_input("session id is required"));
    }
    if request.username.trim().is_empty() {
        return Err(CommandError::invalid_input("username is required"));
    }
    let app_state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let active = require_active_case(&app_state)?;
        let connection = get_case_connection(&app_state)?;
        let result = app_state
            .emulation_registry
            .apply_linux_bypass(
                &crate::emulation_registry::BypassCaseRef {
                    case_conn: &connection,
                    case_root: &active.case_root,
                    case_id: &active.meta.id,
                },
                &request.session_id,
                request.partition_index,
                &request.username,
            )
            .map_err(CommandError::from_typed_service_error)?;
        write_emulation_edit_audit_log(
            &app_state,
            EmulationAuditEvent::LinuxBypass,
            &request.session_id,
            &result.data_source_id,
            serde_json::json!({
                "partitionIndex": result.partition_index,
                "username": result.username,
                "passwordSet": result.password_set,
                "alreadyConfigured": result.already_configured,
            }),
        );
        Ok(result)
    })
    .await
    .map_err(CommandError::from_join_error)?
}
