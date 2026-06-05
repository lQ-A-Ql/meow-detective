use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CaseSummaryDto {
    pub id: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub number: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub examiner: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CaseMetricsDto {
    pub data_source_count: u64,
    pub indexed_file_count: u64,
    pub timeline_event_count: u64,
    pub artifact_count: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecentObjectDto {
    pub id: String,
    pub title: String,
    pub detail: String,
    pub time: String,
    pub kind: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DataSourceSummaryDto {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub source_path: String,
    pub imported_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_count: Option<u64>,
    #[serde(rename = "sourceHash", skip_serializing_if = "Option::is_none")]
    pub source_hash: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hash_status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub canonical_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub evidence_size: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reader_kind: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provenance_status: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub partitions: Vec<DataSourcePartitionDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DataSourcePartitionDto {
    pub index: u32,
    pub name: String,
    pub kind_label: String,
    pub status: String,
    pub offset: u64,
    pub length: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub type_guid: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filesystem: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unlock_hint: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecentCaseDto {
    pub case_root: String,
    pub name: String,
    pub opened_at: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn data_source_summary_serializes_provenance_fields_as_frontend_contract() {
        let dto = DataSourceSummaryDto {
            id: "ds-1".to_string(),
            name: "Evidence".to_string(),
            kind: "raw".to_string(),
            source_path: "D:/evidence/disk.raw".to_string(),
            imported_at: "2026-06-04T00:00:00Z".to_string(),
            file_count: Some(42),
            source_hash: Some("a".repeat(64)),
            hash_status: Some("hashed".to_string()),
            canonical_path: Some("D:/canonical/disk.raw".to_string()),
            evidence_size: Some(4096),
            reader_kind: Some("raw".to_string()),
            provenance_status: Some("recorded".to_string()),
            warnings: vec!["metadata warning".to_string()],
            partitions: vec![DataSourcePartitionDto {
                index: 1,
                name: "Basic data".to_string(),
                kind_label: "NTFS".to_string(),
                status: "supported".to_string(),
                offset: 1048576,
                length: 4096,
                type_guid: None,
                filesystem: Some("NTFS".to_string()),
                unlock_hint: None,
            }],
        };

        let value = serde_json::to_value(dto).unwrap();

        assert_eq!(value["sourcePath"], "D:/evidence/disk.raw");
        assert_eq!(value["importedAt"], "2026-06-04T00:00:00Z");
        assert_eq!(value["fileCount"], 42);
        assert_eq!(value["sourceHash"], "a".repeat(64));
        assert_eq!(value["hashStatus"], "hashed");
        assert_eq!(value["canonicalPath"], "D:/canonical/disk.raw");
        assert_eq!(value["evidenceSize"], 4096);
        assert_eq!(value["readerKind"], "raw");
        assert_eq!(value["provenanceStatus"], "recorded");
        assert_eq!(value["warnings"][0], "metadata warning");
        assert_eq!(value["partitions"][0]["kindLabel"], "NTFS");
        assert!(value.get("source_hash_sha256").is_none());
        assert!(value.get("source_hash").is_none());
        assert!(value.get("canonical_source_path").is_none());
        assert!(value.get("canonical_path").is_none());
    }

    #[test]
    fn data_source_summary_skips_missing_optional_provenance_fields() {
        let dto = DataSourceSummaryDto {
            id: "ds-legacy".to_string(),
            name: "Legacy".to_string(),
            kind: "raw".to_string(),
            source_path: "D:/legacy.raw".to_string(),
            imported_at: "2026-06-04T00:00:00Z".to_string(),
            file_count: None,
            source_hash: None,
            hash_status: None,
            canonical_path: None,
            evidence_size: None,
            reader_kind: None,
            provenance_status: None,
            warnings: Vec::new(),
            partitions: Vec::new(),
        };

        let value = serde_json::to_value(dto).unwrap();

        assert!(value.get("fileCount").is_none());
        assert!(value.get("sourceHash").is_none());
        assert!(value.get("hashStatus").is_none());
        assert!(value.get("canonicalPath").is_none());
        assert!(value.get("evidenceSize").is_none());
        assert!(value.get("readerKind").is_none());
        assert!(value.get("provenanceStatus").is_none());
        assert!(value.get("warnings").is_none());
        assert!(value.get("partitions").is_none());
    }
}
