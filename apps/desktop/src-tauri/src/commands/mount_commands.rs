use persistence_sqlite::repositories::audit_repo::{AuditAction, AuditRepo};
use tauri::State;
use transport::{commands::MountImageRequestDto, dto::MountStatusDto, CommandError};

use crate::{mount_registry::MountRegistryError, state::AppState};

#[tauri::command]
pub async fn mount_image(
    state: State<'_, AppState>,
    request: MountImageRequestDto,
) -> Result<MountStatusDto, CommandError> {
    request.validate().map_err(CommandError::invalid_input)?;
    let app_state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let connection = app_state
            .get_connection()
            .map_err(CommandError::from_service_error)?;
        let active = crate::commands::command_support::require_active_case(&app_state)?;
        let registry = app_state.mount_registry.clone();
        let data_source_id = domain::DataSourceId(request.data_source_id.clone());
        let status = registry
            .mount(
                &connection,
                &active.case_root,
                &active.meta.id,
                &data_source_id,
                request.partition_index as usize,
                request.mount_point.as_deref(),
            )
            .map_err(CommandError::from_typed_service_error)?;
        let details = serde_json::json!({
            "status": "mounted",
            "mountId": status.target.mount_id,
            "partitionIndex": status.target.partition_index,
            "filesystem": status.target.filesystem,
            "mountPoint": status.target.mount_point,
            "readOnly": status.target.read_only,
        });
        if let Err(error) = AuditRepo::new(&connection).log(
            Some(&active.meta.id.0),
            "system",
            &AuditAction::ImageMount,
            Some(&request.data_source_id),
            &details.to_string(),
        ) {
            if let Err(cleanup_error) = registry.unmount(&status.target.mount_id) {
                tracing::error!(
                    error = %cleanup_error,
                    mount_id = %status.target.mount_id,
                    "Failed to roll back an unaudited image mount"
                );
            }
            return Err(CommandError::from_typed_service_error(error));
        }
        Ok(status)
    })
    .await
    .map_err(CommandError::from_join_error)?
}

#[tauri::command]
pub async fn unmount_image(
    state: State<'_, AppState>,
    mount_id: String,
) -> Result<(), CommandError> {
    if mount_id.trim().is_empty() {
        return Err(CommandError::invalid_input("mount id is required"));
    }
    let app_state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let connection = app_state
            .get_connection()
            .map_err(CommandError::from_service_error)?;
        let active = crate::commands::command_support::require_active_case(&app_state)?;
        let registry = app_state.mount_registry.clone();
        let status = registry
            .status(&mount_id)
            .map_err(CommandError::from_typed_service_error)?;
        let details = serde_json::json!({
            "status": "requested",
            "mountId": status.target.mount_id,
            "partitionIndex": status.target.partition_index,
            "filesystem": status.target.filesystem,
            "mountPoint": status.target.mount_point,
            "readOnly": status.target.read_only,
        });
        AuditRepo::new(&connection)
            .log(
                Some(&active.meta.id.0),
                "system",
                &AuditAction::ImageUnmount,
                Some(&status.target.data_source_id),
                &details.to_string(),
            )
            .map_err(CommandError::from_typed_service_error)?;
        registry
            .unmount(&mount_id)
            .map_err(CommandError::from_typed_service_error)
    })
    .await
    .map_err(CommandError::from_join_error)?
}

#[tauri::command]
pub async fn get_mount_status(
    state: State<'_, AppState>,
    mount_id: String,
) -> Result<MountStatusDto, CommandError> {
    if mount_id.trim().is_empty() {
        return Err(CommandError::invalid_input("mount id is required"));
    }
    let registry = state.inner().mount_registry.clone();
    tauri::async_runtime::spawn_blocking(move || {
        registry
            .status(&mount_id)
            .map_err(CommandError::from_typed_service_error)
    })
    .await
    .map_err(CommandError::from_join_error)?
}

#[tauri::command]
pub async fn list_mounts(state: State<'_, AppState>) -> Result<Vec<MountStatusDto>, CommandError> {
    let registry = state.inner().mount_registry.clone();
    tauri::async_runtime::spawn_blocking(move || {
        registry
            .list()
            .map_err(|error: MountRegistryError| CommandError::from_typed_service_error(error))
    })
    .await
    .map_err(CommandError::from_join_error)?
}
