use super::error::StagingError;
use infrastructure::constants::{MANIFEST_FILE_NAME, STAGING_DIR_NAME};
use persistence_sqlite::repositories::staging_repo::StagingRepo;
use persistence_sqlite::DbResult;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

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

    pub fn load(case_root: &Path, data_source_id: &str) -> Option<Self> {
        let path = manifest_path(case_root, data_source_id);
        if !path.exists() {
            return None;
        }
        let data = std::fs::read_to_string(path).ok()?;
        serde_json::from_str(&data).ok()
    }

    pub fn save(&self, case_root: &Path) -> Result<(), StagingError> {
        let path = manifest_path(case_root, &self.data_source_id);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_string_pretty(self)?;
        let tmp_path = path.with_extension("json.tmp");
        std::fs::write(&tmp_path, json)?;
        std::fs::rename(tmp_path, path)?;
        Ok(())
    }

    pub fn pending_partitions(&self) -> Vec<&PartitionEntry> {
        self.partitions
            .iter()
            .filter(|partition| {
                matches!(
                    partition.status,
                    PartitionStatus::Pending | PartitionStatus::Failed
                )
            })
            .collect()
    }

    pub fn all_partitions_done(&self) -> bool {
        !self.partitions.is_empty()
            && self
                .partitions
                .iter()
                .all(|partition| partition.status == PartitionStatus::Done)
    }
}

pub fn staging_dir(case_root: &Path, data_source_id: &str) -> PathBuf {
    case_root.join(STAGING_DIR_NAME).join(data_source_id)
}

fn manifest_path(case_root: &Path, data_source_id: &str) -> PathBuf {
    staging_dir(case_root, data_source_id).join(MANIFEST_FILE_NAME)
}

pub fn staging_db_path(case_root: &Path, data_source_id: &str, partition_index: usize) -> PathBuf {
    enum_staging_db_path(case_root, data_source_id, partition_index)
}

pub fn enum_staging_db_path(
    case_root: &Path,
    data_source_id: &str,
    partition_index: usize,
) -> PathBuf {
    staging_dir(case_root, data_source_id).join(format!("enum_partition_{partition_index}.db"))
}

fn legacy_partition_staging_db_path(
    case_root: &Path,
    data_source_id: &str,
    partition_index: usize,
) -> PathBuf {
    staging_dir(case_root, data_source_id).join(format!("partition_{partition_index}.db"))
}

pub(super) fn existing_enum_staging_db_path(
    case_root: &Path,
    data_source_id: &str,
    partition_index: usize,
) -> PathBuf {
    let current = enum_staging_db_path(case_root, data_source_id, partition_index);
    if current.exists() {
        return current;
    }
    let legacy = legacy_partition_staging_db_path(case_root, data_source_id, partition_index);
    if legacy.exists() {
        legacy
    } else {
        current
    }
}

pub fn analysis_staging_db_path(
    case_root: &Path,
    data_source_id: &str,
    worker_id: usize,
) -> PathBuf {
    staging_dir(case_root, data_source_id).join(format!("analysis_worker_{worker_id}.db"))
}

pub fn open_partition_staging(
    case_root: &Path,
    data_source_id: &str,
    partition_index: usize,
) -> DbResult<Connection> {
    open_enum_staging(case_root, data_source_id, partition_index)
}

pub fn open_enum_staging(
    case_root: &Path,
    data_source_id: &str,
    partition_index: usize,
) -> DbResult<Connection> {
    StagingRepo::open_partition_staging_conn(case_root, data_source_id, partition_index)
}

pub fn open_analysis_staging(
    case_root: &Path,
    data_source_id: &str,
    worker_id: usize,
) -> DbResult<Connection> {
    StagingRepo::open_analysis_staging_conn(case_root, data_source_id, worker_id)
}
