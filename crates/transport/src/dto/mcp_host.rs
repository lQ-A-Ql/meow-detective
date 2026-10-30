//! Embedded MCP server settings and tool contracts.
use serde::{Deserialize, Serialize};

pub mod access;

pub const MCP_HOST_POLICY_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct McpHostSettingsDto {
    pub enabled: bool,
    #[serde(default)]
    pub disabled_tools: Vec<String>,
    #[serde(default)]
    pub policy_version: u32,
}

impl Default for McpHostSettingsDto {
    fn default() -> Self {
        Self {
            enabled: true,
            disabled_tools: Vec::new(),
            policy_version: MCP_HOST_POLICY_VERSION,
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
    pub output_schema: serde_json::Value,
    pub read_only: bool,
    pub destructive: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct McpHostToolCallRequestDto {
    pub name: String,
    #[serde(default = "empty_arguments")]
    pub arguments: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct McpHostJobRequestDto {
    pub job_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct McpHostStartContentIndexRequestDto {
    pub data_source_id: String,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_details: Option<crate::errors::ApiErrorDto>,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct McpHostRegistryBrowseRequestDto {
    pub file_id: String,
    #[serde(default)]
    pub key_path: String,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct McpHostExtractFileRequestDto {
    pub file_id: String,
    #[serde(default)]
    pub destination_path: Option<String>,
    #[serde(default)]
    pub destination_grant_id: Option<String>,
    #[serde(default)]
    pub relative_path: Option<String>,
    #[serde(default)]
    pub overwrite: bool,
}

impl McpHostExtractFileRequestDto {
    pub fn validate(&self) -> Result<(), String> {
        if self.file_id.trim().is_empty() || self.file_id.len() > 2048 {
            return Err("fileId is invalid".to_string());
        }
        let Some(destination_path) = self.destination_path.as_deref() else {
            if self.destination_grant_id.is_some() {
                return Err("destinationGrantId is no longer accepted".to_string());
            }
            if self.relative_path.as_deref().is_some_and(str::is_empty) {
                return Err("relativePath is invalid".to_string());
            }
            return Ok(());
        };
        if destination_path.trim().is_empty() || destination_path.len() > 32_767 {
            return Err("destinationPath is invalid".to_string());
        }
        if destination_path.contains('\0') {
            return Err("destinationPath contains a null byte".to_string());
        }
        let destination = std::path::Path::new(destination_path);
        if !destination.is_absolute() {
            return Err("destinationPath must be an absolute path".to_string());
        }
        if destination.file_name().is_none() {
            return Err("destinationPath must include a file name".to_string());
        }
        let normalized = destination_path.replace('/', "\\");
        let upper = normalized.to_ascii_uppercase();
        if upper.starts_with("\\\\.\\") || upper.starts_with("\\\\?\\") {
            return Err("device destination paths are not supported".to_string());
        }
        #[cfg(windows)]
        for component in destination.components() {
            if matches!(component, std::path::Component::Normal(_))
                && component.as_os_str().to_string_lossy().contains(':')
            {
                return Err(
                    "destinationPath must not use a Windows alternate data stream".to_string(),
                );
            }
        }
        Ok(())
    }
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
