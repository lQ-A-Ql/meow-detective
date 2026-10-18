use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PngCarvingRequestDto {
    pub data_source_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub offset: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub length: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ZipCarvingRequestDto {
    pub data_source_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub offset: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub length: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CarvedArtifactDto {
    pub artifact_id: String,
    pub data_source_id: String,
    pub source_revision: String,
    pub view: String,
    pub offset: String,
    pub length: String,
    pub sha256: String,
    pub validator_version: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct PngCarvingSummaryDto {
    pub scanned_bytes: String,
    pub range_offset: String,
    pub range_length: String,
    pub candidates: u32,
    pub complete: u32,
    pub partial: u32,
    pub rejected: u32,
    pub artifacts: Vec<CarvedArtifactDto>,
    pub artifacts_truncated: bool,
    pub rejections: Vec<CarvingRejectionDto>,
}
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ZipCarvingSummaryDto {
    pub scanned_bytes: String,
    pub range_offset: String,
    pub range_length: String,
    pub candidates: u32,
    pub complete: u32,
    pub partial: u32,
    pub rejected: u32,
    pub artifacts: Vec<CarvedArtifactDto>,
    pub artifacts_truncated: bool,
    pub rejections: Vec<CarvingRejectionDto>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CarvingRejectionDto {
    pub offset: String,
    pub status: String,
    pub reason: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CarvedRangeRequestDto {
    pub artifact_id: String,
    pub offset: String,
    pub length: u32,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CarvedRangeDto {
    pub artifact_id: String,
    pub offset: String,
    pub base64: String,
    pub eof: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CarvedExportRequestDto {
    pub artifact_id: String,
    pub destination_path: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CarvedExportDto {
    pub artifact_id: String,
    pub bytes_written: String,
    pub sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CarvedSearchRequestDto {
    pub data_source_id: String,
    #[serde(default)]
    pub query: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cursor: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CarvedArtifactPageDto {
    pub items: Vec<CarvedArtifactDto>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
}
