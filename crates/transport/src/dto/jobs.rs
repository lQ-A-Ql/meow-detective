use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JobSnapshotDto {
    pub id: String,
    pub name: String,
    pub scope: String,
    pub progress: u32,
    pub status: String,
    pub detail: String,
    #[serde(default)]
    pub warning_count: u32,
    #[serde(default)]
    pub skipped_count: u32,
    #[serde(default)]
    pub failed_count: u32,
    #[serde(default)]
    pub partial: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_partition: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completed_partitions: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_partitions: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub partition_progress: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WarningItemDto {
    pub id: String,
    pub title: String,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TraceItemDto {
    pub id: String,
    pub ts: String,
    pub message: String,
}

#[cfg(test)]
mod tests {
    use super::JobSnapshotDto;

    #[test]
    fn job_snapshot_defaults_stage4_counts() {
        let json = r#"{
            "id": "job-1",
            "name": "Import",
            "scope": "Case ingest",
            "progress": 100,
            "status": "completed",
            "detail": "done"
        }"#;

        let snapshot: JobSnapshotDto = serde_json::from_str(json).expect("deserialize");

        assert_eq!(snapshot.warning_count, 0);
        assert_eq!(snapshot.skipped_count, 0);
        assert_eq!(snapshot.failed_count, 0);
        assert!(!snapshot.partial);
    }

    #[test]
    fn job_snapshot_serializes_stage4_counts_as_camel_case() {
        let snapshot = JobSnapshotDto {
            id: "job-1".to_string(),
            name: "Import".to_string(),
            scope: "Case ingest".to_string(),
            progress: 100,
            status: "completed".to_string(),
            detail: "partial".to_string(),
            warning_count: 2,
            skipped_count: 1,
            failed_count: 0,
            partial: true,
            current_partition: None,
            completed_partitions: None,
            total_partitions: None,
            partition_progress: None,
        };

        let value = serde_json::to_value(snapshot).expect("serialize");

        assert_eq!(value["warningCount"], 2);
        assert_eq!(value["skippedCount"], 1);
        assert_eq!(value["failedCount"], 0);
        assert_eq!(value["partial"], true);
    }
}
