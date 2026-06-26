use super::error::ImportAnalysisError;
use super::options::{AnalysisProgressCallback, ImportAnalysisOptions, ImportAnalysisStats};
use super::progress::{current_rss_mb, rows_per_sec};
use super::worker_runtime::clear_analysis_worker_rows;
use crate::staging;
use persistence_sqlite::DbResult;
use rusqlite::Connection;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::Instant;

const ANALYSIS_LAYOUT_VERSION: &str = "2";

pub(super) fn merge_finished_analysis_staging(
    options: &ImportAnalysisOptions,
    worker_ids: &[usize],
    mut stats: ImportAnalysisStats,
    progress_cb: Option<AnalysisProgressCallback<'_>>,
    analysis_started: Instant,
) -> Result<ImportAnalysisStats, ImportAnalysisError> {
    if let Some(cb) = progress_cb {
        cb(
            84,
            &format!(
                "Analysis workers complete: phase=analysis scheduling=running elapsedMs={} workerBudget={} activeWorkers=0 queuedTasks={} pendingTasks=0 processed={} rowsPerSec={} indexed={} rssMb={}",
                analysis_started.elapsed().as_millis(),
                worker_ids.len().max(1),
                stats.processed_count,
                stats.processed_count,
                rows_per_sec(stats.processed_count, analysis_started.elapsed()),
                stats.indexed_count,
                current_rss_mb()
            ),
        );
        cb(84, "Merging analysis staging DBs...");
    }
    let merge_started = Instant::now();
    let merge_stats = staging::merge_analysis_staging_to_main(
        &persistence_sqlite::open_or_create(&options.db_path)?,
        &options.case_root,
        &options.data_source_id.0,
        worker_ids,
        &options.case_id,
        &options.index_dir,
        Some(&|completed, total| {
            if let Some(cb) = progress_cb {
                let pct = 84 + ((completed as u32 * 10) / total.max(1) as u32);
                cb(
                    pct.min(94),
                    &format!(
                        "Merged analysis staging {}/{}: phase=analysis-merge elapsedMs={} rssMb={}",
                        completed,
                        total,
                        merge_started.elapsed().as_millis(),
                        current_rss_mb()
                    ),
                );
            }
        }),
    )
    .map_err(|e| ImportAnalysisError::Staging(e.to_string()))?;
    stats.artifact_count = merge_stats.artifact_count;
    stats.timeline_count = merge_stats.timeline_count;
    stats.indexed_count = merge_stats.indexed_count;

    Ok(stats)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum AnalysisStartupAction {
    RunWorkers,
    MergeOnly,
    AlreadyMerged,
}

#[derive(Debug, Clone)]
pub(super) struct ExistingAnalysisWorker {
    worker_id: usize,
    status: Option<String>,
    merged: bool,
    worker_count: Option<usize>,
}

pub(super) fn prepare_analysis_staging_startup(
    options: &ImportAnalysisOptions,
    worker_ids: &[usize],
    worker_count: usize,
    progress_cb: Option<AnalysisProgressCallback<'_>>,
) -> Result<AnalysisStartupAction, ImportAnalysisError> {
    let existing_ids = discover_analysis_worker_ids(&options.case_root, &options.data_source_id.0)?;
    let existing = load_existing_analysis_workers(options, &existing_ids)?;

    if !existing.is_empty()
        && existing
            .iter()
            .all(|worker| worker.status.as_deref() == Some("done") && worker.merged)
    {
        return Ok(AnalysisStartupAction::AlreadyMerged);
    }

    let expected: HashSet<usize> = worker_ids.iter().copied().collect();
    let by_id: HashMap<usize, ExistingAnalysisWorker> = existing
        .iter()
        .cloned()
        .map(|worker| (worker.worker_id, worker))
        .collect();
    let extra_unmerged_exists = existing
        .iter()
        .any(|worker| !expected.contains(&worker.worker_id) && !worker.merged);
    let expected_all_done = worker_ids.iter().all(|worker_id| {
        by_id.get(worker_id).is_some_and(|worker| {
            worker.status.as_deref() == Some("done")
                && worker
                    .worker_count
                    .is_none_or(|stored_count| stored_count == worker_count)
        })
    });
    let expected_count_mismatch = worker_ids.iter().any(|worker_id| {
        by_id
            .get(worker_id)
            .and_then(|worker| worker.worker_count)
            .is_some_and(|stored_count| stored_count != worker_count)
    });

    if expected_all_done && !extra_unmerged_exists && !expected_count_mismatch {
        return Ok(AnalysisStartupAction::MergeOnly);
    }

    if !existing.is_empty() && (extra_unmerged_exists || expected_count_mismatch) {
        if let Some(cb) = progress_cb {
            cb(
                72,
                &format!(
                    "Analysis staging layout changed; reinitializing unfinished worker DBs: previousWorkers={:?} currentWorkers={:?}",
                    existing_ids, worker_ids
                ),
            );
        }
    }

    let mut stale_extra_ids = Vec::new();
    for worker in &existing {
        if !expected.contains(&worker.worker_id) && !worker.merged {
            stale_extra_ids.push(worker.worker_id);
        }
    }
    for worker_id in stale_extra_ids {
        remove_analysis_worker_db_files(&options.case_root, &options.data_source_id.0, worker_id)
            .map_err(|e| {
            ImportAnalysisError::Other(format!("Remove stale analysis staging {worker_id}: {e}"))
        })?;
    }

    for worker_id in worker_ids {
        let conn = staging::open_analysis_staging(
            &options.case_root,
            &options.data_source_id.0,
            *worker_id,
        )
        .map_err(|e| {
            ImportAnalysisError::Staging(format!("Open analysis staging {worker_id}: {e}"))
        })?;
        clear_analysis_worker_rows(&conn).map_err(|e| {
            ImportAnalysisError::Staging(format!("Clear analysis staging {worker_id}: {e}"))
        })?;
        init_analysis_worker_meta(&conn, worker_count, "pending").map_err(|e| {
            ImportAnalysisError::Staging(format!("Init analysis staging {worker_id}: {e}"))
        })?;
    }

    Ok(AnalysisStartupAction::RunWorkers)
}

pub(super) fn discover_analysis_worker_ids(
    case_root: &Path,
    data_source_id: &str,
) -> Result<Vec<usize>, ImportAnalysisError> {
    let dir = staging::staging_dir(case_root, data_source_id);
    if !dir.exists() {
        return Ok(Vec::new());
    }

    let mut worker_ids = Vec::new();
    for entry in std::fs::read_dir(&dir)
        .map_err(|e| ImportAnalysisError::Other(format!("Read staging dir: {e}")))?
    {
        let entry =
            entry.map_err(|e| ImportAnalysisError::Other(format!("Read staging entry: {e}")))?;
        let file_name = entry.file_name();
        let Some(file_name) = file_name.to_str() else {
            continue;
        };
        let Some(id_part) = file_name
            .strip_prefix("analysis_worker_")
            .and_then(|name| name.strip_suffix(".db"))
        else {
            continue;
        };
        if let Ok(worker_id) = id_part.parse::<usize>() {
            worker_ids.push(worker_id);
        }
    }
    worker_ids.sort_unstable();
    worker_ids.dedup();
    Ok(worker_ids)
}

fn load_existing_analysis_workers(
    options: &ImportAnalysisOptions,
    worker_ids: &[usize],
) -> Result<Vec<ExistingAnalysisWorker>, ImportAnalysisError> {
    let mut workers = Vec::with_capacity(worker_ids.len());
    for worker_id in worker_ids {
        let conn = staging::open_analysis_staging(
            &options.case_root,
            &options.data_source_id.0,
            *worker_id,
        )
        .map_err(|e| {
            ImportAnalysisError::Staging(format!("Open analysis staging {worker_id}: {e}"))
        })?;
        let status = staging::get_worker_meta(&conn, "status").map_err(|e| {
            ImportAnalysisError::Staging(format!("Read analysis staging status {worker_id}: {e}"))
        })?;
        let merged = staging::get_worker_meta(&conn, "merged")
            .map_err(|e| {
                ImportAnalysisError::Staging(format!(
                    "Read analysis staging merge flag {worker_id}: {e}"
                ))
            })?
            .as_deref()
            == Some("true");
        let worker_count = staging::get_worker_meta(&conn, "worker_count")
            .map_err(|e| {
                ImportAnalysisError::Staging(format!(
                    "Read analysis staging worker count {worker_id}: {e}"
                ))
            })?
            .and_then(|value| value.parse::<usize>().ok());
        workers.push(ExistingAnalysisWorker {
            worker_id: *worker_id,
            status,
            merged,
            worker_count,
        });
    }
    Ok(workers)
}

fn init_analysis_worker_meta(conn: &Connection, worker_count: usize, status: &str) -> DbResult<()> {
    staging::set_worker_meta(conn, "status", status)?;
    staging::set_worker_meta(conn, "merged", "false")?;
    staging::set_worker_meta(conn, "worker_count", &worker_count.to_string())?;
    staging::set_worker_meta(conn, "layout_version", ANALYSIS_LAYOUT_VERSION)
}

fn remove_analysis_worker_db_files(
    case_root: &Path,
    data_source_id: &str,
    worker_id: usize,
) -> std::io::Result<()> {
    let path = staging::analysis_staging_db_path(case_root, data_source_id, worker_id);
    for candidate in [
        path.clone(),
        PathBuf::from(format!("{}-wal", path.display())),
        PathBuf::from(format!("{}-shm", path.display())),
    ] {
        match std::fs::remove_file(&candidate) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
    }
    Ok(())
}

pub(super) fn collect_done_worker_stats(
    options: &ImportAnalysisOptions,
    worker_ids: &[usize],
) -> Result<ImportAnalysisStats, ImportAnalysisError> {
    let mut stats = ImportAnalysisStats {
        worker_ids: worker_ids.to_vec(),
        ..ImportAnalysisStats::default()
    };
    for worker_id in worker_ids {
        let db_path = staging::analysis_staging_db_path(
            &options.case_root,
            &options.data_source_id.0,
            *worker_id,
        );
        if !db_path.exists() {
            continue;
        }
        let conn = staging::open_analysis_staging(
            &options.case_root,
            &options.data_source_id.0,
            *worker_id,
        )
        .map_err(|e| {
            ImportAnalysisError::Staging(format!("Open analysis staging {worker_id}: {e}"))
        })?;
        stats.processed_count += worker_meta_u64(&conn, "processed_count")?;
        stats.warning_count = stats
            .warning_count
            .saturating_add(worker_meta_u32(&conn, "warning_count")?);
        stats.skipped_count = stats
            .skipped_count
            .saturating_add(worker_meta_u32(&conn, "skipped_count")?);
        stats.failed_count = stats
            .failed_count
            .saturating_add(worker_meta_u32(&conn, "failed_count")?);
        stats.artifact_count += worker_meta_u64(&conn, "artifact_count")?;
        stats.timeline_count += worker_meta_u64(&conn, "timeline_count")?;
        stats.indexed_count += worker_meta_u64(&conn, "indexed_count")?;
    }
    Ok(stats)
}

fn worker_meta_u64(conn: &Connection, key: &str) -> Result<u64, ImportAnalysisError> {
    Ok(staging::get_worker_meta(conn, key)
        .map_err(|e| ImportAnalysisError::Staging(format!("Read worker meta {key}: {e}")))?
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(0))
}

fn worker_meta_u32(conn: &Connection, key: &str) -> Result<u32, ImportAnalysisError> {
    Ok(worker_meta_u64(conn, key)?.min(u32::MAX as u64) as u32)
}
