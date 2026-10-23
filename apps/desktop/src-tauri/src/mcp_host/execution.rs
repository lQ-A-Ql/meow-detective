use crate::state::AppState;
use app_services::mcp_host_service::{
    execute_analysis_tool, execute_file_tool, execute_tool, tool_catalog,
    McpHostAnalysisQueryContext, McpHostFileQueryContext, McpHostQueryContext,
};
use persistence_sqlite::repositories::audit_repo::{AuditAction, AuditRepo};
use serde_json::json;
use transport::{
    dto::mcp_host::{McpHostToolCallRequestDto, McpHostToolCallResultDto},
    CommandError,
};

pub async fn call_tool(
    state: AppState,
    request: McpHostToolCallRequestDto,
) -> Result<McpHostToolCallResultDto, CommandError> {
    if !tool_catalog().iter().any(|tool| tool.name == request.name) {
        return Ok(failure("未知的内置工具"));
    }
    let permit = match state.mcp_host.requests.clone().try_acquire_owned() {
        Ok(permit) => permit,
        Err(_) => return Ok(failure("工具正在忙，请稍后重试")),
    };
    tokio::task::spawn_blocking(move || {
        let _permit = permit;
        execute_for_active_case(&state, request)
    })
    .await
    .map_err(CommandError::from_join_error)?
}

fn execute_for_active_case(
    state: &AppState,
    request: McpHostToolCallRequestDto,
) -> Result<McpHostToolCallResultDto, CommandError> {
    let _access = state.case_access()?;
    // Holding the lifecycle lock until the read completes prevents a response
    // or audit entry from being attributed to a newly opened case.
    let guard = state
        .active_case
        .lock()
        .map_err(|error| CommandError::from_lock_error("Case", error))?;
    let Some(active) = guard.as_ref() else {
        return Ok(failure("请先在应用中打开案件"));
    };
    let conn = rusqlite::Connection::open_with_flags(
        active.db_path(),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .map_err(|_| CommandError::internal("无法读取当前案件"))?;
    conn.busy_timeout(std::time::Duration::from_secs(5))
        .map_err(|_| CommandError::internal("无法读取当前案件"))?;
    let enabled = state
        .mcp_host
        .tool_enabled(&request.name)
        .map_err(CommandError::from_typed_service_error)?;
    let response = if !enabled {
        failure("工具或 MCP 服务已禁用")
    } else {
        let context = McpHostQueryContext {
            connection: &conn,
            case_root: &active.case_root,
            case_meta: &active.meta,
        };
        let result = dispatch_tool(state, context, &request);
        match result {
            Ok(data) if data.to_string().len() <= 1024 * 1024 => McpHostToolCallResultDto {
                success: true,
                data: Some(data),
                error: None,
            },
            Ok(_) => failure("工具结果过大，请缩小查询范围"),
            Err(error) => failure(&error.to_string()),
        }
    };
    let audit_conn = app_services::connection::open_existing_case_db(&active.db_path())
        .map_err(CommandError::from_typed_service_error)?;
    AuditRepo::new(&audit_conn)
        .log(
            Some(&active.meta.id.0),
            "mcp-client",
            &AuditAction::McpToolCall,
            Some(&request.name),
            &json!({"serverId":"embedded", "toolName":request.name, "success":response.success})
                .to_string(),
        )
        .map_err(CommandError::from_typed_service_error)?;
    Ok(response)
}

fn dispatch_tool(
    state: &AppState,
    context: McpHostQueryContext<'_>,
    request: &McpHostToolCallRequestDto,
) -> Result<serde_json::Value, app_services::mcp_host_service::McpHostServiceError> {
    if matches!(
        request.name.as_str(),
        "forensics.list_files" | "forensics.get_file_metadata" | "forensics.read_file"
    ) {
        return execute_file_tool(
            McpHostFileQueryContext {
                query: context,
                preview_runtime: &state.preview_runtime,
                bitlocker_runtime: &state.bitlocker_runtime,
            },
            &request.name,
            &request.arguments,
        );
    }
    if is_analysis_tool(&request.name) {
        let runtime =
            app_services::analysis_service::AnalysisSourceReadRuntime::with_bitlocker_runtime(
                state.bitlocker_runtime.clone(),
            );
        return execute_analysis_tool(
            McpHostAnalysisQueryContext {
                query: context,
                runtime: &runtime,
            },
            &request.name,
            &request.arguments,
        );
    }
    execute_tool(context, &request.name, &request.arguments)
}

fn is_analysis_tool(name: &str) -> bool {
    matches!(
        name,
        "forensics.get_analysis_system_info"
            | "forensics.get_evidence_classification_summary"
            | "forensics.get_file_classification_board"
            | "forensics.get_registry_summary"
            | "forensics.get_registry_structured_summary"
            | "forensics.get_browser_history_summary"
            | "forensics.get_email_summary"
            | "forensics.get_event_log_events"
            | "forensics.get_linux_artifact_summary"
            | "forensics.get_android_device_info"
            | "forensics.get_android_package_summary"
            | "forensics.get_plugin_family_entries"
            | "forensics.get_analysis_report"
            | "forensics.get_case_overview"
            | "forensics.get_correlation_snapshot"
    )
}

fn failure(message: &str) -> McpHostToolCallResultDto {
    McpHostToolCallResultDto {
        success: false,
        data: None,
        error: Some(message.into()),
    }
}
