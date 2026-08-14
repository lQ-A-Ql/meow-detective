use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum FileExtractionPhaseDto {
    Preparing,
    Copying,
    Finalizing,
    Completed,
    CompletedWithWarning,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct FileExtractionProgressDto {
    pub operation_id: String,
    pub file_id: String,
    pub phase: FileExtractionPhaseDto,
    pub bytes_written: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_bytes: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub percent: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct FileExtractionResultDto {
    pub file_id: String,
    pub bytes_written: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_size: Option<u64>,
    pub sha256: String,
    pub destination_file_name: String,
    pub size_verified: bool,
    pub audit_persisted: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub warning: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileTreeNodeDto {
    pub id: String,
    pub name: String,
    pub depth: u32,
    pub has_children: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data_source_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entry_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
    pub deleted: bool,
    pub hidden: bool,
    pub system: bool,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub encrypted: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub node_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expanded: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub active: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileChildrenDto {
    pub children: Vec<FileTreeNodeDto>,
    pub total_count: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub offset: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub truncated: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileRowsPageDto {
    pub rows: Vec<FileEntryRowDto>,
    pub total_count: u64,
    pub offset: u64,
    pub limit: u32,
    pub truncated: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileJumpContextDto {
    pub target: FileEntryRowDto,
    pub directory: FileEntryRowDto,
    pub ancestor_directory_ids: Vec<String>,
    pub row_offset: u64,
    pub requires_show_hidden: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileEntryRowDto {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<String>,
    pub path: String,
    pub name: String,
    pub entry_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ext: Option<String>,
    pub deleted: bool,
    pub hidden: bool,
    pub system: bool,
    pub read_only: bool,
    pub archive: bool,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub encrypted: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub modified_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub accessed_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub changed_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hash_sha256: Option<String>,
}

#[cfg(test)]
#[path = "../../tests/unit/dto/files.rs"]
mod tests;
