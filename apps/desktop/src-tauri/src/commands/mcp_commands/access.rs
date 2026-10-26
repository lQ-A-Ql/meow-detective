use crate::state::AppState;
use tauri::State;
use transport::{
    dto::mcp_host::access::{
        McpHostClientDto, McpHostCreateClientRequestDto, McpHostCreatedClientDto,
    },
    CommandError,
};

#[tauri::command]
pub async fn list_mcp_host_clients(
    state: State<'_, AppState>,
) -> Result<Vec<McpHostClientDto>, CommandError> {
    state.mcp_host.list_clients()
}

#[tauri::command]
pub async fn create_mcp_host_client(
    state: State<'_, AppState>,
    request: McpHostCreateClientRequestDto,
) -> Result<McpHostCreatedClientDto, CommandError> {
    state.mcp_host.create_client(state.inner(), request)
}

#[tauri::command]
pub async fn revoke_mcp_host_client(
    state: State<'_, AppState>,
    client_id: String,
) -> Result<(), CommandError> {
    state.mcp_host.revoke_client(state.inner(), &client_id)
}
