use persistence_sqlite::repositories::audit_repo::AuditAction;
use tauri::State;
use transport::dto::mcp::{
    McpCapabilitiesDto, McpServerStatusDto, McpTestConnectionRequestDto, McpTestConnectionResultDto,
};
use transport::CommandError;

use super::mapping::{
    permissions_from_dto, status_to_dto, summarize_transport, test_transport_summary_from_request,
};
use crate::commands::command_support::write_audit_log;
use crate::state::{app_state::SharedMcpClient, AppState};

pub(super) async fn get_connected_mcp_client(
    state: &AppState,
    server_id: &str,
) -> Result<SharedMcpClient, CommandError> {
    state
        .get_mcp_client(server_id)
        .await
        .map_err(CommandError::from_service_error)
}

#[tauri::command]
pub async fn connect_mcp_server(
    state: State<'_, AppState>,
    server_id: String,
) -> Result<McpServerStatusDto, CommandError> {
    state
        .connect_mcp_server(&server_id)
        .await
        .map_err(CommandError::from_service_error)?;

    let config = {
        let guard = state
            .mcp_config
            .lock()
            .map_err(|error| CommandError::from_lock_error("MCP config", error))?;
        guard
            .servers
            .iter()
            .find(|server| server.id == server_id)
            .cloned()
            .ok_or_else(|| CommandError::not_found("Server"))?
    };
    let status = state
        .get_mcp_server_status(&server_id)
        .ok_or_else(|| CommandError::not_found("Server"))?;

    write_audit_log(
        state.inner(),
        AuditAction::McpConnect,
        Some(&server_id),
        serde_json::json!({
            "status": "connected",
            "serverId": server_id,
            "summary": summarize_transport(&config),
            "capabilities": {
                "resources": status.capabilities.resources,
                "tools": status.capabilities.tools,
                "prompts": status.capabilities.prompts,
            }
        }),
    );
    Ok(status_to_dto(status))
}

#[tauri::command]
pub async fn disconnect_mcp_server(
    state: State<'_, AppState>,
    server_id: String,
) -> Result<(), CommandError> {
    let audit_server_id = server_id.clone();
    state
        .disconnect_mcp_server(&server_id)
        .await
        .map_err(CommandError::from_service_error)?;

    write_audit_log(
        state.inner(),
        AuditAction::McpDisconnect,
        Some(&audit_server_id),
        serde_json::json!({
            "status": "disconnected",
            "serverId": audit_server_id,
        }),
    );
    Ok(())
}

#[tauri::command]
pub async fn test_mcp_connection(
    state: State<'_, AppState>,
    request: McpTestConnectionRequestDto,
) -> Result<McpTestConnectionResultDto, CommandError> {
    if !matches!(request.transport_type.as_str(), "sse" | "stdio") {
        return Err(CommandError::invalid_input("Invalid transport type"));
    }
    let permissions = permissions_from_dto(&request.permissions);
    let summary = test_transport_summary_from_request(&request)?;
    let capabilities = mcp_client::probe::probe_mcp_connection(
        &request.transport_type,
        request.url.clone(),
        request.command.clone(),
        request.args.clone(),
        permissions,
    )
    .await;
    let (success, error, capabilities) = match capabilities {
        Ok(value) => (
            true,
            None,
            Some(McpCapabilitiesDto {
                resources: value.resources,
                tools: value.tools,
                prompts: value.prompts,
            }),
        ),
        Err(error) => (false, Some(error.to_string()), None),
    };
    write_audit_log(
        state.inner(),
        AuditAction::McpTest,
        Some("test"),
        serde_json::json!({
            "success": success,
            "error": error,
            "summary": summary,
            "capabilities": capabilities.as_ref().map(|value| serde_json::json!({
                "resources": value.resources,
                "tools": value.tools,
                "prompts": value.prompts,
            })),
        }),
    );
    Ok(McpTestConnectionResultDto {
        success,
        error,
        capabilities,
    })
}
