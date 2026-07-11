use mcp_client::{
    validate_mcp_config, McpConfig, McpNetworkPolicy, McpPermissionProfile, McpPromptAccess,
    McpResourceAccess, McpServerConfig, McpToolAccess, McpTransport,
};
use transport::dto::mcp::{
    McpConfigDto, McpPermissionProfileDto, McpServerConfigDto, McpServerStatusDto,
    McpTestConnectionRequestDto,
};
use transport::CommandError;

pub(super) fn transport_from_dto(
    server: &McpServerConfigDto,
) -> Result<McpTransport, CommandError> {
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

pub(super) fn permissions_from_dto(dto: &McpPermissionProfileDto) -> McpPermissionProfile {
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

pub(super) fn network_policy_to_dto(value: &McpNetworkPolicy) -> String {
    match value {
        McpNetworkPolicy::LocalhostOnly => "localhostOnly".to_string(),
        McpNetworkPolicy::PrivateLanAllowed => "privateLanAllowed".to_string(),
        McpNetworkPolicy::AnyHost => "anyHost".to_string(),
    }
}

pub(super) fn permissions_to_dto(value: &McpPermissionProfile) -> McpPermissionProfileDto {
    McpPermissionProfileDto {
        resource_access: resource_access_to_dto(&value.resource_access),
        tool_access: tool_access_to_dto(&value.tool_access),
        prompt_access: prompt_access_to_dto(&value.prompt_access),
        network_policy: network_policy_to_dto(&value.network_policy),
        allowed_tools: value.allowed_tools.clone(),
        allowed_commands: value.allowed_commands.clone(),
    }
}

pub(super) fn server_config_from_dto(
    server: &McpServerConfigDto,
) -> Result<McpServerConfig, CommandError> {
    Ok(McpServerConfig {
        id: server.id.clone(),
        name: server.name.clone(),
        transport: transport_from_dto(server)?,
        enabled: server.enabled,
        auto_connect: server.auto_connect,
        permissions: permissions_from_dto(&server.permissions),
    })
}

pub(super) fn config_from_dto(config: McpConfigDto) -> Result<McpConfig, CommandError> {
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
    validate_mcp_config(&mut config).map_err(CommandError::from_typed_service_error)?;
    Ok(config)
}

pub(super) fn status_to_dto(status: mcp_client::McpServerStatus) -> McpServerStatusDto {
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

pub(super) fn summarize_transport(config: &McpServerConfig) -> serde_json::Value {
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

pub(super) fn test_transport_summary_from_request(
    request: &McpTestConnectionRequestDto,
) -> Result<serde_json::Value, CommandError> {
    match request.transport_type.as_str() {
        "sse" => {
            let url = reqwest::Url::parse(request.url.as_deref().unwrap_or("")).ok();
            Ok(serde_json::json!({
                "transport": "sse",
                "scheme": url.as_ref().map(|value| value.scheme()).unwrap_or("unknown"),
                "host": url.as_ref().and_then(|value| value.host_str()).unwrap_or("unknown"),
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
