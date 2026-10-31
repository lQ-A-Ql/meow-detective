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
        Prompt, PromptArgument, PromptMessage, ReadResourceRequestParams, ReadResourceResponse,
        Resource, ResourceContents, ResourceTemplate, Role, ServerCapabilities, ServerConfig, Tool,
        ToolAnnotations,
    },
    service::{RequestContext, RoleServer},
    ErrorData, ServerHandler,
};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};
mod task_monitor;
use task_monitor::wait_for_background_task;
use tokio_util::sync::CancellationToken;
use transport::dto::mcp_host::McpHostToolCallRequestDto;

#[derive(Clone)]
struct HostHandler {
    state: AppState,
    tasks: Arc<TaskManager>,
    task_owners: Arc<Mutex<HashMap<String, String>>>,
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
            let principal_for_monitor = principal.clone();
            let request_for_task = request.clone();
            let task = self
                .tasks
                .spawn(TaskOptions::default(), move |task_context| {
                    Box::pin(async move {
                        let cancellation = CancellationToken::new();
                        let worker = call_tool_as(
                            state.clone(),
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
                                let result = wait_for_background_task(
                                    &state,
                                    &principal_for_monitor,
                                    result,
                                    &task_context,
                                )
                                .await?;
                                Ok(CallToolResult::structured(result.data.expect(
                                    "successful MCP calls must provide structured data",
                                )))
                            }
                            Ok(result) => Ok(error_response(result)),
                            Err(error) => Ok(error_response(super::result::failure(error))),
                        }
                    })
                });
            let task_id = task.task_id.clone();
            self.task_owners
                .lock()
                .map_err(|_| ErrorData::internal_error("任务授权状态不可用", None))?
                .insert(task_id, principal.client.id.clone());
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
        context: RequestContext<RoleServer>,
    ) -> impl std::future::Future<Output = Result<rmcp::model::GetTaskResult, ErrorData>> + '_ {
        let result = self
            .authorize_task(&request.task_id, &context)
            .and_then(|_| {
                self.tasks
                    .get_task(&request.task_id)
                    .map(rmcp::model::GetTaskResult::new)
            });
        if let Ok(principal) = authorization::principal(&context) {
            super::audit::record_protocol(&self.state, &principal, "mcp.tasks.get", result.is_ok());
        }
        std::future::ready(result)
    }

    fn update_task(
        &self,
        request: rmcp::model::UpdateTaskParams,
        context: RequestContext<RoleServer>,
    ) -> impl std::future::Future<Output = Result<(), ErrorData>> + '_ {
        let result = self
            .authorize_task(&request.task_id, &context)
            .and_then(|_| {
                self.tasks
                    .update_task(&request.task_id, request.input_responses)
                    .map(|_| ())
            });
        if let Ok(principal) = authorization::principal(&context) {
            super::audit::record_protocol(
                &self.state,
                &principal,
                "mcp.tasks.update",
                result.is_ok(),
            );
        }
        std::future::ready(result)
    }

    fn cancel_task(
        &self,
        request: rmcp::model::CancelTaskParams,
        context: RequestContext<RoleServer>,
    ) -> impl std::future::Future<Output = Result<(), ErrorData>> + '_ {
        let result = self.authorize_task(&request.task_id, &context);
        let result = result.and_then(|_| self.tasks.cancel_task(&request.task_id));
        if let Ok(principal) = authorization::principal(&context) {
            super::audit::record_protocol(
                &self.state,
                &principal,
                "mcp.tasks.cancel",
                result.is_ok(),
            );
        }
        std::future::ready(result)
    }

    fn list_resources(
        &self,
        _request: Option<PaginatedRequestParams>,
        context: RequestContext<RoleServer>,
    ) -> impl std::future::Future<Output = Result<ListResourcesResult, ErrorData>> + '_ {
        let result = authorization::principal(&context).map(|principal| {
            super::audit::record_protocol(&self.state, &principal, "mcp.resources.list", true);
            let uri = format!("forensics://cases/{}/overview", principal.client.case_id);
            ListResourcesResult::with_all_items(vec![Resource::new(uri, "case-overview")
                .with_title("当前案件概览")
                .with_description("当前案件概览，只返回当前授权案件的数据")])
        });
        std::future::ready(result)
    }

    fn list_resource_templates(
        &self,
        _request: Option<PaginatedRequestParams>,
        context: RequestContext<RoleServer>,
    ) -> impl std::future::Future<Output = Result<ListResourceTemplatesResult, ErrorData>> + '_
    {
        let result = authorization::principal(&context).map(|_| {
            ListResourceTemplatesResult::with_all_items(vec![ResourceTemplate::new(
                "forensics://cases/{caseId}/overview",
                "case-overview-template",
            )
            .with_title("案件概览资源")])
        });
        if let Ok(principal) = authorization::principal(&context) {
            super::audit::record_protocol(
                &self.state,
                &principal,
                "mcp.resources.templates",
                result.is_ok(),
            );
        }
        std::future::ready(result)
    }

    fn read_resource(
        &self,
        request: ReadResourceRequestParams,
        context: RequestContext<RoleServer>,
    ) -> impl std::future::Future<Output = Result<ReadResourceResponse, ErrorData>> + '_ {
        let state = self.state.clone();
        let uri = request.uri;
        let principal = authorization::principal(&context);
        let cancel = context.ct;
        Box::pin(async move {
            let principal = principal?;
            if !resource_matches_case(&uri, &principal.client.case_id) {
                super::audit::record_protocol(&state, &principal, "mcp.resources.read", false);
                return Err(ErrorData::resource_not_found("资源不存在", None));
            }
            let request = McpHostToolCallRequestDto {
                name: "forensics.get_case_overview".into(),
                arguments: serde_json::json!({}),
            };
            let result = call_tool_as(state.clone(), request, Some(principal.clone()), cancel)
                .await
                .map_err(|error| ErrorData::internal_error(error.message, None))?;
            super::audit::record_protocol(&state, &principal, "mcp.resources.read", result.success);
            if !result.success {
                return Err(ErrorData::invalid_request(
                    result.error.unwrap_or_else(|| "资源读取失败".into()),
                    None,
                ));
            }
            let text = serde_json::to_string(&result.data.unwrap_or_default())
                .map_err(|_| ErrorData::internal_error("资源编码失败", None))?;
            Ok(ReadResourceResponse::from(
                rmcp::model::ReadResourceResult::new(vec![
                    ResourceContents::TextResourceContents {
                        uri,
                        mime_type: Some("application/json".into()),
                        text,
                        meta: None,
                    },
                ]),
            ))
        })
    }

    fn list_prompts(
        &self,
        _request: Option<PaginatedRequestParams>,
        context: RequestContext<RoleServer>,
    ) -> impl std::future::Future<Output = Result<ListPromptsResult, ErrorData>> + '_ {
        let result = authorization::principal(&context).map(|principal| {
            super::audit::record_protocol(&self.state, &principal, "mcp.prompts.list", true);
            ListPromptsResult::with_all_items(vec![
                Prompt::new(
                    "case-overview",
                    Some("案件概览调查模板"),
                    Some(vec![PromptArgument::new("dataSourceId")]),
                ),
                Prompt::new(
                    "linux-services-docker",
                    Some("Linux 服务与 Docker 检查模板"),
                    Some(vec![PromptArgument::new("dataSourceId").with_required(true)]),
                ),
                Prompt::new(
                    "timeline-investigation",
                    Some("时间线调查模板"),
                    Some(vec![
                        PromptArgument::new("dataSourceId"),
                        PromptArgument::new("from"),
                        PromptArgument::new("to"),
                    ]),
                ),
            ])
        });
        std::future::ready(result)
    }

    fn get_prompt(
        &self,
        request: GetPromptRequestParams,
        context: RequestContext<RoleServer>,
    ) -> impl std::future::Future<Output = Result<GetPromptResponse, ErrorData>> + '_ {
        let principal = authorization::principal(&context);
        let result = principal
            .as_ref()
            .map_err(Clone::clone)
            .and_then(|principal| {
                let arguments = request.arguments.unwrap_or_default();
                validate_prompt_arguments(&request.name, &arguments, principal)?;
                let uri = format!("forensics://cases/{}/overview", principal.client.case_id);
                let description = match request.name.as_str() {
                    "case-overview" => "先读取案件概览和数据源，再根据来源继续调查。",
                    "linux-services-docker" => {
                        "读取 Linux 服务、Docker 容器和镜像，再预览来源文件。"
                    }
                    "timeline-investigation" => "从时间线、搜索和来源链逐步建立调查线索。",
                    _ => return Err(ErrorData::invalid_params("未知 Prompt", None)),
                };
                Ok(GetPromptResponse::from(rmcp::model::GetPromptResult::new(
                    vec![
                        PromptMessage::new_text(Role::User, description),
                        PromptMessage::new_resource_link(
                            Role::User,
                            Resource::new(uri, "case-overview"),
                        ),
                    ],
                )))
            });
        if let Ok(principal) = principal {
            super::audit::record_protocol(
                &self.state,
                &principal,
                "mcp.prompts.get",
                result.is_ok(),
            );
        }
        std::future::ready(result)
    }
}

fn resource_matches_case(uri: &str, case_id: &str) -> bool {
    let expected = format!("forensics://cases/{case_id}/overview");
    uri == expected || uri == "forensics://cases/current/overview"
}

fn validate_prompt_arguments(
    name: &str,
    arguments: &rmcp::model::JsonObject,
    principal: &app_services::mcp_host_service::access::McpPrincipal,
) -> Result<(), ErrorData> {
    let allowed = match name {
        "case-overview" => ["dataSourceId"].as_slice(),
        "linux-services-docker" => ["dataSourceId"].as_slice(),
        "timeline-investigation" => ["dataSourceId", "from", "to"].as_slice(),
        _ => return Err(ErrorData::invalid_params("未知 Prompt", None)),
    };
    if arguments.keys().any(|key| !allowed.contains(&key.as_str())) {
        return Err(ErrorData::invalid_params("Prompt 参数无效", None));
    }
    let source_id = arguments.get("dataSourceId");
    if name == "linux-services-docker" && !source_id.is_some_and(serde_json::Value::is_string) {
        return Err(ErrorData::invalid_params("缺少数据源标识", None));
    }
    if source_id.is_some_and(|value| {
        value
            .as_str()
            .is_none_or(|id| id.is_empty() || id.len() > 256)
    }) {
        return Err(ErrorData::invalid_params("数据源标识无效", None));
    }
    if let Some(source_id) = source_id.and_then(serde_json::Value::as_str) {
        if !principal.allows_data_source_id(source_id) {
            return Err(ErrorData::invalid_request("数据源未获授权", None));
        }
    }
    for key in ["from", "to"] {
        if arguments.get(key).is_some_and(|value| {
            value.as_str().is_none_or(|text| {
                text.len() > 64 || chrono::DateTime::parse_from_rfc3339(text).is_err()
            })
        }) {
            return Err(ErrorData::invalid_params("时间范围参数无效", None));
        }
    }
    Ok(())
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
    let task_owners = Arc::new(Mutex::new(HashMap::new()));
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
                task_owners: task_owners.clone(),
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

impl HostHandler {
    fn authorize_task(
        &self,
        task_id: &str,
        context: &RequestContext<RoleServer>,
    ) -> Result<app_services::mcp_host_service::access::McpPrincipal, ErrorData> {
        let principal = authorization::principal(context)?;
        let owners = self
            .task_owners
            .lock()
            .map_err(|_| ErrorData::internal_error("任务授权状态不可用", None))?;
        if owners.get(task_id) != Some(&principal.client.id) {
            return Err(ErrorData::invalid_request("任务不可用", None));
        }
        Ok(principal)
    }
}
