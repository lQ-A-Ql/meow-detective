//! Embedded MCP server settings and read-only tool contracts.
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct McpHostSettingsDto {
    pub enabled: bool,
    #[serde(default)]
    pub disabled_tools: Vec<String>,
}

impl Default for McpHostSettingsDto {
    fn default() -> Self {
        Self {
            enabled: true,
            disabled_tools: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpHostStatusDto {
    pub settings: McpHostSettingsDto,
    pub running: bool,
    pub endpoint: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpHostToolDto {
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct McpHostToolCallRequestDto {
    pub name: String,
    #[serde(default = "empty_arguments")]
    pub arguments: serde_json::Value,
}

fn empty_arguments() -> serde_json::Value {
    serde_json::json!({})
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpHostToolCallResultDto {
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

pub const MCP_FILE_PAGE_LIMIT: u32 = 200;
pub const MCP_FILE_CHUNK_LIMIT: u32 = 64 * 1024;
pub const MCP_MAX_SAFE_OFFSET: u64 = 9_007_199_254_740_991;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct McpHostListFilesRequestDto {
    pub data_source_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<String>,
    #[serde(default)]
    pub offset: u64,
    #[serde(default = "default_file_page_limit")]
    pub limit: u32,
}

fn default_file_page_limit() -> u32 {
    100
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct McpHostFileRequestDto {
    pub file_id: String,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum McpHostFileEncodingDto {
    #[default]
    Auto,
    Utf8,
    Base64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct McpHostReadFileRequestDto {
    pub file_id: String,
    #[serde(default)]
    pub offset: u64,
    #[serde(default = "default_file_chunk_length")]
    pub length: u32,
    #[serde(default)]
    pub encoding: McpHostFileEncodingDto,
}

fn default_file_chunk_length() -> u32 {
    MCP_FILE_CHUNK_LIMIT
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpHostFileChunkDto {
    pub file_id: String,
    pub size: u64,
    pub offset: u64,
    pub bytes_read: u32,
    pub next_offset: u64,
    pub eof: bool,
    pub encoding: McpHostFileEncodingDto,
    pub content: String,
    pub chunk_sha256: String,
}

#[cfg(test)]
#[path = "../../tests/unit/dto/mcp_host.rs"]
mod tests;
