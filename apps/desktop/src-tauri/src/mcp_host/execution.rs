use super::{
    audit,
    invocation::Invocation,
    result::{coded, failure, tool_response},
};
use crate::{
    commands::command_support::{snapshot_active_case, ActiveCaseSnapshot},
    state::AppState,
};
use app_services::mcp_host_service::{
    access::McpPrincipal, execute_analysis_tool, execute_file_tool, execute_tool, tool_catalog,
    McpHostAnalysisQueryContext, McpHostFileQueryContext, McpHostQueryContext,
};
use std::sync::Arc;
use tokio_util::sync::CancellationToken;
use transport::{
    dto::mcp_host::{McpHostToolCallRequestDto, McpHostToolCallResultDto},
    CommandError,
};

pub async fn call_tool(
    state: AppState,
    request: McpHostToolCallRequestDto,
) -> Result<McpHostToolCallResultDto, CommandError> {
    call_tool_as(state, request, None, CancellationToken::new()).await
}

pub(super) async fn call_tool_as(
    state: AppState,
    request: McpHostToolCallRequestDto,
    principal: Option<McpPrincipal>,
    cancel: CancellationToken,
) -> Result<McpHostToolCallResultDto, CommandError> {
    let known = tool_catalog().iter().any(|tool| tool.name == request.name);
    let safe_name = if known {
        request.name.clone()
    } else {
        "unknown".into()
    };
    let actor = principal
        .as_ref()
        .map(|p| p.client.id.clone())
        .unwrap_or_else(|| "desktop".into());
    let invocation = match Invocation::new(
        &state,
        principal,
        cancel,
        request.name == "forensics.read_file",
    ) {
        Ok(invocation) => Arc::new(invocation),
        Err(error) => return reject(state, None, actor, safe_name, error).await,
    };
    if !known {
        return reject(
            state,
            Some(invocation),
            actor,
            safe_name,
            coded(
                CommandError::invalid_input("未知的内置工具"),
                "MCP_UNKNOWN_TOOL",
            ),
        )
        .await;
    }
    if let Err(error) = invocation.check(&request.name, &request.arguments) {
        return reject(state, Some(invocation), actor, safe_name, error).await;
    }
    let permit = match state
        .mcp_host
        .admission
        .acquire(
            state.mcp_host.requests.clone(),
            &invocation.actor,
            &invocation.cancel,
            invocation.principal.is_none(),
        )
        .await
    {
        Ok(permit) => permit,
        Err(error) => {
            let error = match error.code.as_str() {
                "CONFLICT" => coded(error, "MCP_QUEUE_FULL"),
                "TIMEOUT" => coded(error, "MCP_QUEUE_TIMEOUT"),
                _ => error,
            };
            return reject(state, Some(invocation), actor, safe_name, error).await;
        }
    };
    let (worker_state, worker_invocation) = (state.clone(), invocation.clone());
    match tokio::task::spawn_blocking(move || {
        let _permit = permit;
        execute_for_active_case(&worker_state, &request, &worker_invocation)
    })
    .await
    {
        Ok(result) => result,
        Err(error) => {
            reject(
                state,
                Some(invocation),
                actor,
                safe_name,
                CommandError::from_join_error(error),
            )
            .await
        }
    }
}

async fn reject(
    state: AppState,
    invocation: Option<Arc<Invocation>>,
    actor: String,
    tool: String,
    error: CommandError,
) -> Result<McpHostToolCallResultDto, CommandError> {
    let waiting = match state.mcp_host.audit_waiters.clone().try_acquire_owned() {
        Ok(waiting) => waiting,
        Err(_) => {
            let response = failure(coded(
                CommandError::conflict("MCP 审计请求容量已满，请稍后重试"),
                "MCP_AUDIT_BUSY",
            ));
            audit::record_detached(invocation.as_deref(), &actor, &tool, &response);
            return Ok(response);
        }
    };
    let permit = match tokio::time::timeout(
        std::time::Duration::from_secs(1),
        state.mcp_host.audit_requests.clone().acquire_owned(),
    )
    .await
    {
        Ok(Ok(permit)) => permit,
        _ => {
            let response = failure(coded(
                CommandError::conflict("MCP 审计请求容量已满，请稍后重试"),
                "MCP_AUDIT_BUSY",
            ));
            audit::record_detached(invocation.as_deref(), &actor, &tool, &response);
            return Ok(response);
        }
    };
    drop(waiting);
    tokio::task::spawn_blocking(move || {
        let _permit = permit;
        let response = failure(error);
        audit::record(&state, invocation.as_deref(), &actor, &tool, &response)?;
        Ok(response)
    })
    .await
    .map_err(CommandError::from_join_error)?
}

fn execute_for_active_case(
    state: &AppState,
    request: &McpHostToolCallRequestDto,
    invocation: &Invocation,
) -> Result<McpHostToolCallResultDto, CommandError> {
    let active = match snapshot_active_case(state) {
        Ok(Some(active))
            if active.case_id == invocation.case_id
                && active.session_id == invocation.session_id =>
        {
            active
        }
        result => {
            let error = match result {
                Err(error) => error,
                _ => coded(
                    CommandError::conflict("案件上下文已失效"),
                    "MCP_CASE_CHANGED",
                ),
            };
            let response = failure(error);
            audit::record_detached(
                Some(invocation),
                &invocation.actor,
                &request.name,
                &response,
            );
            return Ok(response);
        }
    };
    let response = match query_active_case(state, request, invocation, &active) {
        Ok(response) => response,
        Err(error) => failure(error),
    };
    audit::record_for_active(&active, invocation, &request.name, &response)?;
    Ok(response)
}

fn query_active_case(
    state: &AppState,
    request: &McpHostToolCallRequestDto,
    invocation: &Invocation,
    active: &ActiveCaseSnapshot,
) -> Result<McpHostToolCallResultDto, CommandError> {
    invocation.check(&request.name, &request.arguments)?;
    if !state
        .mcp_host
        .tool_enabled(&request.name)
        .map_err(CommandError::from_typed_service_error)?
    {
        return Err(coded(
            CommandError::security("工具或 MCP 服务已禁用"),
            "MCP_TOOL_DISABLED",
        ));
    }
    if request.name == "forensics.cancel_job" {
        return cancel_job(state, active, request, invocation);
    }
    if request.name == "forensics.start_content_index" {
        return start_content_index(state, active, request, invocation);
    }
    if request.name == "forensics.start_analysis" {
        return start_analysis(state, active, request, invocation);
    }
    if request.name == "forensics.start_report_export" {
        return start_report_export(state, active, request, invocation);
    }
    if request.name == "forensics.extract_file" {
        let conn = app_services::connection::open_existing_case_db(&active.db_path())
            .map_err(CommandError::from_typed_service_error)?;
        let mut effective = request.clone();
        if let Some(principal) = invocation.principal.as_ref() {
            let destination = principal
                .resolve_export_destination(
                    &conn,
                    &active.case_root,
                    &active.meta.id,
                    &request.arguments,
                )
                .map_err(|_| CommandError::security("导出目录授权无效"))?;
            let arguments = effective
                .arguments
                .as_object_mut()
                .ok_or_else(|| CommandError::invalid_input("工具参数必须是对象"))?;
            arguments.remove("destinationGrantId");
            arguments.remove("relativePath");
            arguments.insert(
                "destinationPath".into(),
                serde_json::Value::String(destination.to_string_lossy().into_owned()),
            );
        }
        return Ok(tool_response(dispatch_tool(
            state,
            McpHostQueryContext {
                connection: &conn,
                case_root: &active.case_root,
                case_meta: &active.meta,
            },
            &effective,
        )));
    }
    let _scope = invocation.reads.enter();
    let conn = rusqlite::Connection::open_with_flags(
        active.db_path(),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .map_err(|_| CommandError::internal("无法读取当前案件"))?;
    conn.busy_timeout(std::time::Duration::from_secs(5))
        .map_err(|_| CommandError::internal("无法读取当前案件"))?;
    let _interrupt = invocation.interrupt_sql(&conn);
    let result = dispatch_tool(
        state,
        McpHostQueryContext {
            connection: &conn,
            case_root: &active.case_root,
            case_meta: &active.meta,
        },
        request,
    );
    invocation.check(&request.name, &request.arguments)?;
    Ok(tool_response(result))
}

fn cancel_job(
    state: &AppState,
    active: &ActiveCaseSnapshot,
    request: &McpHostToolCallRequestDto,
    invocation: &Invocation,
) -> Result<McpHostToolCallResultDto, CommandError> {
    let input: transport::dto::mcp_host::McpHostJobRequestDto =
        serde_json::from_value(request.arguments.clone())
            .map_err(|_| CommandError::invalid_input("任务标识无效"))?;
    let connection = app_services::connection::open_existing_case_db(&active.db_path())
        .map_err(CommandError::from_typed_service_error)?;
    let job_id = domain::JobId(input.job_id.clone());
    if let Some(principal) = invocation.principal.as_ref() {
        let owns_task = state
            .mcp_host
            .owns_task(&input.job_id, &principal.client.id)
            .map_err(CommandError::from_typed_service_error)?;
        if !owns_task {
            return Err(CommandError::security("任务未授权给当前 MCP 客户端"));
        }
    }
    let changed = app_services::job_service::request_cancellation(
        &connection,
        &job_id,
        "Cancellation requested by MCP client",
    )
    .map_err(CommandError::from_typed_service_error)?;
    if changed {
        state.task_manager.cancel(&input.job_id);
    }
    let job = app_services::job_service::get_job_from_db(&connection, &job_id)
        .map_err(CommandError::from_typed_service_error)?;
    Ok(tool_response(Ok(serde_json::json!({
        "jobId": input.job_id,
        "cancelRequested": changed,
        "status": job.status,
    }))))
}

fn start_content_index(
    state: &AppState,
    active: &ActiveCaseSnapshot,
    request: &McpHostToolCallRequestDto,
    invocation: &Invocation,
) -> Result<McpHostToolCallResultDto, CommandError> {
    let input: transport::dto::mcp_host::McpHostStartContentIndexRequestDto =
        serde_json::from_value(request.arguments.clone())
            .map_err(|_| CommandError::invalid_input("内容索引数据源无效"))?;
    let principal = invocation.principal.as_ref();
    let arguments_sha256 = infrastructure::hashing::sha256_bytes(input.data_source_id.as_bytes());
    let binding_connection = if principal.is_some() && input.idempotency_key.is_some() {
        Some(
            app_services::connection::open_existing_case_db(&active.db_path())
                .map_err(CommandError::from_typed_service_error)?,
        )
    } else {
        None
    };
    if let (Some(principal), Some(idempotency_key), Some(connection)) = (
        principal,
        input.idempotency_key.as_deref(),
        binding_connection.as_ref(),
    ) {
        let repo = persistence_sqlite::repositories::mcp_task_binding_repo::McpTaskBindingRepo::new(
            connection,
        );
        if let Some(binding) = repo
            .find(
                &active.meta.id.0,
                &principal.client.id,
                "start_content_index",
                idempotency_key,
            )
            .map_err(CommandError::from_typed_service_error)?
        {
            if binding.arguments_sha256 != arguments_sha256 {
                return Err(CommandError::conflict("幂等键已用于其他内容索引请求"));
            }
            let task =
                app_services::evidence_tasks::get(connection, &active.meta.id, &binding.task_id)?;
            let _ = state
                .mcp_host
                .register_task_owner(&task.task_id, &principal.client.id)
                .map_err(CommandError::from_typed_service_error)?;
            return serialize_task(task);
        }
    }
    let app = state
        .app_handle()
        .ok_or_else(|| CommandError::internal("桌面应用句柄尚未初始化"))?;
    let task = crate::evidence_task_jobs::start(
        state.clone(),
        app,
        transport::dto::EvidenceTaskRequestDto::ContentIndex {
            data_source_id: input.data_source_id.clone(),
        },
    )?;
    if let Some(principal) = principal {
        let _ = state
            .mcp_host
            .register_task_owner(&task.task_id, &principal.client.id)
            .map_err(CommandError::from_typed_service_error)?;
        if let (Some(idempotency_key), Some(connection)) = (
            input.idempotency_key.as_deref(),
            binding_connection.as_ref(),
        ) {
            let binding = persistence_sqlite::repositories::mcp_task_binding_repo::McpTaskBinding {
                case_id: active.meta.id.0.clone(),
                case_session_id: active.session_id.clone(),
                principal_id: principal.client.id.clone(),
                operation: "start_content_index".into(),
                idempotency_key: idempotency_key.into(),
                arguments_sha256,
                task_id: task.task_id.clone(),
            };
            let selected =
                persistence_sqlite::repositories::mcp_task_binding_repo::McpTaskBindingRepo::new(
                    connection,
                )
                .insert_or_get(&binding)
                .map_err(CommandError::from_typed_service_error)?;
            if selected.task_id != task.task_id {
                let _ = crate::evidence_task_jobs::cancel(state, &task.task_id);
                let existing = app_services::evidence_tasks::get(
                    connection,
                    &active.meta.id,
                    &selected.task_id,
                )?;
                return serialize_task(existing);
            }
        }
    }
    serialize_task(task)
}

fn serialize_task(
    task: transport::dto::EvidenceTaskDto,
) -> Result<McpHostToolCallResultDto, CommandError> {
    let value =
        serde_json::to_value(task).map_err(|error| CommandError::internal(error.to_string()))?;
    Ok(tool_response(Ok(value)))
}

fn start_analysis(
    state: &AppState,
    active: &ActiveCaseSnapshot,
    request: &McpHostToolCallRequestDto,
    invocation: &Invocation,
) -> Result<McpHostToolCallResultDto, CommandError> {
    let input: transport::dto::mcp_host::McpHostStartAnalysisRequestDto =
        serde_json::from_value(request.arguments.clone())
            .map_err(|_| CommandError::invalid_input("分析任务参数无效"))?;
    let principal = invocation.principal.as_ref();
    let arguments_sha256 = analysis_argument_hash(&input.data_source_id, &input.categories)?;
    let binding_connection = if principal.is_some() && input.idempotency_key.is_some() {
        Some(
            app_services::connection::open_existing_case_db(&active.db_path())
                .map_err(CommandError::from_typed_service_error)?,
        )
    } else {
        None
    };
    if let (Some(principal), Some(key), Some(connection)) = (
        principal,
        input.idempotency_key.as_deref(),
        binding_connection.as_ref(),
    ) {
        let repo = persistence_sqlite::repositories::mcp_task_binding_repo::McpTaskBindingRepo::new(
            connection,
        );
        if let Some(binding) = repo
            .find(
                &active.meta.id.0,
                &principal.client.id,
                "start_analysis",
                key,
            )
            .map_err(CommandError::from_typed_service_error)?
        {
            if binding.arguments_sha256 != arguments_sha256 {
                return Err(CommandError::conflict("幂等键已用于其他分析请求"));
            }
            let job = app_services::job_service::get_job_from_db(
                connection,
                &domain::JobId(binding.task_id),
            )
            .map_err(CommandError::from_typed_service_error)?;
            return Ok(tool_response(Ok(serde_json::to_value(job)
                .map_err(|error| CommandError::internal(error.to_string()))?)));
        }
    }
    let app = state
        .app_handle()
        .ok_or_else(|| CommandError::internal("桌面应用句柄尚未初始化"))?;
    let source_id = input.data_source_id.clone();
    let categories = input.categories.clone();
    let runtime = app_services::analysis_service::AnalysisSourceReadRuntime::with_bitlocker_runtime(
        state.bitlocker_runtime.clone(),
    );
    let task_id = crate::analysis_task_jobs::start_evidence_classification(
        state,
        &app,
        crate::analysis_task_jobs::AnalysisTaskRequest {
            case_id: &active.meta.id.0,
            case_root: &active.case_root,
            data_source_id: Some(&source_id),
            kind: "Evidence classification",
        },
        categories,
        runtime,
    )?;
    if let Some(principal) = principal {
        let _ = state
            .mcp_host
            .register_task_owner(&task_id, &principal.client.id)
            .map_err(CommandError::from_typed_service_error)?;
        if let (Some(key), Some(connection)) = (
            input.idempotency_key.as_deref(),
            binding_connection.as_ref(),
        ) {
            let binding = persistence_sqlite::repositories::mcp_task_binding_repo::McpTaskBinding {
                case_id: active.meta.id.0.clone(),
                case_session_id: active.session_id.clone(),
                principal_id: principal.client.id.clone(),
                operation: "start_analysis".into(),
                idempotency_key: key.into(),
                arguments_sha256,
                task_id: task_id.clone(),
            };
            let _ =
                persistence_sqlite::repositories::mcp_task_binding_repo::McpTaskBindingRepo::new(
                    connection,
                )
                .insert_or_get(&binding)
                .map_err(CommandError::from_typed_service_error)?;
        }
    }
    Ok(tool_response(Ok(serde_json::json!({
        "jobId": task_id,
        "kind": "Evidence classification",
        "status": "pending",
    }))))
}

fn analysis_argument_hash(
    data_source_id: &str,
    categories: &[String],
) -> Result<String, CommandError> {
    let arguments = serde_json::to_vec(&serde_json::json!({
        "dataSourceId": data_source_id,
        "categories": categories,
    }))
    .map_err(|_| CommandError::invalid_input("分析任务参数无效"))?;
    Ok(infrastructure::hashing::sha256_bytes(&arguments))
}

fn start_report_export(
    state: &AppState,
    active: &ActiveCaseSnapshot,
    request: &McpHostToolCallRequestDto,
    invocation: &Invocation,
) -> Result<McpHostToolCallResultDto, CommandError> {
    let input: transport::dto::mcp_host::McpHostStartReportExportRequestDto =
        serde_json::from_value(request.arguments.clone())
            .map_err(|_| CommandError::invalid_input("报告导出参数无效"))?;
    let task_id = crate::mcp_report_task::start(
        state,
        &active.meta,
        &active.meta.id.0,
        &active.case_root,
        &input.template_id,
        &input.format,
    )?;
    if let Some(principal) = invocation.principal.as_ref() {
        let _ = state
            .mcp_host
            .register_task_owner(&task_id, &principal.client.id)
            .map_err(CommandError::from_typed_service_error)?;
    }
    Ok(tool_response(Ok(serde_json::json!({
        "jobId": task_id,
        "kind": "Report export",
        "status": "pending",
    }))))
}

fn dispatch_tool(
    state: &AppState,
    context: McpHostQueryContext<'_>,
    request: &McpHostToolCallRequestDto,
) -> Result<serde_json::Value, app_services::mcp_host_service::McpHostServiceError> {
    if matches!(
        request.name.as_str(),
        "forensics.list_files"
            | "forensics.get_file_metadata"
            | "forensics.browse_registry_key"
            | "forensics.open_registry_hive"
            | "forensics.resolve_registry_key"
            | "forensics.list_registry_subkeys"
            | "forensics.list_registry_values"
            | "forensics.read_registry_value_range"
            | "forensics.close_registry_hive"
            | "forensics.read_file"
            | "forensics.extract_file"
    ) {
        return execute_file_tool(
            McpHostFileQueryContext {
                query: context,
                preview_runtime: &state.preview_runtime,
                bitlocker_runtime: &state.bitlocker_runtime,
                registry_browser: &state.registry_browser,
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
            | "forensics.list_linux_services"
            | "forensics.list_docker_containers"
            | "forensics.list_docker_images"
            | "forensics.get_android_device_info"
            | "forensics.get_android_package_summary"
            | "forensics.get_plugin_family_entries"
            | "forensics.get_analysis_report"
            | "forensics.get_case_overview"
            | "forensics.get_correlation_snapshot"
            | "forensics.query_graph"
            | "forensics.get_provenance_chain"
    )
}
