//! Rule pack response DTOs.
//!
//! Types returned to the frontend from the rule-pack Tauri commands.

use serde::{Deserialize, Serialize};

// ── Response DTOs ───────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RulePackSummaryDto {
    pub id: String,
    pub name: String,
    pub version: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub status: String,
    pub rule_count: u32,
    pub loaded_at: String,
    pub warnings: Vec<String>,
    pub errors: Vec<String>,
    pub covered_families: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RulePackCoverageDto {
    pub covered_families: Vec<String>,
    pub uncovered_families: Vec<String>,
    pub coverage_percent: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RulePackValidationResultDto {
    pub pack_id: String,
    pub valid: bool,
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
    pub coverage: RulePackCoverageDto,
}
