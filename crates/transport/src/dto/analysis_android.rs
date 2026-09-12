use serde::{Deserialize, Serialize};

use crate::dto::analysis_base::AnalysisParseStatusDto;

/// Android device facts selected from standard read-only system artifacts.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AndroidDeviceInfoDto {
    pub status: AnalysisParseStatusDto,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub manufacturer: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub android_version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sdk_int: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub build_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub build_fingerprint: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub serial_number: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub android_id: Option<String>,
    /// IMEI is absent unless a supported, provenance-preserving decoder has
    /// extracted it. The service never regex-scans arbitrary evidence bytes.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub imei: Option<String>,
    pub facts: Vec<AndroidDeviceFactDto>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AndroidDeviceFactDto {
    pub field: String,
    pub value: String,
    pub confidence: String,
    pub source_file_id: String,
    pub source_path: String,
    pub parser: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub warning: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AndroidPackageSummaryDto {
    pub status: AnalysisParseStatusDto,
    pub total_count: u64,
    pub page_total: u64,
    pub packages: Vec<AndroidPackageDto>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AndroidPackageDto {
    pub package_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub app_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version_code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub install_time: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub update_time: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub installer: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uid: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_id: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_state: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code_path: Option<String>,
    pub source_file_id: String,
    pub source_path: String,
    pub parser: String,
    /// Launcher APK resources are resolved only by a deterministic resource
    /// decoder. A missing value tells the UI to use the common fallback avatar.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon_data_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub warning: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AndroidAnalysisRunDto {
    pub status: AnalysisParseStatusDto,
    pub scanned_file_count: u64,
    pub device_fact_count: u64,
    pub package_count: u64,
    pub warnings: Vec<String>,
}
