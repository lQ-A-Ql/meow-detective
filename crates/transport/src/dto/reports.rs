use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReportHistoryItemDto {
    pub id: String,
    pub file_name: String,
    pub created_by: String,
    pub created_at: String,
    pub status: super::ReportStatusDto,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub progress: Option<u32>,
    pub request: crate::commands::ExportReportRequest,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_size: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sha256: Option<String>,
}

#[cfg(test)]
#[path = "../../tests/unit/dto/reports.rs"]
mod tests;
