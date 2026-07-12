//! MCP DTOs shared across the Tauri boundary.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpConfigDto {
    pub servers: Vec<McpServerConfigDto>,
    pub resources: std::collections::HashMap<String, bool>,
    pub tools: std::collections::HashMap<String, bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerConfigDto {
    pub id: String,
    pub name: String,
    pub transport_type: String,
    pub url: Option<String>,
    pub command: Option<String>,
    pub args: Option<Vec<String>>,
    pub enabled: bool,
    pub auto_connect: bool,
    #[serde(default)]
    pub permissions: McpPermissionProfileDto,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct McpPermissionProfileDto {
    pub resource_access: String,
    pub tool_access: String,
    pub prompt_access: String,
    pub network_policy: String,
    pub allowed_tools: Vec<String>,
    pub allowed_commands: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerStatusDto {
    pub id: String,
    pub name: String,
    pub connected: bool,
    pub has_resources: bool,
    pub has_tools: bool,
    pub has_prompts: bool,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpResourceDto {
    pub uri: String,
    pub name: String,
    pub description: Option<String>,
    pub mime_type: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpToolDto {
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpPromptDto {
    pub name: String,
    pub description: Option<String>,
    pub arguments: Vec<McpPromptArgumentDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpPromptArgumentDto {
    pub name: String,
    pub description: Option<String>,
    pub required: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpToolCallRequestDto {
    pub server_id: String,
    pub tool_name: String,
    pub arguments: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpToolCallResultDto {
    pub success: bool,
    pub data: Option<serde_json::Value>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpTestConnectionRequestDto {
    pub transport_type: String,
    pub url: Option<String>,
    pub command: Option<String>,
    pub args: Option<Vec<String>>,
    #[serde(default)]
    pub permissions: McpPermissionProfileDto,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpTestConnectionResultDto {
    pub success: bool,
    pub error: Option<String>,
    pub capabilities: Option<McpCapabilitiesDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpCapabilitiesDto {
    pub resources: bool,
    pub tools: bool,
    pub prompts: bool,
}

#[cfg(test)]
#[path = "../../tests/unit/dto/mcp.rs"]
mod tests;
