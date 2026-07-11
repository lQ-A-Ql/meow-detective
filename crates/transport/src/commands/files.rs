use serde::{Deserialize, Serialize};

use super::validation::{validate_export_destination_path, MAX_PAGE_LIMIT};

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
