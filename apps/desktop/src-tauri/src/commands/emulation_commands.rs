use std::path::PathBuf;

use tauri::State;
use transport::{
    commands::PrepareEmulationRequestDto,
    dto::{EmulationControlModeDto, EmulationSessionStatusDto, EmulationStateDto},
    CommandError,
};

use crate::commands::command_support::{
    get_case_connection, require_active_case, write_emulation_audit_log, EmulationAuditEvent,
};
use crate::emulation_registry::{EmulationSessionStatus, EmulationState};
use crate::state::AppState;

#[tauri::command]
pub async fn prepare_emulation(
    state: State<'_, AppState>,
    request: PrepareEmulationRequestDto,
) -> Result<EmulationSessionStatusDto, CommandError> {
    request.validate().map_err(CommandError::invalid_input)?;
    let app_state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let active = require_active_case(&app_state)?;
        let connection = get_case_connection(&app_state)?;
        let recovery_iso = request.recovery_iso_path.map(PathBuf::from);
        let options = evidence_emulation::VmOptions {
            network: request.options.network,
            clipboard: request.options.clipboard,
            time_sync: request.options.time_sync,
        };
        let status = app_state
            .emulation_registry
            .prepare_session(
                &connection,
                &active.case_root,
                &active.meta.id,
                &domain::DataSourceId(request.data_source_id.clone()),
                recovery_iso.as_deref(),
                options,
            )
            .map_err(CommandError::from_typed_service_error)?;
        audit_emulation(&app_state, EmulationAuditEvent::Prepare, &status);
        Ok(to_dto(status))
    })
    .await
    .map_err(CommandError::from_join_error)?
}

#[tauri::command]
pub async fn launch_emulation(
    state: State<'_, AppState>,
    session_id: String,
) -> Result<EmulationSessionStatusDto, CommandError> {
    validate_session_id(&session_id)?;
    let app_state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        require_active_case(&app_state)?;
        let status = app_state
            .emulation_registry
            .launch(&session_id)
            .map_err(CommandError::from_typed_service_error)?;
        audit_emulation(&app_state, EmulationAuditEvent::Launch, &status);
        Ok(to_dto(status))
    })
    .await
    .map_err(CommandError::from_join_error)?
}

#[tauri::command]
pub async fn get_emulation_status(
    state: State<'_, AppState>,
    session_id: String,
) -> Result<EmulationSessionStatusDto, CommandError> {
    validate_session_id(&session_id)?;
    let app_state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        require_active_case(&app_state)?;
        app_state
            .emulation_registry
            .status(&session_id)
            .map(to_dto)
            .map_err(CommandError::from_typed_service_error)
    })
    .await
    .map_err(CommandError::from_join_error)?
}

#[tauri::command]
pub async fn list_emulation_sessions(
    state: State<'_, AppState>,
) -> Result<Vec<EmulationSessionStatusDto>, CommandError> {
    let app_state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        require_active_case(&app_state)?;
        app_state
            .emulation_registry
            .list()
            .map(|statuses| statuses.into_iter().map(to_dto).collect())
            .map_err(CommandError::from_typed_service_error)
    })
    .await
    .map_err(CommandError::from_join_error)?
}

#[tauri::command]
pub async fn get_emulation_preflight(
    state: State<'_, AppState>,
    data_source_id: String,
) -> Result<transport::dto::EmulationPreflightDto, CommandError> {
    if data_source_id.trim().is_empty() {
        return Err(CommandError::invalid_input("data source id is required"));
    }
    let app_state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let active = require_active_case(&app_state)?;
        let connection = get_case_connection(&app_state)?;
        app_services::mount_service::emulation_preflight(
            &connection,
            &active.case_root,
            &active.meta.id,
            &domain::DataSourceId(data_source_id),
        )
        .map_err(CommandError::from_typed_service_error)
    })
    .await
    .map_err(CommandError::from_join_error)?
}

#[tauri::command]
pub async fn release_emulation(
    state: State<'_, AppState>,
    session_id: String,
) -> Result<EmulationSessionStatusDto, CommandError> {
    validate_session_id(&session_id)?;
    let app_state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        require_active_case(&app_state)?;
        let status = app_state
            .emulation_registry
            .release(&session_id)
            .map_err(CommandError::from_typed_service_error)?;
        audit_emulation(&app_state, EmulationAuditEvent::Release, &status);
        Ok(to_dto(status))
    })
    .await
    .map_err(CommandError::from_join_error)?
}

fn validate_session_id(session_id: &str) -> Result<(), CommandError> {
    if session_id.trim().is_empty() {
        Err(CommandError::invalid_input("session id is required"))
    } else {
        Ok(())
    }
}

fn to_dto(status: EmulationSessionStatus) -> EmulationSessionStatusDto {
    EmulationSessionStatusDto {
        session_id: status.session_id,
        data_source_id: status.data_source_id,
        state: state_to_dto(status.state),
        logical_length: status.logical_length,
        control_mode: EmulationControlModeDto::InteractiveOnly,
        error: status.error,
    }
}

fn state_to_dto(state: EmulationState) -> EmulationStateDto {
    match state {
        EmulationState::DescriptorReady => EmulationStateDto::DescriptorReady,
        EmulationState::Running => EmulationStateDto::Running,
        EmulationState::Quiescing => EmulationStateDto::Quiescing,
        EmulationState::Released => EmulationStateDto::Released,
        EmulationState::FailedCleanupPending => EmulationStateDto::FailedCleanupPending,
    }
}

fn audit_emulation(
    app_state: &AppState,
    event: EmulationAuditEvent,
    status: &EmulationSessionStatus,
) {
    write_emulation_audit_log(
        app_state,
        event,
        &status.session_id,
        &status.data_source_id,
        &format!("{:?}", status.state),
    );
}

#[cfg(test)]
#[path = "../../tests/unit/commands/emulation_commands.rs"]
mod tests;
