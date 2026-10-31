use super::invocation::Invocation;
use crate::{
    commands::command_support::{snapshot_active_case, ActiveCaseSnapshot},
    state::AppState,
};
use persistence_sqlite::repositories::audit_repo::{AuditAction, AuditRepo};
use transport::{dto::mcp_host::McpHostToolCallResultDto, CommandError};

pub(super) fn record_protocol(
    state: &AppState,
    principal: &app_services::mcp_host_service::access::McpPrincipal,
    operation: &str,
    success: bool,
) {
    let Ok(Some(active)) = snapshot_active_case(state) else {
        return;
    };
    if active.case_id != principal.client.case_id
        || active.session_id != principal.client.case_session_id
    {
        return;
    }
    let Ok(connection) = app_services::connection::open_existing_case_db(&active.db_path()) else {
        return;
    };
    let details = serde_json::json!({
        "serverId":"embedded",
        "operation":operation,
        "success":success,
        "entryPoint":"http"
    });
    let _ = AuditRepo::new(&connection).log(
        Some(&active.case_id),
        &principal.client.id,
        &AuditAction::McpToolCall,
        Some(operation),
        &details.to_string(),
    );
}

pub(super) fn record(
    state: &AppState,
    invocation: Option<&Invocation>,
    actor: &str,
    tool: &str,
    response: &McpHostToolCallResultDto,
) -> Result<(), CommandError> {
    let active = match snapshot_active_case(state) {
        Ok(active) => active,
        Err(error) => {
            record_detached(invocation, actor, tool, response);
            return if error.code == "CONFLICT" {
                Ok(())
            } else {
                Err(error)
            };
        }
    };
    if let Some(active) = active.filter(|active| {
        invocation.is_some_and(|invocation| {
            invocation.case_id == active.case_id && invocation.session_id == active.session_id
        })
    }) {
        return record_for_active(
            &active,
            invocation.expect("matched invocation"),
            tool,
            response,
        );
    }
    record_detached(invocation, actor, tool, response);
    Ok(())
}

pub(super) fn record_detached(
    invocation: Option<&Invocation>,
    actor: &str,
    tool: &str,
    response: &McpHostToolCallResultDto,
) {
    tracing::info!(
        actor,
        tool,
        success = response.success,
        request_id = invocation.map(|value| value.request_id.as_str()),
        case_id = invocation.map(|value| value.case_id.as_str()),
        error_code = response
            .error_details
            .as_ref()
            .map(|error| error.code.as_str()),
        "MCP tool audit"
    );
}

pub(super) fn record_for_active(
    active: &ActiveCaseSnapshot,
    invocation: &Invocation,
    tool: &str,
    response: &McpHostToolCallResultDto,
) -> Result<(), CommandError> {
    match persist_event(active, invocation, tool, response) {
        Ok(()) => {
            record_detached(Some(invocation), &invocation.actor, tool, response);
            Ok(())
        }
        Err(error) => {
            let response = super::result::failure(error.clone());
            record_detached(Some(invocation), &invocation.actor, tool, &response);
            Err(error)
        }
    }
}

fn persist_event(
    active: &ActiveCaseSnapshot,
    invocation: &Invocation,
    tool: &str,
    response: &McpHostToolCallResultDto,
) -> Result<(), CommandError> {
    let connection = app_services::connection::open_existing_case_db(&active.db_path())
        .map_err(CommandError::from_typed_service_error)?;
    let error = response.error_details.as_ref();
    let details = serde_json::json!({
        "serverId":"embedded", "toolName":tool, "success":response.success,
        "requestId":invocation.request_id,
        "entryPoint":if invocation.principal.is_some() { "http" } else { "desktop" },
        "errorCode":error.map(|error| &error.code), "category":error.and_then(|error| error.category.as_deref()),
    });
    AuditRepo::new(&connection)
        .log(
            Some(&active.case_id),
            &invocation.actor,
            &AuditAction::McpToolCall,
            Some(tool),
            &details.to_string(),
        )
        .map_err(CommandError::from_typed_service_error)?;
    Ok(())
}
