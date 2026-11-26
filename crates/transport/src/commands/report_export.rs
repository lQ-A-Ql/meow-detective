use crate::dto::ReportFormatDto;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReportScopeDto {
    pub operating_system: bool,
    pub hardware: bool,
    pub browsers: bool,
    pub email: bool,
}

impl Default for ReportScopeDto {
    fn default() -> Self {
        Self {
            operating_system: true,
            hardware: true,
            browsers: true,
            email: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ExportReportRequest {
    pub case_id: String,
    pub format: ReportFormatDto,
    #[serde(default)]
    pub scope: ReportScopeDto,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data_source_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SaveReportRequest {
    pub case_id: String,
    pub report_id: String,
    pub destination: String,
    #[serde(default)]
    pub overwrite: bool,
}
