//! MCP (Model Context Protocol) commands
//!
//! Tauri commands for managing MCP server connections.

use mcp_client::{McpConfig, McpServerConfig, McpTransport};
use tauri::State;
use transport::dto::mcp::*;
use transport::CommandError;

use crate::state::AppState;

/// Get current MCP configuration.
#[tauri::command]
pub async fn get_mcp_config(state: State<'_, AppState>) -> Result<McpConfigDto, CommandError> {
    let guard = state
        .mcp_config
        .lock()
        .map_err(|e| CommandError::from_service_error(e.to_string()))?;

    Ok(McpConfigDto {
        servers: guard
            .servers
            .iter()
            .map(|s| McpServerConfigDto {
                id: s.id.clone(),
                name: s.name.clone(),
                transport_type: match &s.transport {
                    McpTransport::Sse { .. } => "sse".to_string(),
                    McpTransport::Stdio { .. } => "stdio".to_string(),
                },
                url: match &s.transport {
                    McpTransport::Sse { url } => Some(url.clone()),
                    _ => None,
                },
                command: match &s.transport {
                    McpTransport::Stdio { command, .. } => Some(command.clone()),
                    _ => None,
                },
                args: match &s.transport {
                    McpTransport::Stdio { args, .. } => Some(args.clone()),
                    _ => None,
                },
                enabled: s.enabled,
                auto_connect: s.auto_connect,
            })
            .collect(),
        resources: guard.resources.clone(),
        tools: guard.tools.clone(),
    })
}

/// Save MCP configuration.
#[tauri::command]
pub async fn save_mcp_config(
    state: State<'_, AppState>,
    config: McpConfigDto,
) -> Result<(), CommandError> {
    let mcp_config = McpConfig {
        servers: config
            .servers
            .iter()
            .map(|s| {
                let transport = match s.transport_type.as_str() {
                    "sse" => McpTransport::Sse {
                        url: s.url.clone().unwrap_or_default(),
                    },
                    "stdio" => McpTransport::Stdio {
                        command: s.command.clone().unwrap_or_default(),
                        args: s.args.clone().unwrap_or_default(),
                    },
                    _ => McpTransport::Sse { url: String::new() },
                };
                McpServerConfig {
                    id: s.id.clone(),
                    name: s.name.clone(),
                    transport,
                    enabled: s.enabled,
                    auto_connect: s.auto_connect,
                }
            })
            .collect(),
        resources: config.resources,
        tools: config.tools,
    };

    let mut guard = state
        .mcp_config
        .lock()
        .map_err(|e| CommandError::from_service_error(e.to_string()))?;
    *guard = mcp_config;
    drop(guard);

    state
        .save_mcp_config()
        .map_err(CommandError::from_service_error)
}

/// Add an MCP server.
#[tauri::command]
pub async fn add_mcp_server(
    state: State<'_, AppState>,
    server: McpServerConfigDto,
) -> Result<McpServerStatusDto, CommandError> {
    let transport = match server.transport_type.as_str() {
        "sse" => McpTransport::Sse {
            url: server.url.clone().unwrap_or_default(),
        },
        "stdio" => McpTransport::Stdio {
            command: server.command.clone().unwrap_or_default(),
            args: server.args.clone().unwrap_or_default(),
        },
        _ => {
            return Err(CommandError::from_service_error(
                "Invalid transport type".to_string(),
            ))
        }
    };

    let config = McpServerConfig {
        id: server.id.clone(),
        name: server.name.clone(),
        transport,
        enabled: server.enabled,
        auto_connect: server.auto_connect,
    };

    state
        .add_mcp_server(config)
        .map_err(CommandError::from_service_error)?;

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

/// Remove an MCP server.
#[tauri::command]
pub async fn remove_mcp_server(
    state: State<'_, AppState>,
    server_id: String,
) -> Result<(), CommandError> {
    state
        .remove_mcp_server(&server_id)
        .map_err(CommandError::from_service_error)
}

/// Connect to an MCP server.
#[tauri::command]
pub async fn connect_mcp_server(
    state: State<'_, AppState>,
    server_id: String,
) -> Result<McpServerStatusDto, CommandError> {
    let app_state = state.inner().clone();
    let server_id_clone = server_id.clone();

    // Use spawn_blocking to avoid Send issues with MutexGuard
    let result = tauri::async_runtime::spawn_blocking(move || {
        // Get config first
        let config = {
            let guard = app_state
                .mcp_config
                .lock()
                .map_err(|e| CommandError::from_service_error(e.to_string()))?;
            guard
                .servers
                .iter()
                .find(|s| s.id == server_id_clone)
                .cloned()
                .ok_or_else(|| CommandError::not_found("Server"))?
        };

        // Create client and connect in a blocking context
        let rt = tokio::runtime::Runtime::new()
            .map_err(|e| CommandError::from_service_error(e.to_string()))?;

        let mut client = mcp_client::McpClient::new(config);
        rt.block_on(client.connect())
            .map_err(|e| CommandError::from_service_error(e.to_string()))?;

        // Store the connected client
        let mut clients = app_state
            .mcp_clients
            .lock()
            .map_err(|e| CommandError::from_service_error(e.to_string()))?;
        clients.insert(server_id_clone.clone(), client);

        Ok::<(), CommandError>(())
    })
    .await
    .map_err(CommandError::from_join_error)?;

    result?;

    // Get the status after connecting
    let status = state
        .get_mcp_server_status(&server_id)
        .ok_or_else(|| CommandError::not_found("Server"))?;

    Ok(McpServerStatusDto {
        id: status.id,
        name: status.name,
        connected: status.connected,
        has_resources: status.capabilities.resources,
        has_tools: status.capabilities.tools,
        has_prompts: status.capabilities.prompts,
        last_error: status.last_error,
    })
}

/// Disconnect from an MCP server.
#[tauri::command]
pub async fn disconnect_mcp_server(
    state: State<'_, AppState>,
    server_id: String,
) -> Result<(), CommandError> {
    let app_state = state.inner().clone();

    tauri::async_runtime::spawn_blocking(move || {
        let rt = tokio::runtime::Runtime::new()
            .map_err(|e| CommandError::from_service_error(e.to_string()))?;

        let mut clients = app_state
            .mcp_clients
            .lock()
            .map_err(|e| CommandError::from_service_error(e.to_string()))?;

        if let Some(client) = clients.get_mut(&server_id) {
            rt.block_on(client.disconnect())
                .map_err(|e| CommandError::from_service_error(e.to_string()))?;
        }

        Ok::<(), CommandError>(())
    })
    .await
    .map_err(CommandError::from_join_error)?
}

/// Test MCP connection.
#[tauri::command]
pub async fn test_mcp_connection(
    request: McpTestConnectionRequest,
) -> Result<McpTestConnectionResult, CommandError> {
    let transport = match request.transport_type.as_str() {
        "sse" => McpTransport::Sse {
            url: request.url.unwrap_or_default(),
        },
        "stdio" => McpTransport::Stdio {
            command: request.command.unwrap_or_default(),
            args: request.args.unwrap_or_default(),
        },
        _ => {
            return Ok(McpTestConnectionResult {
                success: false,
                error: Some("Invalid transport type".to_string()),
                capabilities: None,
            })
        }
    };

    let config = McpServerConfig {
        id: "test".to_string(),
        name: "Test".to_string(),
        transport,
        enabled: true,
        auto_connect: false,
    };

    let mut client = mcp_client::McpClient::new(config);
    match client.connect().await {
        Ok(capabilities) => {
            let _ = client.disconnect().await;
            Ok(McpTestConnectionResult {
                success: true,
                error: None,
                capabilities: Some(McpCapabilitiesDto {
                    resources: capabilities.resources,
                    tools: capabilities.tools,
                    prompts: capabilities.prompts,
                }),
            })
        }
        Err(e) => Ok(McpTestConnectionResult {
            success: false,
            error: Some(e.to_string()),
            capabilities: None,
        }),
    }
}

/// List MCP resources from a server.
#[tauri::command]
pub async fn list_mcp_resources(
    state: State<'_, AppState>,
    server_id: String,
) -> Result<Vec<McpResourceDto>, CommandError> {
    let app_state = state.inner().clone();

    tauri::async_runtime::spawn_blocking(move || {
        let rt = tokio::runtime::Runtime::new()
            .map_err(|e| CommandError::from_service_error(e.to_string()))?;

        let clients = app_state
            .mcp_clients
            .lock()
            .map_err(|e| CommandError::from_service_error(e.to_string()))?;

        let client = clients
            .get(&server_id)
            .ok_or_else(|| CommandError::not_found("Server"))?;

        let resources = rt
            .block_on(client.list_resources())
            .map_err(|e| CommandError::from_service_error(e.to_string()))?;

        Ok(resources
            .into_iter()
            .map(|r| McpResourceDto {
                uri: r.uri,
                name: r.name,
                description: r.description,
                mime_type: r.mime_type,
            })
            .collect())
    })
    .await
    .map_err(CommandError::from_join_error)?
}

/// List MCP tools from a server.
#[tauri::command]
pub async fn list_mcp_tools(
    state: State<'_, AppState>,
    server_id: String,
) -> Result<Vec<McpToolDto>, CommandError> {
    let app_state = state.inner().clone();

    tauri::async_runtime::spawn_blocking(move || {
        let rt = tokio::runtime::Runtime::new()
            .map_err(|e| CommandError::from_service_error(e.to_string()))?;

        let clients = app_state
            .mcp_clients
            .lock()
            .map_err(|e| CommandError::from_service_error(e.to_string()))?;

        let client = clients
            .get(&server_id)
            .ok_or_else(|| CommandError::not_found("Server"))?;

        let tools = rt
            .block_on(client.list_tools())
            .map_err(|e| CommandError::from_service_error(e.to_string()))?;

        Ok(tools
            .into_iter()
            .map(|t| McpToolDto {
                name: t.name,
                description: t.description,
                input_schema: t.input_schema,
            })
            .collect())
    })
    .await
    .map_err(CommandError::from_join_error)?
}

/// Call an MCP tool.
#[tauri::command]
pub async fn call_mcp_tool(
    state: State<'_, AppState>,
    request: McpToolCallRequest,
) -> Result<McpToolCallResult, CommandError> {
    let app_state = state.inner().clone();

    tauri::async_runtime::spawn_blocking(move || {
        let rt = tokio::runtime::Runtime::new()
            .map_err(|e| CommandError::from_service_error(e.to_string()))?;

        let clients = app_state
            .mcp_clients
            .lock()
            .map_err(|e| CommandError::from_service_error(e.to_string()))?;

        let client = clients
            .get(&request.server_id)
            .ok_or_else(|| CommandError::not_found("Server"))?;

        match rt.block_on(client.call_tool(&request.tool_name, request.arguments)) {
            Ok(result) => Ok(McpToolCallResult {
                success: true,
                data: Some(result),
                error: None,
            }),
            Err(e) => Ok(McpToolCallResult {
                success: false,
                data: None,
                error: Some(e.to_string()),
            }),
        }
    })
    .await
    .map_err(CommandError::from_join_error)?
}

/// List MCP prompts from a server.
#[tauri::command]
pub async fn list_mcp_prompts(
    state: State<'_, AppState>,
    server_id: String,
) -> Result<Vec<McpPromptDto>, CommandError> {
    let app_state = state.inner().clone();

    tauri::async_runtime::spawn_blocking(move || {
        let rt = tokio::runtime::Runtime::new()
            .map_err(|e| CommandError::from_service_error(e.to_string()))?;

        let clients = app_state
            .mcp_clients
            .lock()
            .map_err(|e| CommandError::from_service_error(e.to_string()))?;

        let client = clients
            .get(&server_id)
            .ok_or_else(|| CommandError::not_found("Server"))?;

        let prompts = rt
            .block_on(client.list_prompts())
            .map_err(|e| CommandError::from_service_error(e.to_string()))?;

        Ok(prompts
            .into_iter()
            .map(|p| McpPromptDto {
                name: p.name,
                description: p.description,
                arguments: p
                    .arguments
                    .into_iter()
                    .map(|a| McpPromptArgumentDto {
                        name: a.name,
                        description: a.description,
                        required: a.required,
                    })
                    .collect(),
            })
            .collect())
    })
    .await
    .map_err(CommandError::from_join_error)?
}

/// Get an MCP prompt.
#[tauri::command]
pub async fn get_mcp_prompt(
    state: State<'_, AppState>,
    server_id: String,
    prompt_name: String,
    arguments: Option<std::collections::HashMap<String, String>>,
) -> Result<String, CommandError> {
    let app_state = state.inner().clone();

    tauri::async_runtime::spawn_blocking(move || {
        let rt = tokio::runtime::Runtime::new()
            .map_err(|e| CommandError::from_service_error(e.to_string()))?;

        let clients = app_state
            .mcp_clients
            .lock()
            .map_err(|e| CommandError::from_service_error(e.to_string()))?;

        let client = clients
            .get(&server_id)
            .ok_or_else(|| CommandError::not_found("Server"))?;

        rt.block_on(client.get_prompt(&prompt_name, arguments))
            .map_err(|e| CommandError::from_service_error(e.to_string()))
    })
    .await
    .map_err(CommandError::from_join_error)?
}
