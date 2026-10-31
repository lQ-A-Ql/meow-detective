use super::super::{invocation::Invocation, result::tool_response};
use crate::commands::command_support::ActiveCaseSnapshot;
use crate::state::AppState;
use infrastructure::hashing::sha256_bytes;
use persistence_sqlite::repositories::mcp_task_binding_repo::{McpTaskBinding, McpTaskBindingRepo};
use transport::{
    dto::{
        mcp_host::{
            McpHostJobRequestDto, McpHostStartAnalysisRequestDto,
            McpHostStartContentIndexRequestDto, McpHostStartReportExportRequestDto,
            McpHostToolCallRequestDto, McpHostToolCallResultDto,
        },
        EvidenceTaskDto,
    },
    CommandError,
};

pub(super) fn cancel_job(
    state: &AppState,
    active: &ActiveCaseSnapshot,
    request: &McpHostToolCallRequestDto,
    invocation: &Invocation,
) -> Result<McpHostToolCallResultDto, CommandError> {
    let input: McpHostJobRequestDto = serde_json::from_value(request.arguments.clone())
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

pub(super) fn start_content_index(
    state: &AppState,
    active: &ActiveCaseSnapshot,
    request: &McpHostToolCallRequestDto,
    invocation: &Invocation,
) -> Result<McpHostToolCallResultDto, CommandError> {
    let input: McpHostStartContentIndexRequestDto =
        serde_json::from_value(request.arguments.clone())
            .map_err(|_| CommandError::invalid_input("内容索引数据源无效"))?;
    let principal = invocation.principal.as_ref();
    let _binding_guard = if principal.is_some() && input.idempotency_key.is_some() {
        Some(state.mcp_host.lock_task_bindings())
    } else {
        None
    };
    let arguments_sha256 = sha256_bytes(input.data_source_id.as_bytes());
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
        let repo = McpTaskBindingRepo::new(connection);
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
            let binding = McpTaskBinding {
                case_id: active.meta.id.0.clone(),
                case_session_id: active.session_id.clone(),
                principal_id: principal.client.id.clone(),
                operation: "start_content_index".into(),
                idempotency_key: idempotency_key.into(),
                arguments_sha256,
                task_id: task.task_id.clone(),
            };
            let selected = McpTaskBindingRepo::new(connection)
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

fn serialize_task(task: EvidenceTaskDto) -> Result<McpHostToolCallResultDto, CommandError> {
    let value =
        serde_json::to_value(task).map_err(|error| CommandError::internal(error.to_string()))?;
    Ok(tool_response(Ok(value)))
}

pub(super) fn start_analysis(
    state: &AppState,
    active: &ActiveCaseSnapshot,
    request: &McpHostToolCallRequestDto,
    invocation: &Invocation,
) -> Result<McpHostToolCallResultDto, CommandError> {
    let input: McpHostStartAnalysisRequestDto =
        serde_json::from_value(request.arguments.clone())
            .map_err(|_| CommandError::invalid_input("分析任务参数无效"))?;
    let principal = invocation.principal.as_ref();
    let _binding_guard = if principal.is_some() && input.idempotency_key.is_some() {
        Some(state.mcp_host.lock_task_bindings())
    } else {
        None
    };
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
        if let Some(job) = find_analysis_binding(
            connection,
            active,
            &principal.client.id,
            key,
            &arguments_sha256,
        )? {
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
            if let Some(job) = bind_analysis_task(
                connection,
                active,
                &principal.client.id,
                key,
                &arguments_sha256,
                &task_id,
                state,
            )? {
                return Ok(tool_response(Ok(serde_json::to_value(job)
                    .map_err(|error| CommandError::internal(error.to_string()))?)));
            }
        }
    }
    Ok(tool_response(Ok(serde_json::json!({
        "jobId": task_id,
        "kind": "Evidence classification",
        "status": "pending",
    }))))
}

fn find_analysis_binding(
    connection: &rusqlite::Connection,
    active: &ActiveCaseSnapshot,
    principal_id: &str,
    key: &str,
    arguments_sha256: &str,
) -> Result<Option<transport::dto::JobSnapshotDto>, CommandError> {
    let Some(binding) = McpTaskBindingRepo::new(connection)
        .find(&active.meta.id.0, principal_id, "start_analysis", key)
        .map_err(CommandError::from_typed_service_error)?
    else {
        return Ok(None);
    };
    if binding.arguments_sha256 != arguments_sha256 {
        return Err(CommandError::conflict("幂等键已用于其他分析请求"));
    }
    app_services::job_service::get_job_from_db(connection, &domain::JobId(binding.task_id))
        .map(Some)
        .map_err(CommandError::from_typed_service_error)
}

fn bind_analysis_task(
    connection: &rusqlite::Connection,
    active: &ActiveCaseSnapshot,
    principal_id: &str,
    key: &str,
    arguments_sha256: &str,
    task_id: &str,
    state: &AppState,
) -> Result<Option<transport::dto::JobSnapshotDto>, CommandError> {
    let binding = McpTaskBinding {
        case_id: active.meta.id.0.clone(),
        case_session_id: active.session_id.clone(),
        principal_id: principal_id.into(),
        operation: "start_analysis".into(),
        idempotency_key: key.into(),
        arguments_sha256: arguments_sha256.into(),
        task_id: task_id.into(),
    };
    let selected = McpTaskBindingRepo::new(connection)
        .insert_or_get(&binding)
        .map_err(CommandError::from_typed_service_error)?;
    if selected.task_id == task_id {
        return Ok(None);
    }
    let _ = state.task_manager.cancel(task_id);
    app_services::job_service::get_job_from_db(connection, &domain::JobId(selected.task_id))
        .map(Some)
        .map_err(CommandError::from_typed_service_error)
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
    Ok(sha256_bytes(&arguments))
}

pub(super) fn start_report_export(
    state: &AppState,
    active: &ActiveCaseSnapshot,
    request: &McpHostToolCallRequestDto,
    invocation: &Invocation,
) -> Result<McpHostToolCallResultDto, CommandError> {
    let input: McpHostStartReportExportRequestDto =
        serde_json::from_value(request.arguments.clone())
            .map_err(|_| CommandError::invalid_input("报告导出参数无效"))?;
    let principal = invocation.principal.as_ref();
    let arguments_sha256 = sha256_bytes(
        &serde_json::to_vec(&serde_json::json!({
            "templateId": &input.template_id,
            "format": &input.format,
        }))
        .map_err(|error| CommandError::internal(error.to_string()))?,
    );
    let binding_connection = if principal.is_some() && input.idempotency_key.is_some() {
        Some(
            app_services::connection::open_existing_case_db(&active.db_path())
                .map_err(CommandError::from_typed_service_error)?,
        )
    } else {
        None
    };
    let _binding_guard = if principal.is_some() && input.idempotency_key.is_some() {
        Some(state.mcp_host.lock_task_bindings())
    } else {
        None
    };
    if let (Some(principal), Some(key), Some(connection)) = (
        principal,
        input.idempotency_key.as_deref(),
        binding_connection.as_ref(),
    ) {
        if let Some(job) = find_report_binding(
            connection,
            active,
            &principal.client.id,
            key,
            &arguments_sha256,
        )? {
            return Ok(tool_response(Ok(serde_json::to_value(job)
                .map_err(|error| CommandError::internal(error.to_string()))?)));
        }
    }
    let task_id = crate::mcp_report_task::start(
        state,
        &active.meta,
        &active.meta.id.0,
        &active.case_root,
        &input.template_id,
        &input.format,
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
            if let Some(job) = bind_report_task(
                connection,
                active,
                &principal.client.id,
                key,
                &arguments_sha256,
                &task_id,
                state,
            )? {
                return Ok(tool_response(Ok(serde_json::to_value(job)
                    .map_err(|error| CommandError::internal(error.to_string()))?)));
            }
        }
    }
    Ok(tool_response(Ok(serde_json::json!({
        "jobId": task_id,
        "kind": "Report export",
        "status": "pending",
    }))))
}

fn find_report_binding(
    connection: &rusqlite::Connection,
    active: &ActiveCaseSnapshot,
    principal_id: &str,
    key: &str,
    arguments_sha256: &str,
) -> Result<Option<transport::dto::JobSnapshotDto>, CommandError> {
    let Some(binding) = McpTaskBindingRepo::new(connection)
        .find(&active.meta.id.0, principal_id, "start_report_export", key)
        .map_err(CommandError::from_typed_service_error)?
    else {
        return Ok(None);
    };
    if binding.arguments_sha256 != arguments_sha256 {
        return Err(CommandError::conflict("幂等键已用于其他报告导出请求"));
    }
    app_services::job_service::get_job_from_db(connection, &domain::JobId(binding.task_id))
        .map(Some)
        .map_err(CommandError::from_typed_service_error)
}

fn bind_report_task(
    connection: &rusqlite::Connection,
    active: &ActiveCaseSnapshot,
    principal_id: &str,
    key: &str,
    arguments_sha256: &str,
    task_id: &str,
    state: &AppState,
) -> Result<Option<transport::dto::JobSnapshotDto>, CommandError> {
    let binding = McpTaskBinding {
        case_id: active.meta.id.0.clone(),
        case_session_id: active.session_id.clone(),
        principal_id: principal_id.into(),
        operation: "start_report_export".into(),
        idempotency_key: key.into(),
        arguments_sha256: arguments_sha256.into(),
        task_id: task_id.into(),
    };
    let selected = McpTaskBindingRepo::new(connection)
        .insert_or_get(&binding)
        .map_err(CommandError::from_typed_service_error)?;
    if selected.task_id == task_id {
        return Ok(None);
    }
    let _ = state.task_manager.cancel(task_id);
    let _ = app_services::job_service::request_cancellation(
        connection,
        &domain::JobId(task_id.to_owned()),
        "Duplicate MCP report task cancelled",
    );
    app_services::job_service::get_job_from_db(connection, &domain::JobId(selected.task_id))
        .map(Some)
        .map_err(CommandError::from_typed_service_error)
}
