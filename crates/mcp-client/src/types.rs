//! MCP Protocol Types
//!
//! Core types for the Model Context Protocol.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

use crate::error::{McpError, McpResult};

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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
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

/// Validate and normalize an MCP configuration before it is used or persisted.
pub fn validate_mcp_config(config: &mut McpConfig) -> McpResult<()> {
    let mut seen_server_ids = std::collections::HashSet::new();

    for server in &mut config.servers {
        server.id = server.id.trim().to_string();
        server.name = server.name.trim().to_string();

        if server.id.is_empty() {
            return Err(McpError::Protocol("MCP server id is required".to_string()));
        }
        if !seen_server_ids.insert(server.id.clone()) {
            return Err(McpError::Protocol(format!(
                "Duplicate MCP server id: {}",
                server.id
            )));
        }
        if server.name.is_empty() {
            return Err(McpError::Protocol(format!(
                "MCP server {} name is required",
                server.id
            )));
        }

        validate_mcp_transport(&mut server.transport)?;
    }

    Ok(())
}

/// Validate a single MCP server config.
pub fn validate_mcp_server_config(config: &mut McpServerConfig) -> McpResult<()> {
    let mut mcp_config = McpConfig {
        servers: vec![config.clone()],
        resources: HashMap::new(),
        tools: HashMap::new(),
    };

    validate_mcp_config(&mut mcp_config)?;
    *config = mcp_config
        .servers
        .into_iter()
        .next()
        .ok_or_else(|| McpError::Protocol("MCP server config was not preserved".to_string()))?;
    Ok(())
}

/// Validate and normalize an MCP transport.
pub fn validate_mcp_transport(transport: &mut McpTransport) -> McpResult<()> {
    match transport {
        McpTransport::Sse { url } => validate_sse_url(url),
        McpTransport::Stdio { command, args } => validate_stdio_command(command, args),
    }
}

/// Validate and normalize an SSE endpoint URL.
pub fn validate_sse_url(url: &mut String) -> McpResult<()> {
    let trimmed = url.trim();
    if trimmed.is_empty() {
        return Err(McpError::Protocol("MCP SSE URL is required".to_string()));
    }

    let parsed = reqwest::Url::parse(trimmed)
        .map_err(|e| McpError::Protocol(format!("Invalid MCP SSE URL: {}", e)))?;
    match parsed.scheme() {
        "http" | "https" => {}
        scheme => {
            return Err(McpError::Protocol(format!(
                "Unsupported MCP SSE URL scheme: {}",
                scheme
            )))
        }
    }
    if parsed.host_str().is_none() {
        return Err(McpError::Protocol(
            "MCP SSE URL must include a host".to_string(),
        ));
    }
    if parsed.username() != "" || parsed.password().is_some() {
        return Err(McpError::Protocol(
            "MCP SSE URL must not include embedded credentials".to_string(),
        ));
    }

    *url = parsed.to_string();
    Ok(())
}

/// Validate and normalize a stdio command.
pub fn validate_stdio_command(command: &mut String, args: &[String]) -> McpResult<()> {
    let trimmed = command.trim();
    if trimmed.is_empty() {
        return Err(McpError::Protocol(
            "MCP stdio command is required".to_string(),
        ));
    }
    if trimmed.contains('\0') || args.iter().any(|arg| arg.contains('\0')) {
        return Err(McpError::Protocol(
            "MCP stdio command and args must not contain NUL bytes".to_string(),
        ));
    }

    let path = Path::new(trimmed);
    if path.is_absolute() || path.components().count() > 1 {
        return Err(McpError::Protocol(
            "MCP stdio command must be an executable name, not a path".to_string(),
        ));
    }

    *command = trimmed.to_string();
    Ok(())
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
    fn validates_and_normalizes_sse_urls() {
        let mut url = " http://localhost:3001/sse ".to_string();
        validate_sse_url(&mut url).unwrap();
        assert_eq!(url, "http://localhost:3001/sse");
    }

    #[test]
    fn rejects_unsupported_sse_url_schemes() {
        let mut url = "file:///tmp/mcp.sock".to_string();
        let err = validate_sse_url(&mut url).unwrap_err();
        assert!(err.to_string().contains("Unsupported MCP SSE URL scheme"));
    }

    #[test]
    fn rejects_sse_urls_with_embedded_credentials() {
        let mut url = "https://user:pass@example.com/sse".to_string();
        let err = validate_sse_url(&mut url).unwrap_err();
        assert!(err.to_string().contains("embedded credentials"));
    }

    #[test]
    fn validates_and_normalizes_stdio_command() {
        let mut command = " node ".to_string();
        validate_stdio_command(&mut command, &["server.js".to_string()]).unwrap();
        assert_eq!(command, "node");
    }

    #[test]
    fn rejects_stdio_command_paths() {
        let mut command = "./node".to_string();
        let err = validate_stdio_command(&mut command, &[]).unwrap_err();
        assert!(err.to_string().contains("not a path"));
    }

    #[test]
    fn rejects_duplicate_mcp_server_ids() {
        let mut config = McpConfig {
            servers: vec![
                McpServerConfig {
                    id: "srv".to_string(),
                    name: "One".to_string(),
                    transport: McpTransport::Sse {
                        url: "http://localhost:3001/sse".to_string(),
                    },
                    enabled: true,
                    auto_connect: false,
                },
                McpServerConfig {
                    id: " srv ".to_string(),
                    name: "Two".to_string(),
                    transport: McpTransport::Sse {
                        url: "http://localhost:3002/sse".to_string(),
                    },
                    enabled: true,
                    auto_connect: false,
                },
            ],
            resources: HashMap::new(),
            tools: HashMap::new(),
        };

        let err = validate_mcp_config(&mut config).unwrap_err();
        assert!(err.to_string().contains("Duplicate MCP server id"));
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

    #[test]
    fn validates_sse_urls() {
        let mut url = " http://localhost:3001/mcp ".to_string();
        validate_sse_url(&mut url).unwrap();
        assert_eq!(url, "http://localhost:3001/mcp");

        let mut https = "https://example.test/sse".to_string();
        assert!(validate_sse_url(&mut https).is_ok());

        let mut file_url = "file:///tmp/server".to_string();
        assert!(validate_sse_url(&mut file_url).is_err());

        let mut credential_url = "http://user:pass@example.test/sse".to_string();
        assert!(validate_sse_url(&mut credential_url).is_err());

        let mut missing_host = "http:///".to_string();
        assert!(validate_sse_url(&mut missing_host).is_err());
    }

    #[test]
    fn validates_stdio_commands() {
        let mut command = " python ".to_string();
        validate_stdio_command(&mut command, &[]).unwrap();
        assert_eq!(command, "python");

        let mut empty = String::new();
        assert!(validate_stdio_command(&mut empty, &[]).is_err());

        let mut relative = "../server".to_string();
        assert!(validate_stdio_command(&mut relative, &[]).is_err());

        let mut absolute = "C:\\tools\\server.exe".to_string();
        assert!(validate_stdio_command(&mut absolute, &[]).is_err());

        let mut nul_command = "server\0name".to_string();
        assert!(validate_stdio_command(&mut nul_command, &[]).is_err());

        let mut valid = "server".to_string();
        assert!(validate_stdio_command(&mut valid, &["bad\0arg".to_string()]).is_err());
    }

    #[test]
    fn validates_config_and_rejects_duplicate_ids() {
        let mut config = McpConfig {
            servers: vec![McpServerConfig {
                id: " test ".to_string(),
                name: " Test Server ".to_string(),
                transport: McpTransport::Sse {
                    url: " http://localhost:3001/sse ".to_string(),
                },
                enabled: true,
                auto_connect: false,
            }],
            resources: HashMap::new(),
            tools: HashMap::new(),
        };

        validate_mcp_config(&mut config).unwrap();
        assert_eq!(config.servers[0].id, "test");
        assert_eq!(config.servers[0].name, "Test Server");
        assert_eq!(
            config.servers[0].transport,
            McpTransport::Sse {
                url: "http://localhost:3001/sse".to_string()
            }
        );

        config.servers.push(McpServerConfig {
            id: "test".to_string(),
            name: "Duplicate".to_string(),
            transport: McpTransport::Stdio {
                command: "python".to_string(),
                args: vec![],
            },
            enabled: true,
            auto_connect: false,
        });
        assert!(validate_mcp_config(&mut config).is_err());
    }
}
