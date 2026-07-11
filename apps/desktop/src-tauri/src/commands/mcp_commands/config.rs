use mcp_client::{validate_mcp_server_config, McpTransport};
use persistence_sqlite::repositories::audit_repo::AuditAction;
use tauri::State;
use transport::dto::mcp::{McpConfigDto, McpServerConfigDto, McpServerStatusDto};
use transport::CommandError;

use super::mapping::{
    config_from_dto, permissions_to_dto, server_config_from_dto, summarize_transport,
};
use crate::commands::command_support::write_audit_log;
use crate::state::AppState;

#[tauri::command]
pub async fn get_mcp_config(state: State<'_, AppState>) -> Result<McpConfigDto, CommandError> {
    let guard = state
        .mcp_config
        .lock()
        .map_err(|error| CommandError::from_lock_error("MCP config", error))?;

    Ok(McpConfigDto {
        servers: guard
            .servers
            .iter()
            .map(|server| McpServerConfigDto {
                id: server.id.clone(),
                name: server.name.clone(),
                transport_type: match &server.transport {
                    McpTransport::Sse { .. } => "sse".to_string(),
                    McpTransport::Stdio { .. } => "stdio".to_string(),
                },
                url: match &server.transport {
                    McpTransport::Sse { url } => Some(url.clone()),
                    _ => None,
                },
                command: match &server.transport {
                    McpTransport::Stdio { command, .. } => Some(command.clone()),
                    _ => None,
                },
                args: match &server.transport {
                    McpTransport::Stdio { args, .. } => Some(args.clone()),
                    _ => None,
                },
                enabled: server.enabled,
                auto_connect: server.auto_connect,
                permissions: permissions_to_dto(&server.permissions),
            })
            .collect(),
        resources: guard.resources.clone(),
        tools: guard.tools.clone(),
    })
}

#[tauri::command]
pub async fn save_mcp_config(
    state: State<'_, AppState>,
    config: McpConfigDto,
) -> Result<(), CommandError> {
    let mcp_config = config_from_dto(config)?;
    {
        let mut guard = state
            .mcp_config
            .lock()
            .map_err(|error| CommandError::from_lock_error("MCP config", error))?;
        *guard = mcp_config;
    }

    state
        .save_mcp_config()
        .map_err(CommandError::from_service_error)?;
    state
        .sync_mcp_clients_with_config()
        .await
        .map_err(CommandError::from_service_error)
}

#[tauri::command]
pub async fn add_mcp_server(
    state: State<'_, AppState>,
    server: McpServerConfigDto,
) -> Result<McpServerStatusDto, CommandError> {
    let mut config = server_config_from_dto(&server)?;
    validate_mcp_server_config(&mut config).map_err(CommandError::from_typed_service_error)?;
    state
        .add_mcp_server(config.clone())
        .map_err(CommandError::from_service_error)?;

    write_audit_log(
        state.inner(),
        AuditAction::McpConnect,
        Some(&server.id),
        serde_json::json!({
            "status": "configured",
            "serverId": server.id,
            "name": server.name,
            "summary": summarize_transport(&config),
        }),
    );

    Ok(McpServerStatusDto {
        id: server.id,
        name: server.name,
        connected: false,
        has_resources: false,
        has_tools: false,
        has_prompts: false,
        last_error: None,
    })
}

#[tauri::command]
pub async fn remove_mcp_server(
    state: State<'_, AppState>,
    server_id: String,
) -> Result<(), CommandError> {
    state
        .disconnect_mcp_server(&server_id)
        .await
        .map_err(CommandError::from_service_error)?;
    state
        .remove_mcp_server(&server_id)
        .map_err(CommandError::from_service_error)?;

    write_audit_log(
        state.inner(),
        AuditAction::McpDisconnect,
        Some(&server_id),
        serde_json::json!({
            "status": "removed",
            "serverId": server_id,
        }),
    );
    Ok(())
}
