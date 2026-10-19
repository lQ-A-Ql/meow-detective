use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppUpdateInfoDto {
    pub current_version: String,
    pub configured: bool,
    pub portable: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub available: Option<AppUpdateReleaseDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppUpdateReleaseDto {
    pub version: String,
    pub notes: String,
    pub schema_compatible: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppUpdateProgressDto {
    pub phase: String,
    pub downloaded_bytes: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_bytes: Option<u64>,
}

/// Metadata appended to the signed executable, covered by the update signature.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PortablePackageDto {
    pub format: u32,
    pub version: String,
    pub case_schema: String,
}
