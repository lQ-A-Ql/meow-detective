use infrastructure::constants::{MANIFEST_FILE_NAME, STAGING_DIR_NAME};
use std::path::{Path, PathBuf};

// ---------------------------------------------------------------------------
// Staging DB paths
// ---------------------------------------------------------------------------

/// Get the staging directory for a data source.
pub fn staging_dir(case_root: &Path, data_source_id: &str) -> PathBuf {
    case_root.join(STAGING_DIR_NAME).join(data_source_id)
}

/// Get the manifest file path.
pub(super) fn manifest_path(case_root: &Path, data_source_id: &str) -> PathBuf {
    staging_dir(case_root, data_source_id).join(MANIFEST_FILE_NAME)
}

/// Get the staging DB path for a partition.
pub fn staging_db_path(case_root: &Path, data_source_id: &str, partition_index: usize) -> PathBuf {
    enum_staging_db_path(case_root, data_source_id, partition_index)
}

/// Get the enumeration staging DB path for a partition.
pub fn enum_staging_db_path(
    case_root: &Path,
    data_source_id: &str,
    partition_index: usize,
) -> PathBuf {
    staging_dir(case_root, data_source_id).join(format!("enum_partition_{}.db", partition_index))
}

fn legacy_partition_staging_db_path(
    case_root: &Path,
    data_source_id: &str,
    partition_index: usize,
) -> PathBuf {
    staging_dir(case_root, data_source_id).join(format!("partition_{}.db", partition_index))
}

/// Resolve existing enum staging DBs created by older builds.
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

/// Get the analysis staging DB path for an analysis worker.
pub fn analysis_staging_db_path(
    case_root: &Path,
    data_source_id: &str,
    worker_id: usize,
) -> PathBuf {
    staging_dir(case_root, data_source_id).join(format!("analysis_worker_{}.db", worker_id))
}
