use super::execution::call_tool;
use crate::state::AppState;
use rmcp::transport::streamable_http_server::{
    session::local::LocalSessionManager, StreamableHttpServerConfig, StreamableHttpService,
};
use rmcp::{
    model::{
        CallToolRequestParams, CallToolResponse, CallToolResult, ContentBlock, Implementation,
        ListToolsResult, PaginatedRequestParams, ServerCapabilities, ServerConfig, Tool,
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
}

impl ServerHandler for HostHandler {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new("Meow_Detective", env!("CARGO_PKG_VERSION")))
            .with_instructions("Read-only tools for the case currently open in Meow~Detective. Use dataSourceId from list_data_sources for source and plugin queries.")
    }

    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, ErrorData> {
        let tools = app_services::mcp_host_service::tool_catalog()
            .into_iter()
            .map(|tool| {
                Tool::new(
                    tool.name,
                    tool.description,
                    tool.input_schema.as_object().cloned().unwrap_or_default(),
                )
                .with_annotations(ToolAnnotations::from_raw(
                    None,
                    Some(true),
                    Some(false),
                    Some(true),
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
        _context: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, ErrorData> {
        let request = McpHostToolCallRequestDto {
            name: request.name.into_owned(),
            arguments: serde_json::Value::Object(request.arguments.unwrap_or_default()),
        };
        let result = call_tool(self.state.clone(), request).await;
        let result = match result {
            Ok(result) if result.success => {
                CallToolResult::structured(result.data.unwrap_or_default())
            }
            Ok(result) => CallToolResult::error(vec![ContentBlock::text(
                result.error.unwrap_or_else(|| "工具调用失败".into()),
            )]),
            Err(_) => CallToolResult::error(vec![ContentBlock::text("工具调用失败")]),
        };
        Ok(result.into())
    }
}

pub(super) fn router(state: AppState, cancel: CancellationToken) -> axum::Router {
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
            })
        },
        Arc::new(LocalSessionManager::default()),
        config,
    );
    axum::Router::new().route_service("/mcp", service)
}
