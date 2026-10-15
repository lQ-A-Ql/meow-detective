use super::batch_sink::EnumerationStats;
use super::ntfs_plan::PartitionEnumStrategy;
use super::partition_enumeration::enumerate_partition;
use crate::import_pipeline::{profile::PhaseRecorder, ImportProfiler};
use crate::staging;
use evidence_core::SharedReaderFactory;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use transport::dto::import_benchmark::{
    ImportBenchmarkPhaseDto as Phase, ImportBenchmarkStateDto as State,
};

/// Work required to enumerate one partition.
pub struct PartitionWork {
    pub index: usize,
    pub name: String,
    pub fs_kind: String,
    pub strategy: PartitionEnumStrategy,
    pub source_path: PathBuf,
    pub source_kind: domain::DataSourceKind,
    pub volume_offset: u64,
    pub reader_factory: Option<SharedReaderFactory>,
    /// Reader budget assigned by the import coordinator.
    pub mft_workers: usize,
}

impl PartitionWork {
    pub(super) fn uses_e01_reader(&self) -> bool {
        self.source_kind == domain::DataSourceKind::E01
    }

    pub(super) fn uses_local_disk_reader(&self) -> bool {
        self.source_kind == domain::DataSourceKind::LocalDisk
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
    enumerate_single_partition_profiled(
        case_root,
        data_source_id,
        partition,
        cancel_token,
        progress_cb,
        None,
    )
}

pub(super) fn enumerate_single_partition_profiled(
    case_root: &Path,
    data_source_id: &str,
    partition: PartitionWork,
    cancel_token: &AtomicBool,
    progress_cb: Option<&dyn Fn(u64, u64)>,
    profiler: Option<ImportProfiler>,
) -> PartitionResult {
    let index = partition.index;
    let mut profile = PhaseRecorder::new(profiler, Some(index));
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
    let started = profile.start();
    let result = enumerate_partition(
        &conn,
        &partition,
        data_source_id,
        cancel_token,
        progress_cb,
        &mut warnings,
        &mut profile,
    );
    let result = finish_partition(
        &conn,
        index,
        warnings,
        result.map_err(|error| error.to_string()),
    );
    profile.record(
        Phase::Enumeration,
        started,
        if result.error.is_none() {
            State::Completed
        } else {
            State::Failed
        },
    );
    if result.error.is_some() && cancel_token.load(std::sync::atomic::Ordering::Relaxed) {
        profile.cancel(Phase::Enumeration);
    }
    result
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

fn finish_partition(
    conn: &rusqlite::Connection,
    index: usize,
    warnings: Vec<String>,
    result: Result<EnumerationStats, String>,
) -> PartitionResult {
    match result {
        Ok(stats) => {
            match persistence_sqlite::repositories::staging_repo::StagingRepo::complete_enum(
                conn,
                stats.file_count,
                stats.dir_count,
            ) {
                Ok(()) => PartitionResult::completed(index, warnings, stats),
                Err(error) => failed_partition(
                    conn,
                    index,
                    warnings,
                    format!("Failed to complete partition staging: {error}"),
                ),
            }
        }
        Err(error) => failed_partition(conn, index, warnings, error),
    }
}

fn failed_partition(
    conn: &rusqlite::Connection,
    index: usize,
    warnings: Vec<String>,
    error: String,
) -> PartitionResult {
    let _ = staging::set_staging_meta(conn, "status", "failed");
    let _ = staging::set_staging_meta(conn, "error", &error);
    PartitionResult::failed(index, warnings, error)
}
