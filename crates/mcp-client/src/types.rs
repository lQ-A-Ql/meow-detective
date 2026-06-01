//! MCP Protocol Types
//!
//! Core types for the Model Context Protocol.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// MCP 服务器配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerConfig {
    /// 服务器唯一标识
    pub id: String,
    /// 服务器显示名称
    pub name: String,
    /// 传输类型
    pub transport: McpTransport,
    /// 是否启用
    pub enabled: bool,
    /// 是否自动连接
    pub auto_connect: bool,
}

/// MCP 传输方式
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum McpTransport {
    /// HTTP/SSE 传输
    Sse {
        /// SSE 端点 URL
        url: String,
    },
    /// Stdio 传输 (本地进程)
    Stdio {
        /// 命令
        command: String,
        /// 参数
        args: Vec<String>,
    },
}

/// MCP 服务器状态
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerStatus {
    /// 服务器 ID
    pub id: String,
    /// 服务器名称
    pub name: String,
    /// 是否已连接
    pub connected: bool,
    /// 服务器能力
    pub capabilities: McpCapabilities,
    /// 最后错误信息
    pub last_error: Option<String>,
}

/// MCP 能力
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct McpCapabilities {
    /// 是否支持 Resources
    pub resources: bool,
    /// 是否支持 Tools
    pub tools: bool,
    /// 是否支持 Prompts
    pub prompts: bool,
}

/// MCP Resource
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpResource {
    /// 资源 URI (例如: forensics://cases)
    pub uri: String,
    /// 资源名称
    pub name: String,
    /// 资源描述
    pub description: Option<String>,
    /// MIME 类型
    pub mime_type: Option<String>,
}

/// MCP Tool
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpTool {
    /// 工具名称
    pub name: String,
    /// 工具描述
    pub description: String,
    /// 输入参数 JSON Schema
    pub input_schema: serde_json::Value,
}

/// MCP Prompt
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpPrompt {
    /// Prompt 名称
    pub name: String,
    /// Prompt 描述
    pub description: Option<String>,
    /// 参数列表
    pub arguments: Vec<McpPromptArgument>,
}

/// MCP Prompt 参数
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpPromptArgument {
    /// 参数名称
    pub name: String,
    /// 参数描述
    pub description: Option<String>,
    /// 是否必需
    pub required: bool,
}

/// MCP 配置
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct McpConfig {
    /// 服务器列表
    pub servers: Vec<McpServerConfig>,
    /// 资源启用配置
    pub resources: HashMap<String, bool>,
    /// 工具启用配置
    pub tools: HashMap<String, bool>,
}

/// MCP JSON-RPC Request
#[derive(Debug, Serialize)]
pub struct JsonRpcRequest {
    pub jsonrpc: String,
    pub id: u64,
    pub method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<serde_json::Value>,
}

/// MCP JSON-RPC Response
#[derive(Debug, Deserialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: String,
    pub id: Option<u64>,
    pub result: Option<serde_json::Value>,
    pub error: Option<JsonRpcError>,
}

/// MCP JSON-RPC Error
#[derive(Debug, Deserialize)]
pub struct JsonRpcError {
    pub code: i64,
    pub message: String,
    pub data: Option<serde_json::Value>,
}

/// Initialize Request 参数
#[derive(Debug, Serialize)]
pub struct InitializeParams {
    #[serde(rename = "protocolVersion")]
    pub protocol_version: String,
    pub capabilities: ClientCapabilities,
    #[serde(rename = "clientInfo")]
    pub client_info: ClientInfo,
}

/// 客户端能力
#[derive(Debug, Serialize, Default)]
pub struct ClientCapabilities {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub roots: Option<RootsCapability>,
}

/// Roots 能力
#[derive(Debug, Serialize)]
pub struct RootsCapability {
    pub list_changed: bool,
}

/// 客户端信息
#[derive(Debug, Serialize)]
pub struct ClientInfo {
    pub name: String,
    pub version: String,
}

/// Tool Call 参数
#[derive(Debug, Serialize)]
pub struct ToolCallParams {
    pub name: String,
    pub arguments: serde_json::Value,
}

/// Resource Read 参数
#[derive(Debug, Serialize)]
pub struct ResourceReadParams {
    pub uri: String,
}

/// Prompt Get 参数
#[derive(Debug, Serialize)]
pub struct PromptGetParams {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub arguments: Option<HashMap<String, String>>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mcp_server_config_sse() {
        let config = McpServerConfig {
            id: "test".to_string(),
            name: "Test Server".to_string(),
            transport: McpTransport::Sse {
                url: "http://localhost:3001".to_string(),
            },
            enabled: true,
            auto_connect: false,
        };

        let json = serde_json::to_string(&config).unwrap();
        assert!(json.contains("\"Sse\""));
        assert!(json.contains("localhost:3001"));
    }

    #[test]
    fn test_mcp_server_config_stdio() {
        let config = McpServerConfig {
            id: "test".to_string(),
            name: "Test Server".to_string(),
            transport: McpTransport::Stdio {
                command: "python".to_string(),
                args: vec!["-m".to_string(), "mcp_server".to_string()],
            },
            enabled: true,
            auto_connect: true,
        };

        let json = serde_json::to_string(&config).unwrap();
        assert!(json.contains("\"Stdio\""));
        assert!(json.contains("python"));
    }

    #[test]
    fn test_mcp_resource() {
        let resource = McpResource {
            uri: "forensics://cases".to_string(),
            name: "Cases".to_string(),
            description: Some("List of cases".to_string()),
            mime_type: Some("application/json".to_string()),
        };

        let json = serde_json::to_string(&resource).unwrap();
        assert!(json.contains("forensics://cases"));
    }

    #[test]
    fn test_mcp_tool() {
        let tool = McpTool {
            name: "search_files".to_string(),
            description: "Search files".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "query": { "type": "string" }
                }
            }),
        };

        let json = serde_json::to_string(&tool).unwrap();
        assert!(json.contains("search_files"));
    }

    #[test]
    fn test_mcp_config_default() {
        let config = McpConfig::default();
        assert!(config.servers.is_empty());
        assert!(config.resources.is_empty());
        assert!(config.tools.is_empty());
    }

    #[test]
    fn test_mcp_capabilities_default() {
        let caps = McpCapabilities::default();
        assert!(!caps.resources);
        assert!(!caps.tools);
        assert!(!caps.prompts);
    }

    #[test]
    fn test_json_rpc_request() {
        let req = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: 1,
            method: "initialize".to_string(),
            params: Some(serde_json::json!({})),
        };

        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("\"jsonrpc\":\"2.0\""));
        assert!(json.contains("\"method\":\"initialize\""));
    }

    #[test]
    fn test_mcp_server_status() {
        let status = McpServerStatus {
            id: "test".to_string(),
            name: "Test".to_string(),
            connected: true,
            capabilities: McpCapabilities {
                resources: true,
                tools: true,
                prompts: false,
            },
            last_error: None,
        };

        let json = serde_json::to_string(&status).unwrap();
        assert!(json.contains("\"connected\":true"));
        assert!(json.contains("\"resources\":true"));
    }

    #[test]
    fn test_mcp_prompt() {
        let prompt = McpPrompt {
            name: "analyze_timeline".to_string(),
            description: Some("Analyze timeline".to_string()),
            arguments: vec![
                McpPromptArgument {
                    name: "time_start".to_string(),
                    description: Some("Start time".to_string()),
                    required: true,
                },
                McpPromptArgument {
                    name: "time_end".to_string(),
                    description: Some("End time".to_string()),
                    required: false,
                },
            ],
        };

        let json = serde_json::to_string(&prompt).unwrap();
        assert!(json.contains("analyze_timeline"));
        assert!(json.contains("time_start"));
        assert!(json.contains("time_end"));
    }

    #[test]
    fn test_mcp_server_status_no_error() {
        let status = McpServerStatus {
            id: "test".to_string(),
            name: "Test".to_string(),
            connected: false,
            capabilities: McpCapabilities::default(),
            last_error: Some("Connection refused".to_string()),
        };

        assert_eq!(status.last_error.unwrap(), "Connection refused");
    }

    #[test]
    fn test_mcp_resource_no_optional() {
        let resource = McpResource {
            uri: "forensics://test".to_string(),
            name: "Test".to_string(),
            description: None,
            mime_type: None,
        };

        let json = serde_json::to_string(&resource).unwrap();
        assert!(json.contains("forensics://test"));
        // description is null in JSON when None
        assert!(json.contains("\"description\":null"));
    }
}
