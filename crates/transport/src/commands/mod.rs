use serde::{Deserialize, Serialize};

pub use crate::dto::{
    ArtifactRowDto, CaseMetricsDto, CaseSummaryDto, DataSourceSummaryDto, FileChildrenDto,
    FileEntryRowDto, FileTreeNodeDto, JobSnapshotDto, RecentCaseDto, RecentObjectDto,
    ReportHistoryItemDto, ReportTemplateDto, SearchResultPageDto, TimelineEventDto, TraceItemDto,
    ViewerHandleDto, ViewerRangeRequestDto, ViewerRangeResponseDto, WarningItemDto,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateCaseRequest {
    pub case_root: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub examiner: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenCaseRequest {
    pub case_root: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportDataSourceRequest {
    pub source_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenFileHandleRequest {
    pub file_id: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetFileRowsRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetFileChildrenRequest {
    pub parent_id: String,
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

fn default_search_limit() -> u32 {
    50
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetArtifactRowsRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub family: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RenameDataSourceRequest {
    pub data_source_id: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeleteCaseRequest {
    pub case_root: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeleteDataSourceRequest {
    pub data_source_id: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetTimelineRequest {
    #[serde(default)]
    pub offset: u64,
    #[serde(default = "default_timeline_limit")]
    pub limit: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub time_start: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub time_end: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub event_type: Option<String>,
}

fn default_timeline_limit() -> u32 {
    100
}
