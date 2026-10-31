use super::{authorization, execution::call_tool_as};
use crate::state::AppState;
use rmcp::task_manager::{TaskExit, TaskManager, TaskOptions};
use rmcp::transport::streamable_http_server::{
    session::local::LocalSessionManager, StreamableHttpServerConfig, StreamableHttpService,
};
use rmcp::{
    model::{
        CallToolRequestParams, CallToolResponse, CallToolResult, ContentBlock,
        GetPromptRequestParams, GetPromptResponse, Implementation, ListPromptsResult,
        ListResourceTemplatesResult, ListResourcesResult, ListToolsResult, PaginatedRequestParams,
        Prompt, PromptMessage, ReadResourceRequestParams, ReadResourceResponse, Resource,
        ResourceContents, ResourceTemplate, Role, ServerCapabilities, ServerConfig, Tool,
        ToolAnnotations,
    },
    service::{RequestContext, RoleServer},
    ErrorData, ServerHandler,
};
use std::sync::Arc;
use tokio_util::sync::CancellationToken;
use transport::dto::mcp_host::McpHostToolCallRequestDto;

#[derive(Clone)]
struct HostHandler {
    state: AppState,
    tasks: Arc<TaskManager>,
}

impl ServerHandler for HostHandler {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(
            ServerCapabilities::builder()
                .enable_tools()
                .enable_prompts()
                .enable_resources()
                .enable_tasks()
                .build(),
        )
            .with_server_info(Implementation::new("Meow_Detective", env!("CARGO_PKG_VERSION")))
            .with_instructions("Forensic tools for the case currently open in Meow~Detective. Start with list_data_sources for dataSourceId values, use search_files or list_files to obtain fileId values, and use the timeline, data-source analysis, and content-index coverage tools for investigator-facing views. Queries preserve original evidence. The extract_file tool writes an exported copy and is disabled by default.")
    }

    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, ErrorData> {
        let principal = authorization::principal(&context)?;
        let tools = app_services::mcp_host_service::tool_catalog()
            .into_iter()
            .filter(|tool| principal.allows(tool))
            .map(|tool| {
                let output_schema =
                    Arc::new(tool.output_schema.as_object().cloned().unwrap_or_default());
                Tool::new(
                    tool.name,
                    tool.description,
                    tool.input_schema.as_object().cloned().unwrap_or_default(),
                )
                .with_raw_output_schema(output_schema)
                .with_annotations(ToolAnnotations::from_raw(
                    None,
                    Some(tool.read_only),
                    Some(tool.destructive),
                    Some(tool.read_only),
                    Some(false),
                ))
            })
            .collect();
        Ok(ListToolsResult {
            tools,
            ..Default::default()
        })
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, ErrorData> {
        let principal = authorization::principal(&context)?;
        let request = McpHostToolCallRequestDto {
            name: request.name.into_owned(),
            arguments: serde_json::Value::Object(request.arguments.unwrap_or_default()),
        };
        if supports_tasks(&request.name, &context) {
            let state = self.state.clone();
            let principal_for_task = principal.clone();
            let request_for_task = request.clone();
            let task = self
                .tasks
                .spawn(TaskOptions::default(), move |task_context| {
                    Box::pin(async move {
                        let cancellation = CancellationToken::new();
                        let worker = call_tool_as(
                            state,
                            request_for_task,
                            Some(principal_for_task),
                            cancellation.clone(),
                        );
                        tokio::pin!(worker);
                        let result = tokio::select! {
                            result = &mut worker => result,
                            _ = task_context.cancelled() => {
                                cancellation.cancel();
                                let _ = worker.await;
                                return Err(TaskExit::Cancelled);
                            }
                        };
                        match result {
                            Ok(result) if result.success => {
                                Ok(CallToolResult::structured(result.data.expect(
                                    "successful MCP calls must provide structured data",
                                )))
                            }
                            Ok(result) => Ok(error_response(result)),
                            Err(error) => Ok(error_response(super::result::failure(error))),
                        }
                    })
                });
            return Ok(rmcp::model::CallToolResponse::from(
                rmcp::model::CreateTaskResult::new(task),
            ));
        }
        let result = call_tool_as(self.state.clone(), request, Some(principal), context.ct).await;
        let result = match result {
            Ok(result) if result.success => CallToolResult::structured(
                result
                    .data
                    .expect("successful MCP calls must provide structured data"),
            ),
            Ok(result) => error_response(result),
            Err(error) => error_response(super::result::failure(error)),
        };
        Ok(result.into())
    }

    fn get_task(
        &self,
        request: rmcp::model::GetTaskParams,
        _context: RequestContext<RoleServer>,
    ) -> impl std::future::Future<Output = Result<rmcp::model::GetTaskResult, ErrorData>> + '_ {
        std::future::ready(
            self.tasks
                .get_task(&request.task_id)
                .map(rmcp::model::GetTaskResult::new),
        )
    }

    fn update_task(
        &self,
        request: rmcp::model::UpdateTaskParams,
        _context: RequestContext<RoleServer>,
    ) -> impl std::future::Future<Output = Result<(), ErrorData>> + '_ {
        std::future::ready(
            self.tasks
                .update_task(&request.task_id, request.input_responses)
                .map(|_| ()),
        )
    }

    fn cancel_task(
        &self,
        request: rmcp::model::CancelTaskParams,
        _context: RequestContext<RoleServer>,
    ) -> impl std::future::Future<Output = Result<(), ErrorData>> + '_ {
        std::future::ready(self.tasks.cancel_task(&request.task_id))
    }

    fn list_resources(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> impl std::future::Future<Output = Result<ListResourcesResult, ErrorData>> + '_ {
        std::future::ready(Ok(ListResourcesResult::with_all_items(vec![
            Resource::new("forensics://cases/current/overview", "case-overview")
                .with_title("当前案件概览")
                .with_description("当前案件的摘要入口，详细数据继续通过工具读取"),
        ])))
    }

    fn list_resource_templates(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> impl std::future::Future<Output = Result<ListResourceTemplatesResult, ErrorData>> + '_
    {
        std::future::ready(Ok(ListResourceTemplatesResult::with_all_items(vec![
            ResourceTemplate::new(
                "forensics://cases/{caseId}/overview",
                "case-overview-template",
            )
            .with_title("案件概览资源"),
        ])))
    }

    fn read_resource(
        &self,
        request: ReadResourceRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> impl std::future::Future<Output = Result<ReadResourceResponse, ErrorData>> + '_ {
        let result = if request.uri == "forensics://cases/current/overview"
            || request.uri.starts_with("forensics://cases/")
        {
            Ok(ReadResourceResponse::from(rmcp::model::ReadResourceResult::new(vec![
                ResourceContents::TextResourceContents {
                    uri: request.uri,
                    mime_type: Some("text/plain".into()),
                    text: "请使用 get_current_case、list_data_sources 和 get_case_overview 获取当前案件数据。".into(),
                    meta: None,
                },
            ])))
        } else {
            Err(ErrorData::resource_not_found(request.uri, None))
        };
        std::future::ready(result)
    }

    fn list_prompts(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> impl std::future::Future<Output = Result<ListPromptsResult, ErrorData>> + '_ {
        std::future::ready(Ok(ListPromptsResult::with_all_items(vec![
            Prompt::new("case-overview", Some("案件概览调查模板"), None),
            Prompt::new(
                "linux-services-docker",
                Some("Linux 服务与 Docker 检查模板"),
                None,
            ),
            Prompt::new("timeline-investigation", Some("时间线调查模板"), None),
        ])))
    }

    fn get_prompt(
        &self,
        request: GetPromptRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> impl std::future::Future<Output = Result<GetPromptResponse, ErrorData>> + '_ {
        let description = match request.name.as_str() {
            "case-overview" => "先读取案件概览和数据源，再根据来源继续调查。",
            "linux-services-docker" => "读取 Linux 服务、Docker 容器和镜像，再预览来源文件。",
            "timeline-investigation" => "从时间线、搜索和来源链逐步建立调查线索。",
            _ => return std::future::ready(Err(ErrorData::invalid_params("未知 Prompt", None))),
        };
        std::future::ready(Ok(GetPromptResponse::from(
            rmcp::model::GetPromptResult::new(vec![PromptMessage::new_text(
                Role::User,
                description,
            )]),
        )))
    }
}

fn supports_tasks(name: &str, context: &RequestContext<RoleServer>) -> bool {
    matches!(
        name,
        "forensics.start_content_index"
            | "forensics.start_analysis"
            | "forensics.start_report_export"
    ) && context
        .client_capabilities()
        .is_some_and(|capabilities| capabilities.supports_tasks())
}

fn error_response(result: transport::dto::mcp_host::McpHostToolCallResultDto) -> CallToolResult {
    let mut response = CallToolResult::error(vec![ContentBlock::text(
        result
            .error
            .expect("failed MCP calls must provide a message"),
    )]);
    response.structured_content = Some(serde_json::json!({
        "error":result.error_details.expect("failed MCP calls must provide typed error details")
    }));
    response
}

pub(super) fn router(state: AppState, cancel: CancellationToken) -> axum::Router {
    let authorization_state = state.clone();
    let tasks = Arc::new(TaskManager::new());
    let mut config = StreamableHttpServerConfig::default().enforce_origin_validation();
    config.legacy_session_mode = false;
    config.json_response = true;
    config.cancellation_token = cancel;
    config.allowed_hosts = vec!["127.0.0.1".into(), "localhost".into()];
    config.max_request_body_bytes = 64 * 1024;
    let service = StreamableHttpService::new(
        move || {
            Ok(HostHandler {
                state: state.clone(),
                tasks: tasks.clone(),
            })
        },
        Arc::new(LocalSessionManager::default()),
        config,
    );
    axum::Router::new()
        .route_service("/mcp", service)
        .layer(axum::middleware::from_fn_with_state(
            authorization_state,
            authorization::authenticate,
        ))
}
