//! MCP (Model Context Protocol) DTOs
//!
//! Data transfer objects for MCP functionality.

use serde::{Deserialize, Serialize};

/// MCP 配置 DTO
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpConfigDto {
    /// 服务器列表
    pub servers: Vec<McpServerConfigDto>,
    /// 资源启用配置
    pub resources: std::collections::HashMap<String, bool>,
    /// 工具启用配置
    pub tools: std::collections::HashMap<String, bool>,
}

/// MCP 服务器配置 DTO
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerConfigDto {
    /// 服务器 ID
    pub id: String,
    /// 服务器名称
    pub name: String,
    /// 传输类型: "sse" | "stdio"
    pub transport_type: String,
    /// SSE URL (当 transport_type = "sse")
    pub url: Option<String>,
    /// Stdio 命令 (当 transport_type = "stdio")
    pub command: Option<String>,
    /// Stdio 参数 (当 transport_type = "stdio")
    pub args: Option<Vec<String>>,
    /// 是否启用
    pub enabled: bool,
    /// 是否自动连接
    pub auto_connect: bool,
}

/// MCP 服务器状态 DTO
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerStatusDto {
    /// 服务器 ID
    pub id: String,
    /// 服务器名称
    pub name: String,
    /// 是否已连接
    pub connected: bool,
    /// 是否支持 Resources
    pub has_resources: bool,
    /// 是否支持 Tools
    pub has_tools: bool,
    /// 是否支持 Prompts
    pub has_prompts: bool,
    /// 最后错误信息
    pub last_error: Option<String>,
}

/// MCP Resource DTO
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpResourceDto {
    /// 资源 URI
    pub uri: String,
    /// 资源名称
    pub name: String,
    /// 资源描述
    pub description: Option<String>,
    /// MIME 类型
    pub mime_type: Option<String>,
}

/// MCP Tool DTO
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpToolDto {
    /// 工具名称
    pub name: String,
    /// 工具描述
    pub description: String,
    /// 输入参数 Schema (JSON)
    pub input_schema: serde_json::Value,
}

/// MCP Prompt DTO
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpPromptDto {
    /// Prompt 名称
    pub name: String,
    /// Prompt 描述
    pub description: Option<String>,
    /// 参数列表
    pub arguments: Vec<McpPromptArgumentDto>,
}

/// MCP Prompt 参数 DTO
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpPromptArgumentDto {
    /// 参数名称
    pub name: String,
    /// 参数描述
    pub description: Option<String>,
    /// 是否必需
    pub required: bool,
}

/// MCP Tool 调用请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpToolCallRequest {
    /// 服务器 ID
    pub server_id: String,
    /// 工具名称
    pub tool_name: String,
    /// 参数
    pub arguments: serde_json::Value,
}

/// MCP Tool 调用结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpToolCallResult {
    /// 是否成功
    pub success: bool,
    /// 结果数据
    pub data: Option<serde_json::Value>,
    /// 错误信息
    pub error: Option<String>,
}

/// MCP 测试连接请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpTestConnectionRequest {
    /// 传输类型: "sse" | "stdio"
    pub transport_type: String,
    /// SSE URL
    pub url: Option<String>,
    /// Stdio 命令
    pub command: Option<String>,
    /// Stdio 参数
    pub args: Option<Vec<String>>,
}

/// MCP 测试连接结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpTestConnectionResult {
    /// 是否成功
    pub success: bool,
    /// 错误信息
    pub error: Option<String>,
    /// 服务器能力
    pub capabilities: Option<McpCapabilitiesDto>,
}

/// MCP 能力 DTO
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpCapabilitiesDto {
    /// 是否支持 Resources
    pub resources: bool,
    /// 是否支持 Tools
    pub tools: bool,
    /// 是否支持 Prompts
    pub prompts: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn server_config_serializes_current_snake_case_response_fields() {
        let server = McpServerConfigDto {
            id: "srv-1".to_string(),
            name: "Local MCP".to_string(),
            transport_type: "stdio".to_string(),
            url: None,
            command: Some("node".to_string()),
            args: Some(vec!["server.js".to_string()]),
            enabled: true,
            auto_connect: false,
        };

        let value = serde_json::to_value(server).expect("serialize server config");

        assert_eq!(value["transport_type"], "stdio");
        assert_eq!(value["auto_connect"], false);
        assert!(value.get("transportType").is_none());
        assert!(value.get("autoConnect").is_none());
    }

    #[test]
    fn server_status_serializes_current_snake_case_capability_fields() {
        let status = McpServerStatusDto {
            id: "srv-1".to_string(),
            name: "Local MCP".to_string(),
            connected: true,
            has_resources: true,
            has_tools: false,
            has_prompts: true,
            last_error: Some("boom".to_string()),
        };

        let value = serde_json::to_value(status).expect("serialize server status");

        assert_eq!(value["has_resources"], true);
        assert_eq!(value["has_tools"], false);
        assert_eq!(value["has_prompts"], true);
        assert_eq!(value["last_error"], "boom");
        assert!(value.get("hasResources").is_none());
        assert!(value.get("hasTools").is_none());
        assert!(value.get("hasPrompts").is_none());
        assert!(value.get("lastError").is_none());
    }

    #[test]
    fn tool_call_request_accepts_current_snake_case_request_fields() {
        let value = json!({
            "server_id": "srv-1",
            "tool_name": "lookup",
            "arguments": { "query": "mft" }
        });

        let request: McpToolCallRequest =
            serde_json::from_value(value).expect("deserialize tool call request");

        assert_eq!(request.server_id, "srv-1");
        assert_eq!(request.tool_name, "lookup");
        assert_eq!(request.arguments["query"], "mft");
    }

    #[test]
    fn tool_call_request_documents_camel_case_boundary_is_top_level_only() {
        let value = json!({
            "serverId": "srv-1",
            "toolName": "lookup",
            "arguments": { "query": "mft" }
        });

        let error = serde_json::from_value::<McpToolCallRequest>(value)
            .expect_err("camelCase is reserved for Tauri command args, not nested MCP requests");

        assert!(error.to_string().contains("server_id"));
    }

    #[test]
    fn test_connection_request_accepts_current_snake_case_transport_field() {
        let value = json!({
            "transport_type": "sse",
            "url": "http://127.0.0.1:3000/sse",
            "command": null,
            "args": null
        });

        let request: McpTestConnectionRequest =
            serde_json::from_value(value).expect("deserialize test connection request");

        assert_eq!(request.transport_type, "sse");
        assert_eq!(request.url.as_deref(), Some("http://127.0.0.1:3000/sse"));
    }

    #[test]
    fn protocol_dtos_keep_current_nested_snake_case_fields() {
        let resource = McpResourceDto {
            uri: "file:///case/report".to_string(),
            name: "Case report".to_string(),
            description: Some("Report resource".to_string()),
            mime_type: Some("text/markdown".to_string()),
        };
        let tool = McpToolDto {
            name: "lookup".to_string(),
            description: "Lookup evidence".to_string(),
            input_schema: json!({ "type": "object" }),
        };

        let resource_value = serde_json::to_value(resource).expect("serialize resource");
        let tool_value = serde_json::to_value(tool).expect("serialize tool");

        assert_eq!(resource_value["mime_type"], "text/markdown");
        assert_eq!(tool_value["input_schema"]["type"], "object");
        assert!(resource_value.get("mimeType").is_none());
        assert!(tool_value.get("inputSchema").is_none());
    }

    #[test]
    fn server_config_request_documents_snake_case_protocol_boundary() {
        let value = json!({
            "id": "srv-1",
            "name": "Local MCP",
            "transport_type": "stdio",
            "url": null,
            "command": "node",
            "args": ["server.js"],
            "enabled": true,
            "auto_connect": true
        });

        let server: McpServerConfigDto =
            serde_json::from_value(value).expect("deserialize server config");

        assert_eq!(server.transport_type, "stdio");
        assert!(server.auto_connect);

        let camel_case_value = json!({
            "id": "srv-1",
            "name": "Local MCP",
            "transportType": "stdio",
            "enabled": true,
            "autoConnect": true
        });

        let error = serde_json::from_value::<McpServerConfigDto>(camel_case_value)
            .expect_err("frontend maps camelCase server input before sending nested DTOs");

        assert!(error.to_string().contains("transport_type"));
    }
}
