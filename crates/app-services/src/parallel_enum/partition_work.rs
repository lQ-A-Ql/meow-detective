use super::batch_sink::{clear_staging_file_entries, enumerate_fs_to_staging, EnumerationStats};
use super::ntfs::enumerate_ntfs_mft_to_staging;
use crate::staging;
use evidence_core::FileSystemReader;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

/// Work required to enumerate one partition.
pub struct PartitionWork {
    pub index: usize,
    pub name: String,
    pub fs_kind: String,
    pub fs: Box<dyn FileSystemReader + Send>,
    pub source_path: PathBuf,
    pub source_kind: String,
    pub volume_offset: u64,
}

impl PartitionWork {
    pub(super) fn uses_e01_reader(&self) -> bool {
        self.source_kind.eq_ignore_ascii_case("e01")
    }

    pub(super) fn uses_local_disk_reader(&self) -> bool {
        self.source_kind.eq_ignore_ascii_case("localdisk")
            || self.source_kind.eq_ignore_ascii_case("local_disk")
    }
}

/// Result from a single partition enumeration.
#[derive(Debug)]
pub struct PartitionResult {
    pub index: usize,
    pub file_count: u64,
    pub dir_count: u64,
    pub total_size: u64,
    pub warnings: Vec<String>,
    pub error: Option<String>,
}

impl PartitionResult {
    pub(super) fn cancelled(index: usize) -> Self {
        Self::failed(index, Vec::new(), "Cancelled".to_string())
    }

    fn failed(index: usize, warnings: Vec<String>, error: String) -> Self {
        Self {
            index,
            file_count: 0,
            dir_count: 0,
            total_size: 0,
            warnings,
            error: Some(error),
        }
    }

    fn completed(index: usize, warnings: Vec<String>, stats: EnumerationStats) -> Self {
        Self {
            index,
            file_count: stats.file_count,
            dir_count: stats.dir_count,
            total_size: stats.total_size,
            warnings,
            error: None,
        }
    }
}

pub(super) fn enumerate_single_partition(
    case_root: &Path,
    data_source_id: &str,
    partition: PartitionWork,
    cancel_token: &AtomicBool,
    progress_cb: Option<&dyn Fn(u64, u64)>,
) -> PartitionResult {
    let index = partition.index;
    let conn = match staging::open_partition_staging(case_root, data_source_id, index) {
        Ok(conn) => conn,
        Err(error) => {
            return PartitionResult::failed(
                index,
                Vec::new(),
                format!("Failed to open staging DB: {error}"),
            );
        }
    };
    if let Some(result) = completed_staging_result(&conn, index) {
        return result;
    }

    let _ = staging::set_staging_meta(&conn, "status", "running");
    let mut warnings = Vec::new();
    let result = enumerate_partition(
        &conn,
        &partition,
        data_source_id,
        cancel_token,
        progress_cb,
        &mut warnings,
    );
    finish_partition(&conn, index, warnings, result)
}

fn completed_staging_result(conn: &rusqlite::Connection, index: usize) -> Option<PartitionResult> {
    let status = staging::get_staging_meta(conn, "status").ok().flatten()?;
    if status != "done" {
        return None;
    }
    Some(PartitionResult {
        index,
        file_count: staging::staging_db_row_count(conn).unwrap_or(0),
        dir_count: 0,
        total_size: 0,
        warnings: Vec::new(),
        error: None,
    })
}

fn enumerate_partition(
    conn: &rusqlite::Connection,
    partition: &PartitionWork,
    data_source_id: &str,
    cancel_token: &AtomicBool,
    progress_cb: Option<&dyn Fn(u64, u64)>,
    warnings: &mut Vec<String>,
) -> Result<EnumerationStats, String> {
    if !partition.fs_kind.eq_ignore_ascii_case("ntfs") {
        return enumerate_fs_to_staging(
            conn,
            &*partition.fs,
            data_source_id,
            partition.index,
            cancel_token,
            progress_cb,
        );
    }

    match enumerate_ntfs_mft_to_staging(conn, partition, data_source_id, cancel_token, progress_cb)
    {
        Ok(stats) => Ok(stats),
        Err(error) => {
            tracing::warn!(
                "MFT fast path failed for partition {}: {}; falling back to recursive enum",
                partition.index,
                error
            );
            let _ = clear_staging_file_entries(conn);
            let message = error.to_string();
            let _ = staging::set_staging_meta(conn, "mft_fallback_warning", &message);
            warnings.push(format!("MFT fast path fallback: {message}"));
            enumerate_fs_to_staging(
                conn,
                &*partition.fs,
                data_source_id,
                partition.index,
                cancel_token,
                progress_cb,
            )
        }
    }
}

fn finish_partition(
    conn: &rusqlite::Connection,
    index: usize,
    warnings: Vec<String>,
    result: Result<EnumerationStats, String>,
) -> PartitionResult {
    match result {
        Ok(stats) => {
            let _ = staging::set_staging_meta(conn, "status", "done");
            let _ = staging::set_staging_meta(conn, "file_count", &stats.file_count.to_string());
            let _ = staging::set_staging_meta(conn, "dir_count", &stats.dir_count.to_string());
            PartitionResult::completed(index, warnings, stats)
        }
        Err(error) => {
            let _ = staging::set_staging_meta(conn, "status", "failed");
            let _ = staging::set_staging_meta(conn, "error", &error);
            PartitionResult::failed(index, warnings, error)
        }
    }
}
