use std::path::PathBuf;

use tauri::State;
use transport::{
    commands::PrepareEmulationRequestDto, dto::EmulationSessionStatusDto, CommandError,
};

use crate::commands::command_support::{
    get_case_connection, require_active_case, write_emulation_audit_log, EmulationAuditEvent,
};
use crate::emulation_registry::EmulationSessionStatus;
use crate::state::AppState;

mod bypass;
mod efi_fallback;
mod fs_repair;
mod preflight;
mod status_dto;

use status_dto::to_dto;

pub use bypass::{
    apply_emulation_bypass, apply_emulation_linux_bypass, cleanup_emulation_osdata,
    get_emulation_bypass_accounts, get_emulation_linux_accounts,
};
pub use efi_fallback::install_emulation_efi_fallback;
pub use fs_repair::repair_emulation_fs_journals;
pub use preflight::get_emulation_preflight;

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
        let network_mode = match request.options.network_mode {
            transport::dto::EmulationNetworkModeDto::Off => evidence_emulation::VmNetworkMode::Off,
            transport::dto::EmulationNetworkModeDto::HostOnly => {
                evidence_emulation::VmNetworkMode::HostOnly
            }
            transport::dto::EmulationNetworkModeDto::Nat => evidence_emulation::VmNetworkMode::Nat,
            transport::dto::EmulationNetworkModeDto::Bridged => {
                evidence_emulation::VmNetworkMode::Bridged
            }
        };
        let options = evidence_emulation::VmOptions {
            network_mode,
            clipboard: request.options.clipboard,
            time_sync: request.options.time_sync,
            processor_count: request.options.processor_count,
            memory_mib: request.options.memory_mib,
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
#[path = "../../../tests/unit/commands/emulation_commands.rs"]
mod tests;
