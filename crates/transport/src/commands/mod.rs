use serde::{Deserialize, Serialize};

pub use crate::dto::{
    ArtifactRowDto, CaseMetricsDto, CaseSummaryDto, FileEntryRowDto, FileTreeNodeDto,
    JobSnapshotDto, RecentObjectDto, ReportHistoryItemDto, ReportTemplateDto, SearchResultPageDto,
    TimelineEventDto, TraceItemDto, ViewerHandleDto, ViewerRangeRequestDto, ViewerRangeResponseDto,
    WarningItemDto,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenFileHandleRequest {
    pub file_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchFilesRequest {
    pub query: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetArtifactRowsRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub family: Option<String>,
}
