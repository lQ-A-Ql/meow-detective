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
