use persistence_sqlite::repositories::audit_repo::AuditAction;
use tauri::State;
use transport::dto::mcp::{McpToolCallRequestDto, McpToolCallResultDto, McpToolDto};
use transport::CommandError;

use super::lifecycle::get_connected_mcp_client;
use crate::commands::command_support::write_audit_log;
use crate::state::AppState;

#[tauri::command]
pub async fn list_mcp_tools(
    state: State<'_, AppState>,
    server_id: String,
) -> Result<Vec<McpToolDto>, CommandError> {
    let audit_server_id = server_id.clone();
    let client = get_connected_mcp_client(state.inner(), &server_id).await?;
    let tools = {
        let client = client.lock().await;
        client
            .list_tools()
            .await
            .map_err(CommandError::from_typed_service_error)?
    };

    write_audit_log(
        state.inner(),
        AuditAction::McpToolList,
        Some(&audit_server_id),
        serde_json::json!({
            "serverId": audit_server_id,
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
            .map_err(CommandError::from_typed_service_error)
        {
            Ok(data) => McpToolCallResultDto {
                success: true,
                data: Some(data),
                error: None,
            },
            Err(error) => McpToolCallResultDto {
                success: false,
                data: None,
                error: Some(error.to_string()),
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
