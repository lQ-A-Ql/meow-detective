use serde::{Deserialize, Serialize};

use super::validation::{validate_export_destination_path, MAX_PAGE_LIMIT};
use crate::paging::validate_opaque_cursor;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenFileHandleRequest {
    pub file_id: String,
}

impl OpenFileHandleRequest {
    pub fn validate(&self) -> Result<(), String> {
        if self.file_id.trim().is_empty() {
            return Err("fileId is required".to_string());
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtractFileRequest {
    pub file_id: String,
    pub destination_path: String,
    #[serde(default)]
    pub overwrite: bool,
}

impl ExtractFileRequest {
    pub fn validate(&self) -> Result<(), String> {
        if self.file_id.trim().is_empty() {
            return Err("fileId is required".to_string());
        }
        validate_export_destination_path(&self.destination_path)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetFileRowsRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<String>,
    #[serde(default)]
    pub offset: u64,
    #[serde(default = "default_file_browser_limit")]
    pub limit: u32,
    #[serde(default)]
    pub show_hidden: bool,
    #[serde(default)]
    pub sort_key: FileSortKeyDto,
    #[serde(default)]
    pub sort_direction: FileSortDirectionDto,
}

impl Default for GetFileRowsRequest {
    fn default() -> Self {
        Self {
            parent_id: None,
            offset: 0,
            limit: default_file_browser_limit(),
            show_hidden: false,
            sort_key: FileSortKeyDto::default(),
            sort_direction: FileSortDirectionDto::default(),
        }
    }
}

impl GetFileRowsRequest {
    pub fn validate(&mut self) -> Result<(), String> {
        if self.limit == 0 {
            self.limit = default_file_browser_limit();
        }
        self.limit = self.limit.min(MAX_PAGE_LIMIT);
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetFileJumpContextRequest {
    pub file_id: String,
    #[serde(default)]
    pub show_hidden: bool,
    #[serde(default = "default_file_browser_limit")]
    pub page_limit: u32,
    #[serde(default)]
    pub sort_key: FileSortKeyDto,
    #[serde(default)]
    pub sort_direction: FileSortDirectionDto,
}

impl GetFileJumpContextRequest {
    pub fn validate(&mut self) -> Result<(), String> {
        if self.file_id.trim().is_empty() {
            return Err("fileId is required".to_string());
        }
        if self.page_limit == 0 {
            self.page_limit = default_file_browser_limit();
        }
        self.page_limit = self.page_limit.min(MAX_PAGE_LIMIT);
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetFileChildrenRequest {
    pub parent_id: String,
    #[serde(default)]
    pub offset: u64,
    #[serde(default = "default_file_tree_limit")]
    pub limit: u32,
    #[serde(default)]
    pub show_hidden: bool,
}

impl GetFileChildrenRequest {
    pub fn validate(&mut self) -> Result<(), String> {
        if self.parent_id.trim().is_empty() {
            return Err("parentId is required".to_string());
        }
        if self.limit == 0 {
            self.limit = default_file_tree_limit();
        }
        self.limit = self.limit.min(MAX_PAGE_LIMIT);
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct GetFileTreeRequest {
    #[serde(default)]
    pub show_hidden: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub enum FileSortKeyDto {
    #[default]
    Name,
    Size,
    ModifiedAt,
    Ext,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub enum FileSortDirectionDto {
    #[default]
    Asc,
    Desc,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchFilesRequest {
    pub query: String,
    #[serde(default)]
    pub offset: u64,
    #[serde(default = "default_search_limit")]
    pub limit: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cursor: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunDeletedRecoveryRequest {
    pub data_source_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub partition_index: Option<u32>,
}

impl RunDeletedRecoveryRequest {
    pub fn validate(&self) -> Result<(), String> {
        validate_data_source_id(&self.data_source_id)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListDeletedRecoveriesRequest {
    pub data_source_id: String,
    pub partition_index: u32,
    #[serde(default)]
    pub offset: u64,
    #[serde(default = "default_recovery_limit")]
    pub limit: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReadDeletedRecoveryRangeRequest {
    pub data_source_id: String,
    pub recovery_id: String,
    pub offset: u64,
    pub length: u32,
}

impl ReadDeletedRecoveryRangeRequest {
    pub fn validate(&mut self) -> Result<(), String> {
        validate_data_source_id(&self.data_source_id)?;
        validate_recovery_id(&self.recovery_id)?;
        if self.length == 0 {
            return Err("length must be greater than zero".to_string());
        }
        self.length = self.length.min(1024 * 1024);
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportDeletedRecoveryRequest {
    pub data_source_id: String,
    pub recovery_id: String,
    pub destination_path: String,
    #[serde(default)]
    pub overwrite: bool,
}

impl ExportDeletedRecoveryRequest {
    pub fn validate(&self) -> Result<(), String> {
        validate_data_source_id(&self.data_source_id)?;
        validate_recovery_id(&self.recovery_id)?;
        validate_export_destination_path(&self.destination_path)
    }
}

impl ListDeletedRecoveriesRequest {
    pub fn validate(&mut self) -> Result<(), String> {
        validate_data_source_id(&self.data_source_id)?;
        if self.limit == 0 {
            self.limit = default_recovery_limit();
        }
        self.limit = self.limit.min(MAX_PAGE_LIMIT);
        Ok(())
    }
}

impl SearchFilesRequest {
    pub fn validate(&mut self) -> Result<(), String> {
        if self.query.trim().is_empty() {
            return Err("query is required".to_string());
        }
        if self.limit == 0 {
            self.limit = default_search_limit();
        }
        self.limit = self.limit.min(MAX_PAGE_LIMIT);
        if let Some(cursor) = self.cursor.as_deref() {
            validate_opaque_cursor(cursor).map_err(|error| error.to_string())?;
            if self.offset != 0 {
                return Err("offset must be zero when cursor is provided".to_string());
            }
        }
        Ok(())
    }
}

fn default_file_browser_limit() -> u32 {
    500
}

fn default_file_tree_limit() -> u32 {
    500
}

fn default_search_limit() -> u32 {
    50
}

fn default_recovery_limit() -> u32 {
    100
}

fn validate_data_source_id(value: &str) -> Result<(), String> {
    if value.is_empty()
        || value.len() > 256
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
    {
        return Err("dataSourceId is invalid".to_string());
    }
    Ok(())
}

fn validate_recovery_id(value: &str) -> Result<(), String> {
    let hash = value
        .strip_prefix("recovery:")
        .ok_or_else(|| "recoveryId is invalid".to_string())?;
    if hash.len() != 64
        || !hash
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
    {
        return Err("recoveryId is invalid".to_string());
    }
    Ok(())
}
