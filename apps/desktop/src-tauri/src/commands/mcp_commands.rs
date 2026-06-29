//! MCP (Model Context Protocol) commands.
//!
//! Tauri commands for MCP configuration, connectivity, capability discovery,
//! tool calls, prompt access, and audit logging.

use std::collections::HashMap;

use mcp_client::{
    validate_mcp_config, validate_mcp_server_config, McpConfig, McpNetworkPolicy,
    McpPermissionProfile, McpPromptAccess, McpResourceAccess, McpServerConfig, McpToolAccess,
    McpTransport,
};
use persistence_sqlite::repositories::audit_repo::AuditAction;
use tauri::State;
use transport::dto::mcp::*;
use transport::CommandError;

use super::command_support::write_audit_log;
use crate::state::{app_state::SharedMcpClient, AppState};

async fn get_connected_mcp_client(
    state: &AppState,
    server_id: &str,
) -> Result<SharedMcpClient, CommandError> {
    state
        .get_mcp_client(server_id)
        .await
        .map_err(CommandError::from_service_error)
}

fn transport_from_dto(server: &McpServerConfigDto) -> Result<McpTransport, CommandError> {
    match server.transport_type.as_str() {
        "sse" => Ok(McpTransport::Sse {
            url: server.url.clone().unwrap_or_default(),
        }),
        "stdio" => Ok(McpTransport::Stdio {
            command: server.command.clone().unwrap_or_default(),
            args: server.args.clone().unwrap_or_default(),
        }),
        _ => Err(CommandError::invalid_input("Invalid transport type")),
    }
}

fn resource_access_from_dto(value: &str) -> McpResourceAccess {
    match value {
        "disabled" => McpResourceAccess::Disabled,
        _ => McpResourceAccess::ReadOnly,
    }
}

fn tool_access_from_dto(value: &str) -> McpToolAccess {
    match value {
        "allowList" => McpToolAccess::AllowList,
        _ => McpToolAccess::Disabled,
    }
}

fn prompt_access_from_dto(value: &str) -> McpPromptAccess {
    match value {
        "disabled" => McpPromptAccess::Disabled,
        _ => McpPromptAccess::ReadOnly,
    }
}

fn network_policy_from_dto(value: &str) -> McpNetworkPolicy {
    match value {
        "privateLanAllowed" => McpNetworkPolicy::PrivateLanAllowed,
        "anyHost" => McpNetworkPolicy::AnyHost,
        _ => McpNetworkPolicy::LocalhostOnly,
    }
}

fn permissions_from_dto(dto: &McpPermissionProfileDto) -> McpPermissionProfile {
    McpPermissionProfile {
        resource_access: resource_access_from_dto(&dto.resource_access),
        tool_access: tool_access_from_dto(&dto.tool_access),
        prompt_access: prompt_access_from_dto(&dto.prompt_access),
        network_policy: network_policy_from_dto(&dto.network_policy),
        allowed_tools: dto.allowed_tools.clone(),
        allowed_commands: dto.allowed_commands.clone(),
    }
}

fn resource_access_to_dto(value: &McpResourceAccess) -> String {
    match value {
        McpResourceAccess::Disabled => "disabled".to_string(),
        McpResourceAccess::ReadOnly => "readOnly".to_string(),
    }
}

fn tool_access_to_dto(value: &McpToolAccess) -> String {
    match value {
        McpToolAccess::Disabled => "disabled".to_string(),
        McpToolAccess::AllowList => "allowList".to_string(),
    }
}

fn prompt_access_to_dto(value: &McpPromptAccess) -> String {
    match value {
        McpPromptAccess::Disabled => "disabled".to_string(),
        McpPromptAccess::ReadOnly => "readOnly".to_string(),
    }
}

fn network_policy_to_dto(value: &McpNetworkPolicy) -> String {
    match value {
        McpNetworkPolicy::LocalhostOnly => "localhostOnly".to_string(),
        McpNetworkPolicy::PrivateLanAllowed => "privateLanAllowed".to_string(),
        McpNetworkPolicy::AnyHost => "anyHost".to_string(),
    }
}

fn permissions_to_dto(value: &McpPermissionProfile) -> McpPermissionProfileDto {
    McpPermissionProfileDto {
        resource_access: resource_access_to_dto(&value.resource_access),
        tool_access: tool_access_to_dto(&value.tool_access),
        prompt_access: prompt_access_to_dto(&value.prompt_access),
        network_policy: network_policy_to_dto(&value.network_policy),
        allowed_tools: value.allowed_tools.clone(),
        allowed_commands: value.allowed_commands.clone(),
    }
}

fn server_config_from_dto(server: &McpServerConfigDto) -> Result<McpServerConfig, CommandError> {
    Ok(McpServerConfig {
        id: server.id.clone(),
        name: server.name.clone(),
        transport: transport_from_dto(server)?,
        enabled: server.enabled,
        auto_connect: server.auto_connect,
        permissions: permissions_from_dto(&server.permissions),
    })
}

fn config_from_dto(config: McpConfigDto) -> Result<McpConfig, CommandError> {
    let servers = config
        .servers
        .iter()
        .map(server_config_from_dto)
        .collect::<Result<Vec<_>, _>>()?;

    let mut config = McpConfig {
        servers,
        resources: config.resources,
        tools: config.tools,
    };
    validate_mcp_config(&mut config).map_err(CommandError::from_service_error)?;
    Ok(config)
}

fn status_to_dto(status: mcp_client::McpServerStatus) -> McpServerStatusDto {
    McpServerStatusDto {
        id: status.id,
        name: status.name,
        connected: status.connected,
        has_resources: status.capabilities.resources,
        has_tools: status.capabilities.tools,
        has_prompts: status.capabilities.prompts,
        last_error: status.last_error,
    }
}

fn summarize_transport(config: &McpServerConfig) -> serde_json::Value {
    match &config.transport {
        McpTransport::Sse { url } => {
            let host = reqwest::Url::parse(url)
                .ok()
                .and_then(|parsed| parsed.host_str().map(str::to_string))
                .unwrap_or_else(|| "unknown".to_string());
            serde_json::json!({
                "transport": "sse",
                "host": host,
                "networkPolicy": network_policy_to_dto(&config.permissions.network_policy),
            })
        }
        McpTransport::Stdio { command, .. } => serde_json::json!({
            "transport": "stdio",
            "command": command,
            "allowedCommands": config.permissions.allowed_commands,
        }),
    }
}

fn test_transport_summary_from_request(
    request: &McpTestConnectionRequestDto,
) -> Result<serde_json::Value, CommandError> {
    match request.transport_type.as_str() {
        "sse" => {
            let url_str = request.url.as_deref().unwrap_or("");
            let parsed = reqwest::Url::parse(url_str).ok();
            Ok(serde_json::json!({
                "transport": "sse",
                "scheme": parsed.as_ref().map(|v| v.scheme()).unwrap_or("unknown"),
                "host": parsed.as_ref().and_then(|v| v.host_str()).unwrap_or("unknown"),
                "networkPolicy": &request.permissions.network_policy,
            }))
        }
        "stdio" => Ok(serde_json::json!({
            "transport": "stdio",
            "command": request.command.as_deref().unwrap_or(""),
            "allowedCommands": request.permissions.allowed_commands,
        })),
        _ => Err(CommandError::invalid_input("Invalid transport type")),
    }
}

#[tauri::command]
pub async fn get_mcp_config(state: State<'_, AppState>) -> Result<McpConfigDto, CommandError> {
    let guard = state
        .mcp_config
        .lock()
        .map_err(|e| CommandError::from_lock_error("MCP config", e))?;

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
            .map_err(|e| CommandError::from_lock_error("MCP config", e))?;
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
    validate_mcp_server_config(&mut config).map_err(CommandError::from_service_error)?;

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
            .map_err(|e| CommandError::from_lock_error("MCP config", e))?;
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
    let server_id_for_audit = server_id.clone();
    state
        .disconnect_mcp_server(&server_id)
        .await
        .map_err(CommandError::from_service_error)?;

    write_audit_log(
        state.inner(),
        AuditAction::McpDisconnect,
        Some(&server_id_for_audit),
        serde_json::json!({
            "status": "disconnected",
            "serverId": server_id_for_audit,
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
    let caps = mcp_client::probe::probe_mcp_connection(
        &request.transport_type,
        request.url.clone(),
        request.command.clone(),
        request.args.clone(),
        permissions,
    )
    .await;
    let (success, error, capabilities) = match &caps {
        Ok(c) => (
            true,
            None,
            Some(McpCapabilitiesDto {
                resources: c.resources,
                tools: c.tools,
                prompts: c.prompts,
            }),
        ),
        Err(e) => (false, Some(e.to_string()), None),
    };
    write_audit_log(
        state.inner(),
        AuditAction::McpTest,
        Some("test"),
        serde_json::json!({
            "success": success,
            "error": error,
            "summary": summary,
            "capabilities": capabilities.as_ref().map(|c| serde_json::json!({
                "resources": c.resources,
                "tools": c.tools,
                "prompts": c.prompts,
            })),
        }),
    );
    Ok(McpTestConnectionResultDto {
        success,
        error,
        capabilities,
    })
}

#[tauri::command]
pub async fn list_mcp_resources(
    state: State<'_, AppState>,
    server_id: String,
) -> Result<Vec<McpResourceDto>, CommandError> {
    let server_id_for_audit = server_id.clone();
    let client = get_connected_mcp_client(state.inner(), &server_id).await?;
    let resources = {
        let client = client.lock().await;
        client
            .list_resources()
            .await
            .map_err(CommandError::from_service_error)?
    };

    write_audit_log(
        state.inner(),
        AuditAction::McpResourceList,
        Some(&server_id_for_audit),
        serde_json::json!({
            "serverId": server_id_for_audit,
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

#[tauri::command]
pub async fn list_mcp_tools(
    state: State<'_, AppState>,
    server_id: String,
) -> Result<Vec<McpToolDto>, CommandError> {
    let server_id_for_audit = server_id.clone();
    let client = get_connected_mcp_client(state.inner(), &server_id).await?;
    let tools = {
        let client = client.lock().await;
        client
            .list_tools()
            .await
            .map_err(CommandError::from_service_error)?
    };

    write_audit_log(
        state.inner(),
        AuditAction::McpToolList,
        Some(&server_id_for_audit),
        serde_json::json!({
            "serverId": server_id_for_audit,
            "count": tools.len(),
        }),
    );

    Ok(tools
        .into_iter()
        .map(|tool| McpToolDto {
            name: tool.name,
            description: tool.description,
            input_schema: tool.input_schema,
        })
        .collect())
}

#[tauri::command]
pub async fn call_mcp_tool(
    state: State<'_, AppState>,
    request: McpToolCallRequestDto,
) -> Result<McpToolCallResultDto, CommandError> {
    let audit_server_id = request.server_id.clone();
    let audit_tool_name = request.tool_name.clone();
    let client = get_connected_mcp_client(state.inner(), &request.server_id).await?;
    let result = {
        let client = client.lock().await;
        match client
            .call_tool(&request.tool_name, request.arguments)
            .await
            .map_err(CommandError::from_service_error)
        {
            Ok(data) => McpToolCallResultDto {
                success: true,
                data: Some(data),
                error: None,
            },
            Err(err) => McpToolCallResultDto {
                success: false,
                data: None,
                error: Some(err.to_string()),
            },
        }
    };

    write_audit_log(
        state.inner(),
        AuditAction::McpToolCall,
        Some(&audit_server_id),
        serde_json::json!({
            "serverId": audit_server_id,
            "toolName": audit_tool_name,
            "success": result.success,
            "error": result.error,
        }),
    );

    Ok(result)
}

#[tauri::command]
pub async fn list_mcp_prompts(
    state: State<'_, AppState>,
    server_id: String,
) -> Result<Vec<McpPromptDto>, CommandError> {
    let server_id_for_audit = server_id.clone();
    let client = get_connected_mcp_client(state.inner(), &server_id).await?;
    let prompts = {
        let client = client.lock().await;
        client
            .list_prompts()
            .await
            .map_err(CommandError::from_service_error)?
    };

    write_audit_log(
        state.inner(),
        AuditAction::McpPromptList,
        Some(&server_id_for_audit),
        serde_json::json!({
            "serverId": server_id_for_audit,
            "count": prompts.len(),
        }),
    );

    Ok(prompts
        .into_iter()
        .map(|prompt| McpPromptDto {
            name: prompt.name,
            description: prompt.description,
            arguments: prompt
                .arguments
                .into_iter()
                .map(|arg| McpPromptArgumentDto {
                    name: arg.name,
                    description: arg.description,
                    required: arg.required,
                })
                .collect(),
        })
        .collect())
}

#[tauri::command]
pub async fn get_mcp_prompt(
    state: State<'_, AppState>,
    server_id: String,
    prompt_name: String,
    arguments: Option<HashMap<String, String>>,
) -> Result<String, CommandError> {
    let audit_server_id = server_id.clone();
    let audit_prompt_name = prompt_name.clone();
    let client = get_connected_mcp_client(state.inner(), &server_id).await?;
    let result = {
        let client = client.lock().await;
        client
            .get_prompt(&prompt_name, arguments)
            .await
            .map_err(CommandError::from_service_error)?
    };

    write_audit_log(
        state.inner(),
        AuditAction::McpPromptGet,
        Some(&audit_server_id),
        serde_json::json!({
            "serverId": audit_server_id,
            "promptName": audit_prompt_name,
            "status": "ok",
        }),
    );

    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dummy_permissions() -> McpPermissionProfileDto {
        McpPermissionProfileDto {
            resource_access: "readOnly".to_string(),
            tool_access: "disabled".to_string(),
            prompt_access: "readOnly".to_string(),
            network_policy: "localhostOnly".to_string(),
            allowed_tools: vec![],
            allowed_commands: vec![],
        }
    }

    #[test]
    fn transport_from_dto_rejects_invalid_transport() {
        let server = McpServerConfigDto {
            id: "s1".to_string(),
            name: "Server".to_string(),
            transport_type: "invalid".to_string(),
            url: None,
            command: None,
            args: None,
            enabled: false,
            auto_connect: false,
            permissions: dummy_permissions(),
        };
        let err = transport_from_dto(&server).unwrap_err();
        assert!(err.to_string().contains("Invalid transport type"));
    }

    #[test]
    fn test_transport_summary_rejects_invalid_transport() {
        let request = McpTestConnectionRequestDto {
            transport_type: "invalid".to_string(),
            url: None,
            command: None,
            args: None,
            permissions: dummy_permissions(),
        };
        let err = test_transport_summary_from_request(&request).unwrap_err();
        assert!(err.to_string().contains("Invalid transport type"));
    }
}
