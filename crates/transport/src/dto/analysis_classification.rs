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

/// A single classified file row in the classification board.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClassifiedFileRowDto {
    pub file_id: String,
    pub name: String,
    pub path: String,
    pub size: u64,
    /// Detected magic-byte type (e.g. "PE", "SQLite"); absent for metadata-only rows
    #[serde(skip_serializing_if = "Option::is_none")]
    pub magic_type: Option<String>,
    /// "magic" (header bytes read) or "metadata" (extension/path inference)
    pub classification_source: String,
}

/// A scenario-level bucket inside a classification group (e.g. "Word 文档").
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClassificationSubcategoryDto {
    pub name: String,
    pub file_count: u64,
    pub total_size: u64,
    pub files: Vec<ClassifiedFileRowDto>,
    /// True when more files exist than the sample rows returned
    pub truncated: bool,
}

/// A magic-family group in the classification board (e.g. "文档").
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClassificationGroupDto {
    pub category: String,
    pub display_name: String,
    pub file_count: u64,
    pub total_size: u64,
    pub subcategories: Vec<ClassificationSubcategoryDto>,
}

/// Two-level file classification board: magic families with scenario buckets.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileClassificationBoardDto {
    pub status: AnalysisParseStatusDto,
    pub generated_at: String,
    pub total_files: u64,
    pub total_size: u64,
    /// Files classified from actual header bytes
    pub magic_classified_count: u64,
    /// Files classified from extension/path inference only
    pub metadata_classified_count: u64,
    pub groups: Vec<ClassificationGroupDto>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
}
