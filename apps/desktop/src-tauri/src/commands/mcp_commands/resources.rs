use persistence_sqlite::repositories::audit_repo::AuditAction;
use tauri::State;
use transport::dto::mcp::McpResourceDto;
use transport::CommandError;

use super::lifecycle::get_connected_mcp_client;
use crate::commands::command_support::write_audit_log;
use crate::state::AppState;

#[tauri::command]
pub async fn list_mcp_resources(
    state: State<'_, AppState>,
    server_id: String,
) -> Result<Vec<McpResourceDto>, CommandError> {
    let audit_server_id = server_id.clone();
    let client = get_connected_mcp_client(state.inner(), &server_id).await?;
    let resources = {
        let client = client.lock().await;
        client
            .list_resources()
            .await
            .map_err(CommandError::from_typed_service_error)?
    };

    write_audit_log(
        state.inner(),
        AuditAction::McpResourceList,
        Some(&audit_server_id),
        serde_json::json!({
            "serverId": audit_server_id,
            "count": resources.len(),
        }),
    );

    Ok(resources
        .into_iter()
        .map(|resource| McpResourceDto {
            uri: resource.uri,
            name: resource.name,
            description: resource.description,
            mime_type: resource.mime_type,
        })
        .collect())
}
