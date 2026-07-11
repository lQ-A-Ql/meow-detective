use std::collections::HashMap;

use persistence_sqlite::repositories::audit_repo::AuditAction;
use tauri::State;
use transport::dto::mcp::{McpPromptArgumentDto, McpPromptDto};
use transport::CommandError;

use super::lifecycle::get_connected_mcp_client;
use crate::commands::command_support::write_audit_log;
use crate::state::AppState;

#[tauri::command]
pub async fn list_mcp_prompts(
    state: State<'_, AppState>,
    server_id: String,
) -> Result<Vec<McpPromptDto>, CommandError> {
    let audit_server_id = server_id.clone();
    let client = get_connected_mcp_client(state.inner(), &server_id).await?;
    let prompts = {
        let client = client.lock().await;
        client
            .list_prompts()
            .await
            .map_err(CommandError::from_typed_service_error)?
    };

    write_audit_log(
        state.inner(),
        AuditAction::McpPromptList,
        Some(&audit_server_id),
        serde_json::json!({
            "serverId": audit_server_id,
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
                .map(|argument| McpPromptArgumentDto {
                    name: argument.name,
                    description: argument.description,
                    required: argument.required,
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
            .map_err(CommandError::from_typed_service_error)?
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
