use serde::{Deserialize, Serialize};

use crate::dto::analysis_base::{AnalysisParseStatusDto, AnalysisProvenanceDto};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnalysisFileClassificationDto {
    pub category: String,
    pub files: Vec<AnalysisClassifiedFileDto>,
    pub file_count: u64,
    pub total_size: u64,
    pub status: AnalysisParseStatusDto,
    pub warnings: Vec<String>,
    pub provenance: Vec<AnalysisProvenanceDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EvidenceClassificationSummaryDto {
    pub status: AnalysisParseStatusDto,
    pub categories: Vec<EvidenceCategoryDto>,
    pub totals: EvidenceClassificationTotalsDto,
    pub generated_at: String,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EvidenceClassificationTotalsDto {
    pub category_count: u64,
    pub candidate_file_count: u64,
    pub total_size: u64,
    pub artifact_count: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EvidenceCategoryDto {
    pub category: String,
    pub display_name: String,
    pub status: AnalysisParseStatusDto,
    pub file_count: u64,
    pub total_size: u64,
    pub artifact_count: u64,
    pub confidence: f32,
    pub sources: Vec<EvidenceSourceDto>,
    pub warnings: Vec<String>,
    pub provenance: Vec<AnalysisProvenanceDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EvidenceSourceDto {
    pub file_id: String,
    pub path: String,
    pub size: u64,
    pub evidence_kind: String,
    pub parser: String,
    pub status: AnalysisParseStatusDto,
    pub artifact_count: u64,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnalysisClassifiedFileDto {
    pub file_id: String,
    pub path: String,
    pub name: String,
    pub size: u64,
    pub file_type: String,
    pub magic_description: String,
    pub provenance: AnalysisProvenanceDto,
}
