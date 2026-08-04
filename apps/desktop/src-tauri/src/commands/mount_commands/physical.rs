use persistence_sqlite::repositories::audit_repo::{AuditAction, AuditRepo};
use tauri::State;
use transport::{commands::MountPhysicalImageRequestDto, dto::MountStatusDto, CommandError};

use crate::state::AppState;

#[tauri::command]
pub async fn mount_physical_image(
    state: State<'_, AppState>,
    request: MountPhysicalImageRequestDto,
) -> Result<MountStatusDto, CommandError> {
    request.validate().map_err(CommandError::invalid_input)?;
    let app_state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let connection = app_state
            .get_connection()
            .map_err(CommandError::from_service_error)?;
        let active = crate::commands::command_support::require_active_case(&app_state)?;
        let data_source_id = domain::DataSourceId(request.data_source_id.clone());
        let registry = app_state.physical_mount_registry.clone();
        let status = registry
            .mount(&connection, &active.meta.id, &data_source_id)
            .map_err(CommandError::from_typed_service_error)?;
        let details = serde_json::json!({
            "status": "mounted",
            "mode": "physicalDisk",
            "mountId": status.target.mount_id,
            "physicalDevicePath": status.target.physical_device_path,
            "targetAddress": status.target.target_address,
            "readOnly": true,
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
                    "Failed to roll back an unaudited physical image mount"
                );
            }
            return Err(CommandError::from_typed_service_error(error));
        }
        Ok(status)
    })
    .await
    .map_err(CommandError::from_join_error)?
}
