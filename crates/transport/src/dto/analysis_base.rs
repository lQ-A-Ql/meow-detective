use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum AnalysisParseStatusDto {
    Parsed,
    Partial,
    NotParsed,
    Unavailable,
    CandidateFound,
    NotFound,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AnalysisProvenanceDto {
    pub data_source_id: String,
    pub artifact_path: String,
    pub parser: String,
    pub parsed_at: String,
    pub status: AnalysisParseStatusDto,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AnalysisFieldProvenanceDto {
    pub field: String,
    pub value_name: String,
    pub key_path: String,
    pub hive_path: String,
    pub parser: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnalysisExtractionSectionRunDto {
    pub key: String,
    pub label: String,
    pub status: AnalysisParseStatusDto,
    pub scanned_count: u64,
    pub artifact_count: u64,
    pub timeline_event_count: u64,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnalysisExtractionRunDto {
    pub status: AnalysisParseStatusDto,
    pub scanned_count: u64,
    pub artifact_count: u64,
    pub timeline_event_count: u64,
    pub sections: Vec<AnalysisExtractionSectionRunDto>,
    pub generated_at: String,
    pub warnings: Vec<String>,
}

#[cfg(test)]
#[path = "../../tests/unit/dto/analysis_base.rs"]
mod tests;
