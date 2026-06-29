use serde::{Deserialize, Serialize};

use crate::dto::analysis_base::AnalysisParseStatusDto;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserHistorySummaryDto {
    pub status: AnalysisParseStatusDto,
    pub visit_total: u64,
    pub download_total: u64,
    pub cookie_total: u64,
    pub session_total: u64,
    pub password_total: u64,
    pub visits: Vec<BrowserVisitDto>,
    pub downloads: Vec<BrowserDownloadDto>,
    pub cookies: Vec<BrowserCookieDto>,
    pub sessions: Vec<BrowserSessionTabDto>,
    pub passwords: Vec<BrowserPasswordDto>,
    pub generated_at: String,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserVisitDto {
    pub artifact_id: String,
    pub file_id: String,
    pub source_path: String,
    pub browser: String,
    pub profile: String,
    pub url: String,
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub visit_time: Option<String>,
    pub visit_count: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserDownloadDto {
    pub artifact_id: String,
    pub file_id: String,
    pub source_path: String,
    pub browser: String,
    pub profile: String,
    pub url: String,
    pub target_path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_time: Option<String>,
    pub total_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserCookieDto {
    pub artifact_id: String,
    pub file_id: String,
    pub source_path: String,
    pub browser: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub profile: Option<String>,
    pub domain: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value_preview: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expiry: Option<String>,
    pub secure: bool,
    pub http_only: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub same_site: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserSessionTabDto {
    pub artifact_id: String,
    pub file_id: String,
    pub source_path: String,
    pub browser: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub profile: Option<String>,
    pub url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    pub window_index: i32,
    pub tab_index: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_active: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserPasswordDto {
    pub artifact_id: String,
    pub file_id: String,
    pub source_path: String,
    pub browser: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub profile: Option<String>,
    pub url: String,
    pub username: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub password_preview: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_at: Option<String>,
    pub times_used: u64,
}
