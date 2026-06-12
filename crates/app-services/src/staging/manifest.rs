use super::db_paths::manifest_path;
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ImportPhase {
    Enumerating,
    Merging,
    PostProcessing,
    Completed,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum PartitionStatus {
    Pending,
    Running,
    Done,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PartitionEntry {
    pub index: usize,
    pub name: String,
    pub fs_kind: String,
    pub staging_db: String,
    pub status: PartitionStatus,
    pub file_count: u64,
    pub dir_count: u64,
    pub total_size: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StagingManifest {
    pub data_source_id: String,
    pub source_path: String,
    pub source_kind: String,
    pub created_at: String,
    pub phase: ImportPhase,
    pub partitions: Vec<PartitionEntry>,
}

impl StagingManifest {
    /// Create a new manifest for a data source import.
    pub fn create(data_source_id: &str, source_path: &str, source_kind: &str) -> Self {
        Self {
            data_source_id: data_source_id.to_string(),
            source_path: source_path.to_string(),
            source_kind: source_kind.to_string(),
            created_at: chrono::Utc::now().to_rfc3339(),
            phase: ImportPhase::Enumerating,
            partitions: Vec::new(),
        }
    }

    /// Load an existing manifest from disk, if it exists.
    pub fn load(case_root: &Path, data_source_id: &str) -> Option<Self> {
        let path = manifest_path(case_root, data_source_id);
        if !path.exists() {
            return None;
        }
        let data = std::fs::read_to_string(&path).ok()?;
        serde_json::from_str(&data).ok()
    }

    /// Save manifest to disk atomically (write .tmp then rename).
    pub fn save(&self, case_root: &Path) -> Result<(), String> {
        let path = manifest_path(case_root, &self.data_source_id);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let json = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        let tmp_path = path.with_extension("json.tmp");
        std::fs::write(&tmp_path, &json).map_err(|e| e.to_string())?;
        std::fs::rename(&tmp_path, &path).map_err(|e| e.to_string())?;
        Ok(())
    }

    /// Get partitions that need to be (re-)enumerated.
    pub fn pending_partitions(&self) -> Vec<&PartitionEntry> {
        self.partitions
            .iter()
            .filter(|p| p.status == PartitionStatus::Pending || p.status == PartitionStatus::Failed)
            .collect()
    }

    /// Check if all partitions are done.
    pub fn all_partitions_done(&self) -> bool {
        !self.partitions.is_empty()
            && self
                .partitions
                .iter()
                .all(|p| p.status == PartitionStatus::Done)
    }
}
