use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ReportEntityDto {
    pub identity: String,
    pub fields: BTreeMap<String, String>,
    pub counts: BTreeMap<String, u64>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ReportSectionDto {
    pub record_count: u64,
    pub partial: bool,
    pub entities: Vec<ReportEntityDto>,
    pub counts: BTreeMap<String, u64>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ReportSourceSummaryDto {
    pub operating_system: ReportSectionDto,
    pub hardware: ReportSectionDto,
    pub browsers: ReportSectionDto,
    pub email: ReportSectionDto,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReportSourceDto {
    pub data_source_id: String,
    pub name: String,
    pub platform: String,
    pub generation: String,
    pub hash_status: String,
    pub summary_available: bool,
    pub summary: ReportSourceSummaryDto,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReportDocumentDto {
    pub case_id: String,
    pub case_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub case_number: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub examiner: Option<String>,
    pub generated_at: String,
    pub scope: crate::commands::ReportScopeDto,
    pub sources: Vec<ReportSourceDto>,
}
