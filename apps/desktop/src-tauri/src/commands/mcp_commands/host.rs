use crate::state::AppState;
use tauri::State;
use transport::{
    dto::mcp_host::{
        McpHostSettingsDto, McpHostStatusDto, McpHostToolCallRequestDto, McpHostToolCallResultDto,
        McpHostToolDto,
    },
    CommandError,
};

#[tauri::command]
pub async fn get_mcp_host_status(
    state: State<'_, AppState>,
) -> Result<McpHostStatusDto, CommandError> {
    state
        .mcp_host
        .status()
        .map_err(CommandError::from_typed_service_error)
}

#[tauri::command]
pub async fn set_mcp_host_settings(
    state: State<'_, AppState>,
    settings: McpHostSettingsDto,
) -> Result<McpHostStatusDto, CommandError> {
    state
        .mcp_host
        .configure(state.inner().clone(), settings)
        .await
        .map_err(CommandError::from_typed_service_error)
}

#[tauri::command]
pub async fn list_mcp_host_tools() -> Result<Vec<McpHostToolDto>, CommandError> {
    Ok(app_services::mcp_host_service::tool_catalog())
}

#[tauri::command]
pub async fn call_mcp_host_tool(
    state: State<'_, AppState>,
    request: McpHostToolCallRequestDto,
) -> Result<McpHostToolCallResultDto, CommandError> {
    crate::mcp_host::call_tool(state.inner().clone(), request).await
}
