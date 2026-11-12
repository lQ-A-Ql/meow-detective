use super::error::StagingError;
use super::partition_root::{merge_partition_into_main, PartitionMergeStats};
use super::schema::{
    analysis_staging_db_path, enum_staging_db_path, open_analysis_staging, open_partition_staging,
    PartitionStatus, StagingManifest,
};
use super::writer::{get_staging_meta, get_worker_meta, set_staging_meta, set_worker_meta};
use persistence_source::repositories::staging_repo::StagingRepo;
use rusqlite::Connection;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

const INDEX_DOC_MERGE_PAGE_SIZE: i64 = 50;

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct StagingMergeStats {
    pub staging_rows: u64,
    pub merged_rows: u64,
    pub ignored_rows: u64,
}

impl StagingMergeStats {
    fn add(&mut self, other: PartitionMergeStats) {
        self.staging_rows += other.staging_rows;
        self.merged_rows += other.merged_rows;
        self.ignored_rows += other.ignored_rows;
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct AnalysisMergeStats {
    pub artifact_count: u64,
    pub timeline_count: u64,
    pub indexed_count: u64,
}

pub fn merge_all_staging_to_main(
    main_conn: &Connection,
    case_root: &Path,
    data_source_id: &str,
    manifest: &StagingManifest,
    progress_cb: Option<&dyn Fn(usize, usize)>,
) -> Result<u64, StagingError> {
    merge_all_staging_to_main_with_stats(
        main_conn,
        case_root,
        data_source_id,
        manifest,
        progress_cb,
    )
    .map(|stats| stats.merged_rows)
}

pub fn merge_all_staging_to_main_with_stats(
    main_conn: &Connection,
    case_root: &Path,
    data_source_id: &str,
    manifest: &StagingManifest,
    progress_cb: Option<&dyn Fn(usize, usize)>,
) -> Result<StagingMergeStats, StagingError> {
    merge_all_staging_to_main_with_cancel(
        main_conn,
        case_root,
        data_source_id,
        manifest,
        progress_cb,
        None,
    )
}

pub fn merge_all_staging_to_main_with_cancel(
    main_conn: &Connection,
    case_root: &Path,
    data_source_id: &str,
    manifest: &StagingManifest,
    progress_cb: Option<&dyn Fn(usize, usize)>,
    cancel_token: Option<&AtomicBool>,
) -> Result<StagingMergeStats, StagingError> {
    let mut stats = StagingMergeStats::default();
    let total = manifest.partitions.len();

    for (position, partition) in manifest.partitions.iter().enumerate() {
        if cancel_token.is_some_and(|token| token.load(Ordering::Relaxed)) {
            return Err(StagingError::Cancelled);
        }
        if partition.status != PartitionStatus::Done {
            continue;
        }
        if !enum_staging_db_path(case_root, data_source_id, partition.index).exists() {
            continue;
        }
        if partition_already_merged(case_root, data_source_id, partition.index)? {
            report_progress(progress_cb, position + 1, total);
            continue;
        }

        let started = Instant::now();
        let staging_conn = open_partition_staging(case_root, data_source_id, partition.index)
            .map_err(|error| {
                StagingError::Other(format!("Open staging DB {}: {error}", partition.index))
            })?;
        let partition_stats =
            merge_partition_into_main(main_conn, &staging_conn, data_source_id, partition)?;
        log_enum_merge(partition.index, partition_stats, started.elapsed());
        set_staging_meta(&staging_conn, "merged", "true").map_err(|error| {
            StagingError::Other(format!(
                "Mark staging DB {} merged: {error}",
                partition.index
            ))
        })?;
        stats.add(partition_stats);
        report_progress(progress_cb, position + 1, total);
    }

    Ok(stats)
}

pub fn merge_all_staging_to_main_with_catalog(
    main_conn: &Connection,
    case_root: &Path,
    data_source_id: &str,
    manifest: &StagingManifest,
    source: &source_kernel::storage_runtime::ReadWriteSourceStorageHandle,
    progress_cb: Option<&dyn Fn(usize, usize)>,
    cancel_token: Option<&AtomicBool>,
) -> Result<StagingMergeStats, StagingError> {
    persistence_source::detail::require_source_store(main_conn)
        .map_err(|error| StagingError::Other(format!("Open native source storage: {error}")))?;
    let mut stats = StagingMergeStats::default();
    let mut merged_partitions = Vec::new();
    let total = manifest.partitions.len();
    let transaction = main_conn.unchecked_transaction().map_err(|error| {
        StagingError::Other(format!("Begin catalog merge transaction: {error}"))
    })?;

    for (position, partition) in manifest.partitions.iter().enumerate() {
        if cancel_token.is_some_and(|token| token.load(Ordering::Relaxed)) {
            let _ = transaction.rollback();
            return Err(StagingError::Cancelled);
        }
        if partition.status != PartitionStatus::Done
            || !enum_staging_db_path(case_root, data_source_id, partition.index).exists()
            || partition_already_merged(case_root, data_source_id, partition.index)?
        {
            report_progress(progress_cb, position + 1, total);
            continue;
        }

        let started = Instant::now();
        let staging_conn = open_partition_staging(case_root, data_source_id, partition.index)
            .map_err(|error| {
                StagingError::Other(format!("Open staging DB {}: {error}", partition.index))
            })?;
        let partition_stats = super::partition_root::merge_partition_into_main_in_transaction(
            &transaction,
            &staging_conn,
            data_source_id,
            partition,
        )?;
        log_enum_merge(partition.index, partition_stats, started.elapsed());
        stats.add(partition_stats);
        merged_partitions.push(partition.index);
        report_progress(progress_cb, position + 1, total);
    }

    crate::import_pipeline::mirror_catalog_records_from_connection(
        &transaction,
        source,
        data_source_id,
    )
    .map_err(|error| {
        StagingError::Other(format!(
            "Mirror Fjall Catalog before SQLite commit: {error}"
        ))
    })?;
    source
        .persist(persistence_source::fjall::PersistMode::SyncAll)
        .map_err(|error| StagingError::Other(format!("Persist Fjall Catalog: {error}")))?;
    crate::import_pipeline::mark_catalog_projection_seeded(source, data_source_id)
        .map_err(|error| StagingError::Other(format!("Mark Fjall Catalog seeded: {error}")))?;
    transaction.commit().map_err(|error| {
        StagingError::Other(format!("Commit SQLite Catalog projection: {error}"))
    })?;

    for partition_index in merged_partitions {
        let staging_conn = open_partition_staging(case_root, data_source_id, partition_index)
            .map_err(|error| {
                StagingError::Other(format!("Open staging DB {partition_index}: {error}"))
            })?;
        set_staging_meta(&staging_conn, "merged", "true").map_err(|error| {
            StagingError::Other(format!("Mark staging DB {partition_index} merged: {error}"))
        })?;
    }

    Ok(stats)
}

pub fn merge_analysis_staging_to_main(
    main_conn: &Connection,
    case_root: &Path,
    data_source_id: &str,
    worker_ids: &[usize],
    case_id: &str,
    index_dir: &Path,
    progress_cb: Option<&dyn Fn(usize, usize)>,
) -> Result<AnalysisMergeStats, StagingError> {
    let mut stats = AnalysisMergeStats::default();
    let total = worker_ids.len().max(1);

    for (position, worker_id) in worker_ids.iter().copied().enumerate() {
        if !analysis_staging_db_path(case_root, data_source_id, worker_id).exists() {
            report_progress(progress_cb, position + 1, total);
            continue;
        }
        if analysis_worker_already_merged(case_root, data_source_id, worker_id)? {
            report_progress(progress_cb, position + 1, total);
            continue;
        }

        let worker_conn = open_analysis_worker(case_root, data_source_id, worker_id)?;
        let worker_stats = merge_analysis_worker(main_conn, &worker_conn, case_id, data_source_id)?;
        stats.artifact_count += worker_stats.artifact_count;
        stats.timeline_count += worker_stats.timeline_count;
        stats.indexed_count += merge_analysis_index(&worker_conn, index_dir, worker_id)?;
        set_worker_meta(&worker_conn, "merged", "true").map_err(|error| {
            StagingError::Other(format!(
                "Mark analysis staging DB {worker_id} merged: {error}"
            ))
        })?;
        report_progress(progress_cb, position + 1, total);
    }

    Ok(stats)
}

#[allow(clippy::too_many_arguments)]
pub fn merge_analysis_staging_to_main_with_fjall(
    main_conn: &Connection,
    case_root: &Path,
    data_source_id: &str,
    worker_ids: &[usize],
    case_id: &str,
    index_dir: &Path,
    source: &source_kernel::storage_runtime::ReadWriteSourceStorageHandle,
    progress_cb: Option<&dyn Fn(usize, usize)>,
) -> Result<AnalysisMergeStats, StagingError> {
    persistence_source::detail::require_source_store(main_conn)
        .map_err(|error| StagingError::Other(format!("Open native source storage: {error}")))?;
    let mut stats = AnalysisMergeStats::default();
    let mut merged_workers = Vec::new();
    let total = worker_ids.len().max(1);
    let transaction = main_conn.unchecked_transaction().map_err(|error| {
        StagingError::Other(format!("Begin Analysis merge transaction: {error}"))
    })?;

    for (position, worker_id) in worker_ids.iter().copied().enumerate() {
        if analysis_worker_already_merged(case_root, data_source_id, worker_id)? {
            report_progress(progress_cb, position + 1, total);
            continue;
        }
        let worker_conn = open_analysis_worker(case_root, data_source_id, worker_id)?;
        let alias = format!("analysis_stage_{worker_id}");
        let (artifact_count, timeline_count) =
            StagingRepo::merge_analysis_staging_to_main_in_transaction_with_alias(
                &transaction,
                &worker_conn,
                case_id,
                data_source_id,
                &alias,
            )
            .map_err(|error| StagingError::Other(format!("Merge analysis staging: {error}")))?;
        stats.artifact_count += artifact_count;
        stats.timeline_count += timeline_count;
        merged_workers.push((worker_id, worker_conn, alias));
        report_progress(progress_cb, position + 1, total);
    }

    crate::import_pipeline::mirror_analysis_records_before_sqlite_commit(
        &transaction,
        source,
        data_source_id,
    )
    .map_err(|error| {
        StagingError::Other(format!(
            "Mirror Fjall Analysis before SQLite commit: {error}"
        ))
    })?;
    source
        .persist(persistence_source::fjall::PersistMode::SyncAll)
        .map_err(|error| StagingError::Other(format!("Persist Fjall Analysis: {error}")))?;
    transaction.commit().map_err(|error| {
        StagingError::Other(format!("Commit SQLite Analysis projection: {error}"))
    })?;

    for (worker_id, worker_conn, alias) in merged_workers {
        let _ = main_conn.execute_batch(&format!("DETACH DATABASE {alias}"));
        stats.indexed_count += merge_analysis_index(&worker_conn, index_dir, worker_id)?;
        set_worker_meta(&worker_conn, "merged", "true").map_err(|error| {
            StagingError::Other(format!(
                "Mark analysis staging DB {worker_id} merged: {error}"
            ))
        })?;
    }
    Ok(stats)
}

fn partition_already_merged(
    case_root: &Path,
    data_source_id: &str,
    partition_index: usize,
) -> Result<bool, StagingError> {
    let conn =
        open_partition_staging(case_root, data_source_id, partition_index).map_err(|error| {
            StagingError::Other(format!("Open staging DB {partition_index}: {error}"))
        })?;
    get_staging_meta(&conn, "merged")
        .map(|value| value.as_deref() == Some("true"))
        .map_err(|error| {
            StagingError::Other(format!(
                "Read staging merge state {partition_index}: {error}"
            ))
        })
}

fn analysis_worker_already_merged(
    case_root: &Path,
    data_source_id: &str,
    worker_id: usize,
) -> Result<bool, StagingError> {
    let conn = open_analysis_worker(case_root, data_source_id, worker_id)?;
    get_worker_meta(&conn, "merged")
        .map(|value| value.as_deref() == Some("true"))
        .map_err(|error| {
            StagingError::Other(format!("Read analysis merge state {worker_id}: {error}"))
        })
}

fn open_analysis_worker(
    case_root: &Path,
    data_source_id: &str,
    worker_id: usize,
) -> Result<Connection, StagingError> {
    open_analysis_staging(case_root, data_source_id, worker_id).map_err(|error| {
        StagingError::Other(format!("Open analysis staging DB {worker_id}: {error}"))
    })
}

fn merge_analysis_worker(
    main_conn: &Connection,
    staging_conn: &Connection,
    case_id: &str,
    data_source_id: &str,
) -> Result<AnalysisMergeStats, StagingError> {
    let started = Instant::now();
    let (artifact_count, timeline_count) = StagingRepo::merge_analysis_staging_to_main(
        main_conn,
        staging_conn,
        case_id,
        data_source_id,
    )
    .map_err(|error| StagingError::Other(format!("Merge analysis staging: {error}")))?;
    log_analysis_db_merge(artifact_count, timeline_count, started.elapsed());
    Ok(AnalysisMergeStats {
        artifact_count,
        timeline_count,
        indexed_count: 0,
    })
}

fn merge_analysis_index(
    staging_conn: &Connection,
    index_dir: &Path,
    worker_id: usize,
) -> Result<u64, StagingError> {
    let started = Instant::now();
    let indexed = merge_index_pages(staging_conn, index_dir)?;
    tracing::info!(
        "Analysis index merge profile: worker={} indexed={} elapsedMs={} rowsPerSec={}",
        worker_id,
        indexed,
        started.elapsed().as_millis(),
        rows_per_sec(indexed, started.elapsed())
    );
    Ok(indexed)
}

fn merge_index_pages(staging_conn: &Connection, index_dir: &Path) -> Result<u64, StagingError> {
    let index = match search::SearchIndex::open(index_dir) {
        Ok(index) => index,
        Err(_) => search::SearchIndex::create(index_dir)
            .map_err(|error| StagingError::Other(error.to_string()))?,
    };
    let mut indexed_total = 0u64;
    let mut offset = 0i64;

    loop {
        let rows = StagingRepo::read_analysis_index_docs_page(
            staging_conn,
            INDEX_DOC_MERGE_PAGE_SIZE,
            offset,
        )
        .map_err(|error| StagingError::Other(error.to_string()))?;
        if rows.is_empty() {
            break;
        }
        let row_count = rows.len();
        let (texts, paths) = map_index_rows(rows);
        indexed_total += index
            .index_documents(&texts, &paths)
            .map_err(|error| StagingError::Other(error.to_string()))?;
        if row_count < INDEX_DOC_MERGE_PAGE_SIZE as usize {
            break;
        }
        offset += INDEX_DOC_MERGE_PAGE_SIZE;
    }

    Ok(indexed_total)
}

type IndexRow = (String, String, String, String);

fn map_index_rows(rows: Vec<IndexRow>) -> (Vec<search::ExtractedText>, Vec<(String, String)>) {
    let mut texts = Vec::with_capacity(rows.len());
    let mut paths = Vec::with_capacity(rows.len());
    for (file_id, path, text, language) in rows {
        texts.push(search::ExtractedText {
            file_id: file_id.clone(),
            content: text,
            encoding: language,
            extractable: true,
            byte_count: 0,
        });
        paths.push((file_id, path));
    }
    (texts, paths)
}

fn report_progress(progress_cb: Option<&dyn Fn(usize, usize)>, completed: usize, total: usize) {
    if let Some(callback) = progress_cb {
        callback(completed, total);
    }
}

fn log_enum_merge(index: usize, stats: PartitionMergeStats, elapsed: Duration) {
    tracing::info!(
        "Enum staging merge profile: partition={} stagingRows={} mergedRows={} ignoredRows={} elapsedMs={} rowsPerSec={}",
        index,
        stats.staging_rows,
        stats.merged_rows,
        stats.ignored_rows,
        elapsed.as_millis(),
        rows_per_sec(stats.merged_rows, elapsed)
    );
}

fn log_analysis_db_merge(artifacts: u64, timeline: u64, elapsed: Duration) {
    tracing::info!(
        "Analysis DB merge profile: artifacts={} timeline={} elapsedMs={} rowsPerSec={}",
        artifacts,
        timeline,
        elapsed.as_millis(),
        rows_per_sec(artifacts + timeline, elapsed)
    );
}

fn rows_per_sec(rows: u64, duration: Duration) -> u64 {
    let seconds = duration.as_secs_f64();
    if seconds <= 0.0 {
        rows
    } else {
        (rows as f64 / seconds).round() as u64
    }
}
