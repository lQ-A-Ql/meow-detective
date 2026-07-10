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
mod tests {
    use super::*;

    #[test]
    fn extraction_run_serializes_section_progress_as_camel_case() {
        let dto = AnalysisExtractionRunDto {
            status: AnalysisParseStatusDto::Partial,
            scanned_count: 3,
            artifact_count: 9,
            timeline_event_count: 4,
            sections: vec![AnalysisExtractionSectionRunDto {
                key: "LinuxJournal".to_string(),
                label: "Linux 日志".to_string(),
                status: AnalysisParseStatusDto::Parsed,
                scanned_count: 2,
                artifact_count: 7,
                timeline_event_count: 4,
                warnings: vec!["rotated log truncated".to_string()],
            }],
            generated_at: "2026-07-10T00:00:00Z".to_string(),
            warnings: vec!["overall warning".to_string()],
        };

        let value = serde_json::to_value(dto).unwrap();

        assert_eq!(value["scannedCount"], 3);
        assert_eq!(value["artifactCount"], 9);
        assert_eq!(value["timelineEventCount"], 4);
        assert_eq!(value["sections"][0]["key"], "LinuxJournal");
        assert_eq!(value["sections"][0]["scannedCount"], 2);
        assert_eq!(value["sections"][0]["timelineEventCount"], 4);
        assert!(value.get("scanned_count").is_none());
        assert!(value["sections"][0].get("scanned_count").is_none());
    }
}
