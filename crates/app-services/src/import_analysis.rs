//! Post-import analysis worker pool.
//!
//! Workers read file rows from the main DB, write artifacts/timeline/index docs
//! to per-worker temp DBs, then the caller merges those temp DBs with one writer.

use crate::{artifact_service, file_service, staging};
use artifacts_core::VecSink;
use crossbeam_channel::{bounded, Receiver, Sender};
use domain::{DataSourceId, EntryType, FileEntry, FileEntryId};
use persistence_sqlite::repositories::file_repo::FileRepo;
use persistence_sqlite::DbResult;
use rusqlite::{params, Connection};
use search::extract_text;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

const TASKS_PER_WORKER_BOUND: usize = 256;
const FILE_PAGE_SIZE: u64 = 750;
const WORKER_INSERT_BATCH: usize = 200;
const INDEX_DOC_INSERT_BATCH: usize = 25;
const DEFAULT_MEMORY_SOFT_LIMIT_MB: u64 = 4 * 1024;
const DEFAULT_MEMORY_HARD_LIMIT_MB: u64 = 6 * 1024;
const ANALYSIS_LAYOUT_VERSION: &str = "2";

pub type AnalysisProgressCallback<'a> = &'a dyn Fn(u32, &str);

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ImportAnalysisMode {
    #[default]
    MetadataOnly,
    BudgetedContent,
    FullContent,
}

impl ImportAnalysisMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::MetadataOnly => "metadataOnly",
            Self::BudgetedContent => "budgetedContent",
            Self::FullContent => "fullContent",
        }
    }

    pub fn allows_content(self) -> bool {
        !matches!(self, Self::MetadataOnly)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContentBudget {
    pub max_files: u64,
    pub max_bytes_total: u64,
    pub max_bytes_per_file: u64,
    pub allowed_extensions: Vec<String>,
}

impl ContentBudget {
    pub fn disabled() -> Self {
        Self {
            max_files: 0,
            max_bytes_total: 0,
            max_bytes_per_file: 0,
            allowed_extensions: Vec::new(),
        }
    }

    pub fn conservative() -> Self {
        Self {
            max_files: infrastructure::constants::IMPORT_TEXT_INDEX_LIMIT as u64,
            max_bytes_total: 64 * 1024 * 1024,
            max_bytes_per_file: infrastructure::constants::IMPORT_TEXT_INDEX_FILE_LIMIT_BYTES,
            allowed_extensions: vec![
                "txt".to_string(),
                "log".to_string(),
                "csv".to_string(),
                "json".to_string(),
                "xml".to_string(),
                "html".to_string(),
                "htm".to_string(),
                "md".to_string(),
                "pf".to_string(),
                "lnk".to_string(),
                "evtx".to_string(),
            ],
        }
    }

    pub fn full() -> Self {
        Self {
            max_files: 10_000,
            max_bytes_total: 512 * 1024 * 1024,
            max_bytes_per_file: infrastructure::constants::ARTIFACT_FILE_LIMIT_BYTES,
            allowed_extensions: Vec::new(),
        }
    }
}

pub fn default_memory_soft_limit_mb() -> u64 {
    DEFAULT_MEMORY_SOFT_LIMIT_MB
}

pub fn default_memory_hard_limit_mb() -> u64 {
    DEFAULT_MEMORY_HARD_LIMIT_MB
}

pub fn content_budget_for_mode(mode: ImportAnalysisMode) -> ContentBudget {
    match mode {
        ImportAnalysisMode::MetadataOnly => ContentBudget::disabled(),
        ImportAnalysisMode::BudgetedContent => ContentBudget::conservative(),
        ImportAnalysisMode::FullContent => ContentBudget::full(),
    }
}

#[derive(Debug, Clone)]
pub struct ImportAnalysisOptions {
    pub case_root: PathBuf,
    pub db_path: PathBuf,
    pub case_id: String,
    pub data_source_id: DataSourceId,
    pub index_dir: PathBuf,
    pub max_analysis_workers: Option<usize>,
    pub cancel_token: Arc<AtomicBool>,
    pub enable_timeline_projection: bool,
    pub enable_content_extraction: bool,
    pub enable_text_indexing: bool,
    pub analysis_mode: ImportAnalysisMode,
    pub content_budget: ContentBudget,
    pub memory_soft_limit_mb: u64,
    pub memory_hard_limit_mb: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ImportAnalysisStats {
    pub processed_count: u64,
    pub artifact_count: u64,
    pub timeline_count: u64,
    pub indexed_count: u64,
    pub warning_count: u32,
    pub skipped_count: u32,
    pub failed_count: u32,
    pub worker_ids: Vec<usize>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct JobOutcomeCounts {
    pub warning_count: u32,
    pub skipped_count: u32,
    pub failed_count: u32,
}

impl JobOutcomeCounts {
    pub fn add_warnings(&mut self, count: usize) {
        self.warning_count = self.warning_count.saturating_add(count as u32);
    }

    pub fn add_skipped(&mut self, count: u32) {
        self.skipped_count = self.skipped_count.saturating_add(count);
    }

    pub fn add_failed(&mut self, count: u32) {
        self.failed_count = self.failed_count.saturating_add(count);
    }

    pub fn is_partial(&self) -> bool {
        self.warning_count > 0 || self.skipped_count > 0 || self.failed_count > 0
    }
}

#[derive(Debug, Clone)]
pub struct PostImportPipelineOptions {
    pub case_root: PathBuf,
    pub db_path: PathBuf,
    pub case_id: String,
    pub data_source_id: DataSourceId,
    pub index_dir: PathBuf,
    pub max_analysis_workers: Option<usize>,
    pub cancel_token: Arc<AtomicBool>,
    pub enable_timeline_projection: bool,
    pub enable_content_extraction: bool,
    pub enable_text_indexing: bool,
    pub analysis_mode: ImportAnalysisMode,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PostImportPipelineError {
    pub message: String,
    pub counts: JobOutcomeCounts,
}

pub fn run_post_import_pipeline_with_counts(
    options: PostImportPipelineOptions,
    progress_cb: Option<AnalysisProgressCallback<'_>>,
) -> Result<(String, JobOutcomeCounts), PostImportPipelineError> {
    let mut counts = JobOutcomeCounts::default();
    if !options.enable_timeline_projection
        && !options.enable_content_extraction
        && !options.enable_text_indexing
    {
        if let Some(cb) = progress_cb {
            cb(
                84,
                &format!(
                    "Post-import skipped: phase=post-import-skip scheduling=deferred workerBudget={} activeWorkers=0 queuedTasks=0 pendingTasks=0 timeline=deferred content=disabled text=disabled contentDeferred=true textDeferred=true",
                    resolve_analysis_worker_count(options.max_analysis_workers).max(1)
                ),
            );
        }
        return Ok((
            "Timeline: deferred until Timeline page. Artifacts: 0. Index: 0 indexed".to_string(),
            counts,
        ));
    }

    let stats = match run_import_analysis_staging(
        {
            let content_deferred =
                !options.enable_content_extraction || !options.analysis_mode.allows_content();
            let text_deferred =
                !options.enable_text_indexing || !options.analysis_mode.allows_content();
            if let Some(cb) = progress_cb {
                cb(
                    70,
                    &format!(
                        "Post-import analysis scheduled: phase=analysis-start scheduling=queued mode={} workerBudget={} activeWorkers=0 queuedTasks=0 pendingTasks=unknown contentDeferred={} textDeferred={}",
                        options.analysis_mode.as_str(),
                        resolve_analysis_worker_count(options.max_analysis_workers).max(1),
                        bool_word(content_deferred),
                        bool_word(text_deferred)
                    ),
                );
            }
            ImportAnalysisOptions {
                case_root: options.case_root,
                db_path: options.db_path,
                case_id: options.case_id,
                data_source_id: options.data_source_id,
                index_dir: options.index_dir,
                max_analysis_workers: options.max_analysis_workers,
                cancel_token: options.cancel_token,
                enable_timeline_projection: options.enable_timeline_projection,
                enable_content_extraction: options.enable_content_extraction,
                enable_text_indexing: options.enable_text_indexing,
                analysis_mode: options.analysis_mode,
                content_budget: content_budget_for_mode(options.analysis_mode),
                memory_soft_limit_mb: default_memory_soft_limit_mb(),
                memory_hard_limit_mb: default_memory_hard_limit_mb(),
            }
        },
        progress_cb,
    ) {
        Ok(stats) => stats,
        Err(message) => {
            counts.add_warnings(1);
            if message.to_ascii_lowercase().contains("cancel") {
                counts.add_skipped(1);
            } else {
                counts.add_failed(1);
            }
            return Err(PostImportPipelineError { message, counts });
        }
    };

    counts.warning_count = counts.warning_count.saturating_add(stats.warning_count);
    counts.skipped_count = counts.skipped_count.saturating_add(stats.skipped_count);
    counts.failed_count = counts.failed_count.saturating_add(stats.failed_count);

    let mut message = format!(
        "Timeline: {} events. Artifacts: {}. Index: {} indexed",
        stats.timeline_count, stats.artifact_count, stats.indexed_count
    );
    if counts.is_partial() {
        message.push_str(&format!(
            ". Partial: {} warnings, {} skipped, {} failed",
            counts.warning_count, counts.skipped_count, counts.failed_count
        ));
    }

    Ok((message, counts))
}

#[derive(Debug, Clone, Default)]
struct WorkerStats {
    processed_count: u64,
    artifact_count: u64,
    timeline_count: u64,
    indexed_count: u64,
    warning_count: u32,
    skipped_count: u32,
    failed_count: u32,
}

#[derive(Debug, Clone)]
struct FileTask {
    id: FileEntryId,
    data_source_id: DataSourceId,
    path: String,
    name: String,
    entry_type: EntryType,
    size: Option<u64>,
    ext: Option<String>,
    deleted: bool,
    created_at: Option<chrono::DateTime<chrono::Utc>>,
    modified_at: Option<chrono::DateTime<chrono::Utc>>,
    accessed_at: Option<chrono::DateTime<chrono::Utc>>,
    changed_at: Option<chrono::DateTime<chrono::Utc>>,
    hash_sha256: Option<String>,
}

impl FileTask {
    fn to_file_entry(&self) -> FileEntry {
        FileEntry {
            id: self.id.clone(),
            parent_id: None,
            data_source_id: self.data_source_id.clone(),
            path: self.path.clone(),
            name: self.name.clone(),
            entry_type: self.entry_type.clone(),
            size: self.size,
            ext: self.ext.clone(),
            deleted: self.deleted,
            created_at: self.created_at,
            modified_at: self.modified_at,
            accessed_at: self.accessed_at,
            changed_at: self.changed_at,
            hash_sha256: self.hash_sha256.clone(),
        }
    }
}

#[derive(Debug)]
struct SharedAnalysisState {
    processed_total: AtomicUsize,
    active_workers: AtomicUsize,
    indexed_total: AtomicUsize,
    queued_total: AtomicUsize,
    content_files_used: AtomicU64,
    content_bytes_used: AtomicU64,
}

impl SharedAnalysisState {
    fn new() -> Self {
        Self {
            processed_total: AtomicUsize::new(0),
            active_workers: AtomicUsize::new(0),
            indexed_total: AtomicUsize::new(0),
            queued_total: AtomicUsize::new(0),
            content_files_used: AtomicU64::new(0),
            content_bytes_used: AtomicU64::new(0),
        }
    }
}

pub fn default_analysis_worker_count() -> usize {
    std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4)
}

pub fn resolve_analysis_worker_count(max_analysis_workers: Option<usize>) -> usize {
    match max_analysis_workers {
        Some(n) if n > 0 => n.min(default_analysis_worker_count()),
        _ => default_analysis_worker_count(),
    }
}

pub fn run_import_analysis_staging(
    options: ImportAnalysisOptions,
    progress_cb: Option<AnalysisProgressCallback<'_>>,
) -> Result<ImportAnalysisStats, String> {
    let analysis_started = Instant::now();
    let worker_count = resolve_analysis_worker_count(options.max_analysis_workers).max(1);
    let worker_ids: Vec<usize> = (0..worker_count).collect();
    let memory_soft_limit_mb = if options.memory_soft_limit_mb == 0 {
        DEFAULT_MEMORY_SOFT_LIMIT_MB
    } else {
        options.memory_soft_limit_mb
    };
    let memory_hard_limit_mb = if options.memory_hard_limit_mb == 0 {
        DEFAULT_MEMORY_HARD_LIMIT_MB
    } else {
        options.memory_hard_limit_mb.max(memory_soft_limit_mb + 1)
    };

    let startup_action =
        prepare_analysis_staging_startup(&options, &worker_ids, worker_count, progress_cb)?;

    if startup_action == AnalysisStartupAction::AlreadyMerged {
        if let Some(cb) = progress_cb {
            cb(
                94,
                "Analysis staging already merged; skipping analysis resume.",
            );
        }
        let merged_worker_ids =
            discover_analysis_worker_ids(&options.case_root, &options.data_source_id.0)?;
        return collect_done_worker_stats(&options, &merged_worker_ids);
    }

    let estimated = count_analysis_file_tasks(&options.db_path, &options.data_source_id)?;
    if memory_hard_limit_exceeded(memory_hard_limit_mb) {
        options.cancel_token.store(true, Ordering::Relaxed);
        return Err(format!(
            "Import analysis memory hard limit exceeded before start: rssMb={} hardLimitMb={}",
            current_rss_mb(),
            memory_hard_limit_mb
        ));
    }
    if let Some(cb) = progress_cb {
        cb(
            72,
            &format!(
                "Analysis staging: phase=analysis-start scheduling=queued mode={} workers={} workerBudget={} activeWorkers=0 queuedTasks=0 pendingTasks={} queueBound={} content={} text={} contentDeferred={} textDeferred={} rssMb={}",
                options.analysis_mode.as_str(),
                worker_count,
                worker_count,
                estimated,
                analysis_task_queue_bound(worker_count),
                if options.enable_content_extraction {
                    "enabled"
                } else {
                    "disabled"
                },
                if options.enable_text_indexing {
                    "enabled"
                } else {
                    "disabled"
                },
                bool_word(!options.enable_content_extraction || !options.analysis_mode.allows_content()),
                bool_word(!options.enable_text_indexing || !options.analysis_mode.allows_content()),
                current_rss_mb()
            ),
        );
    }

    if startup_action == AnalysisStartupAction::MergeOnly {
        let stats = collect_done_worker_stats(&options, &worker_ids)?;
        return merge_finished_analysis_staging(
            &options,
            &worker_ids,
            stats,
            progress_cb,
            analysis_started,
        );
    }

    let queue_bound = analysis_task_queue_bound(worker_count);
    let (task_tx, task_rx): (Sender<FileTask>, Receiver<FileTask>) = bounded(queue_bound);
    let (result_tx, result_rx) = bounded(worker_count);
    let shared = Arc::new(SharedAnalysisState::new());

    let producer_options = options.clone();
    let producer_shared = Arc::clone(&shared);
    let producer = std::thread::Builder::new()
        .name("analysis-task-producer".to_string())
        .spawn(move || {
            let result = enqueue_analysis_tasks(&producer_options, &task_tx, producer_shared);
            drop(task_tx);
            result
        })
        .map_err(|e| format!("Spawn analysis producer: {e}"))?;

    let mut handles = Vec::with_capacity(worker_count);
    for worker_id in worker_ids.iter().copied() {
        let rx = task_rx.clone();
        let tx = result_tx.clone();
        let worker_options = options.clone();
        let shared = Arc::clone(&shared);
        let handle = std::thread::Builder::new()
            .name(format!("analysis-worker-{worker_id}"))
            .spawn(move || {
                shared.active_workers.fetch_add(1, Ordering::Relaxed);
                let result = run_analysis_worker(worker_id, worker_options, rx, shared.clone());
                shared.active_workers.fetch_sub(1, Ordering::Relaxed);
                let _ = tx.send((worker_id, result));
            })
            .map_err(|e| format!("Spawn analysis worker {worker_id}: {e}"))?;
        handles.push(handle);
    }
    drop(task_rx);
    drop(result_tx);

    let mut completed = 0usize;
    let mut stats = ImportAnalysisStats {
        worker_ids: worker_ids.clone(),
        ..ImportAnalysisStats::default()
    };
    while completed < worker_count {
        match result_rx.recv_timeout(Duration::from_millis(250)) {
            Ok((_worker_id, worker_result)) => {
                completed += 1;
                match worker_result {
                    Ok(worker_stats) => add_worker_stats(&mut stats, worker_stats),
                    Err(error) => {
                        stats.warning_count = stats.warning_count.saturating_add(1);
                        stats.failed_count = stats.failed_count.saturating_add(1);
                        tracing::warn!("Analysis worker failed: {}", error);
                    }
                }
                if let Some(cb) = progress_cb {
                    let pct = 72 + ((completed as u32 * 10) / worker_count as u32);
                    cb(
                        pct.min(82),
                        &format!(
                            "Analysis workers done: phase=analysis scheduling={} workersDone={}/{} workerBudget={} processed={} rowsPerSec={} queuedTasks={} pendingTasks={} indexed={} activeWorkers={} rssMb={}",
                            if options.cancel_token.load(Ordering::Relaxed) {
                                "draining"
                            } else {
                                "running"
                            },
                            completed,
                            worker_count,
                            worker_count,
                            shared.processed_total.load(Ordering::Relaxed),
                            rows_per_sec(
                                shared.processed_total.load(Ordering::Relaxed) as u64,
                                analysis_started.elapsed()
                            ),
                            shared.queued_total.load(Ordering::Relaxed),
                            estimated.saturating_sub(
                                shared.processed_total.load(Ordering::Relaxed) as u64
                            ),
                            shared.indexed_total.load(Ordering::Relaxed),
                            shared.active_workers.load(Ordering::Relaxed),
                            current_rss_mb()
                        ),
                    );
                }
            }
            Err(crossbeam_channel::RecvTimeoutError::Timeout) => {
                let rss_mb = current_rss_mb();
                if memory_hard_limit_exceeded(memory_hard_limit_mb) {
                    options.cancel_token.store(true, Ordering::Relaxed);
                    if let Some(cb) = progress_cb {
                        cb(
                            75,
                            &format!(
                                "Analysis memory hard limit exceeded: phase=analysis scheduling=draining rssMb={} hardLimitMb={} workerBudget={} queuedTasks={} pendingTasks={} processed={} activeWorkers={}",
                                rss_mb,
                                memory_hard_limit_mb,
                                worker_count,
                                shared.queued_total.load(Ordering::Relaxed),
                                estimated.saturating_sub(
                                    shared.processed_total.load(Ordering::Relaxed) as u64
                                ),
                                shared.processed_total.load(Ordering::Relaxed),
                                shared.active_workers.load(Ordering::Relaxed)
                            ),
                        );
                    }
                } else if let Some(cb) = progress_cb {
                    let level = if rss_mb >= memory_soft_limit_mb {
                        "soft-limit"
                    } else {
                        "ok"
                    };
                    cb(
                        75,
                        &format!(
                            "Analysis heartbeat: phase=analysis scheduling={} memory={} rssMb={} softLimitMb={} hardLimitMb={} workerBudget={} queuedTasks={} pendingTasks={} processed={}/{} indexed={} activeWorkers={}",
                            scheduling_state(options.cancel_token.load(Ordering::Relaxed), rss_mb, memory_soft_limit_mb),
                            level,
                            rss_mb,
                            memory_soft_limit_mb,
                            memory_hard_limit_mb,
                            worker_count,
                            shared.queued_total.load(Ordering::Relaxed),
                            estimated.saturating_sub(
                                shared.processed_total.load(Ordering::Relaxed) as u64
                            ),
                            shared.processed_total.load(Ordering::Relaxed),
                            estimated,
                            shared.indexed_total.load(Ordering::Relaxed),
                            shared.active_workers.load(Ordering::Relaxed)
                        ),
                    );
                }
            }
            Err(crossbeam_channel::RecvTimeoutError::Disconnected) => break,
        }
    }

    let producer_result = producer
        .join()
        .map_err(|_| "Analysis task producer panicked".to_string())?;
    if let Err(error) = producer_result {
        stats.warning_count = stats.warning_count.saturating_add(1);
        stats.failed_count = stats.failed_count.saturating_add(1);
        return Err(error);
    }

    for handle in handles {
        handle
            .join()
            .map_err(|_| "Analysis worker panicked".to_string())?;
    }

    if options.cancel_token.load(Ordering::Relaxed) {
        stats.warning_count = stats.warning_count.saturating_add(1);
        return Err("Import analysis cancelled by user".to_string());
    }

    merge_finished_analysis_staging(&options, &worker_ids, stats, progress_cb, analysis_started)
}

fn merge_finished_analysis_staging(
    options: &ImportAnalysisOptions,
    worker_ids: &[usize],
    mut stats: ImportAnalysisStats,
    progress_cb: Option<AnalysisProgressCallback<'_>>,
    analysis_started: Instant,
) -> Result<ImportAnalysisStats, String> {
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
        &persistence_sqlite::open_or_create(&options.db_path).map_err(|e| e.to_string())?,
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
    )?;
    stats.artifact_count = merge_stats.artifact_count;
    stats.timeline_count = merge_stats.timeline_count;
    stats.indexed_count = merge_stats.indexed_count;

    Ok(stats)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AnalysisStartupAction {
    RunWorkers,
    MergeOnly,
    AlreadyMerged,
}

#[derive(Debug, Clone)]
struct ExistingAnalysisWorker {
    worker_id: usize,
    status: Option<String>,
    merged: bool,
    worker_count: Option<usize>,
}

fn prepare_analysis_staging_startup(
    options: &ImportAnalysisOptions,
    worker_ids: &[usize],
    worker_count: usize,
    progress_cb: Option<AnalysisProgressCallback<'_>>,
) -> Result<AnalysisStartupAction, String> {
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
            .map_err(|e| format!("Remove stale analysis staging {worker_id}: {e}"))?;
    }

    for worker_id in worker_ids {
        let conn = staging::open_analysis_staging(
            &options.case_root,
            &options.data_source_id.0,
            *worker_id,
        )
        .map_err(|e| format!("Open analysis staging {worker_id}: {e}"))?;
        clear_analysis_worker_rows(&conn)
            .map_err(|e| format!("Clear analysis staging {worker_id}: {e}"))?;
        init_analysis_worker_meta(&conn, worker_count, "pending")
            .map_err(|e| format!("Init analysis staging {worker_id}: {e}"))?;
    }

    Ok(AnalysisStartupAction::RunWorkers)
}

fn discover_analysis_worker_ids(
    case_root: &Path,
    data_source_id: &str,
) -> Result<Vec<usize>, String> {
    let dir = staging::staging_dir(case_root, data_source_id);
    if !dir.exists() {
        return Ok(Vec::new());
    }

    let mut worker_ids = Vec::new();
    for entry in std::fs::read_dir(&dir).map_err(|e| format!("Read staging dir: {e}"))? {
        let entry = entry.map_err(|e| format!("Read staging entry: {e}"))?;
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
) -> Result<Vec<ExistingAnalysisWorker>, String> {
    let mut workers = Vec::with_capacity(worker_ids.len());
    for worker_id in worker_ids {
        let conn = staging::open_analysis_staging(
            &options.case_root,
            &options.data_source_id.0,
            *worker_id,
        )
        .map_err(|e| format!("Open analysis staging {worker_id}: {e}"))?;
        let status = staging::get_worker_meta(&conn, "status")
            .map_err(|e| format!("Read analysis staging status {worker_id}: {e}"))?;
        let merged = staging::get_worker_meta(&conn, "merged")
            .map_err(|e| format!("Read analysis staging merge flag {worker_id}: {e}"))?
            .as_deref()
            == Some("true");
        let worker_count = staging::get_worker_meta(&conn, "worker_count")
            .map_err(|e| format!("Read analysis staging worker count {worker_id}: {e}"))?
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

fn collect_done_worker_stats(
    options: &ImportAnalysisOptions,
    worker_ids: &[usize],
) -> Result<ImportAnalysisStats, String> {
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
        .map_err(|e| format!("Open analysis staging {worker_id}: {e}"))?;
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

fn worker_meta_u64(conn: &Connection, key: &str) -> Result<u64, String> {
    Ok(staging::get_worker_meta(conn, key)
        .map_err(|e| format!("Read worker meta {key}: {e}"))?
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(0))
}

fn worker_meta_u32(conn: &Connection, key: &str) -> Result<u32, String> {
    Ok(worker_meta_u64(conn, key)?.min(u32::MAX as u64) as u32)
}

fn enqueue_analysis_tasks(
    options: &ImportAnalysisOptions,
    task_tx: &Sender<FileTask>,
    shared: Arc<SharedAnalysisState>,
) -> Result<(), String> {
    let conn = persistence_sqlite::open_or_create(&options.db_path).map_err(|e| e.to_string())?;
    let mut offset = 0u64;
    loop {
        if options.cancel_token.load(Ordering::Relaxed) {
            break;
        }
        let page =
            fetch_analysis_file_page(&conn, &options.data_source_id, offset, FILE_PAGE_SIZE)?;
        if page.is_empty() {
            break;
        }
        for file in page {
            if options.cancel_token.load(Ordering::Relaxed) {
                break;
            }
            task_tx
                .send(file)
                .map_err(|e| format!("Queue analysis task: {e}"))?;
            shared.queued_total.fetch_add(1, Ordering::Relaxed);
        }
        offset += FILE_PAGE_SIZE;
    }
    Ok(())
}

fn run_analysis_worker(
    worker_id: usize,
    options: ImportAnalysisOptions,
    task_rx: Receiver<FileTask>,
    shared: Arc<SharedAnalysisState>,
) -> Result<WorkerStats, String> {
    let main_conn =
        persistence_sqlite::open_or_create(&options.db_path).map_err(|e| e.to_string())?;
    let staging_conn =
        staging::open_analysis_staging(&options.case_root, &options.data_source_id.0, worker_id)
            .map_err(|e| e.to_string())?;
    staging::set_worker_meta(&staging_conn, "status", "running").map_err(|e| e.to_string())?;

    let mut stats = WorkerStats::default();
    let registry = artifact_service::create_registry();
    let mut artifacts = Vec::with_capacity(WORKER_INSERT_BATCH);
    let mut timeline_events = Vec::with_capacity(WORKER_INSERT_BATCH);
    let mut index_docs = Vec::with_capacity(INDEX_DOC_INSERT_BATCH);

    while let Ok(task) = task_rx.recv() {
        if options.cancel_token.load(Ordering::Relaxed) {
            break;
        }

        let file = task.to_file_entry();
        stats.processed_count += 1;
        shared.processed_total.fetch_add(1, Ordering::Relaxed);

        if options.enable_timeline_projection {
            let events = timeline::project_file_macb(&file);
            stats.timeline_count += events.len() as u64;
            timeline_events.extend(events);
        }

        if options.analysis_mode.allows_content()
            && options.enable_content_extraction
            && should_extract_artifact(&registry, &file)
            && reserve_content_budget(&options.content_budget, &file, &shared)
        {
            match file_service::open_file_content_by_id(&main_conn, &file.id) {
                Ok(reader) => {
                    let mut sink = VecSink::new();
                    match artifact_service::run_extractors_on_file(
                        &registry, &file.id, &file.path, reader, &mut sink,
                    ) {
                        Ok(extract_stats) => {
                            stats.warning_count = stats
                                .warning_count
                                .saturating_add(extract_stats.warning_count);
                            stats.skipped_count = stats
                                .skipped_count
                                .saturating_add(extract_stats.skipped_count);
                            stats.failed_count = stats
                                .failed_count
                                .saturating_add(extract_stats.failed_count);
                        }
                        Err(error) => {
                            stats.warning_count = stats.warning_count.saturating_add(1);
                            stats.skipped_count = stats.skipped_count.saturating_add(1);
                            tracing::warn!(
                                "Artifact extraction failed for {}: {}",
                                file.path,
                                error
                            );
                        }
                    }
                    stats.artifact_count += sink.artifacts.len() as u64;
                    stats.timeline_count += sink.timeline_events.len() as u64;
                    artifacts.extend(sink.artifacts);
                    timeline_events.extend(sink.timeline_events);
                }
                Err(error) => {
                    stats.warning_count = stats.warning_count.saturating_add(1);
                    stats.skipped_count = stats.skipped_count.saturating_add(1);
                    tracing::warn!(
                        "Artifact extraction skipped unreadable file {}: {}",
                        file.path,
                        error
                    );
                }
            }
        }

        if options.enable_text_indexing
            && should_index_file(&file)
            && options.analysis_mode.allows_content()
            && reserve_content_budget(&options.content_budget, &file, &shared)
            && shared.indexed_total.load(Ordering::Relaxed)
                < infrastructure::constants::IMPORT_TEXT_INDEX_LIMIT
        {
            if let Ok(mut reader) = file_service::open_file_content_by_id(&main_conn, &file.id) {
                let mime = mime_hint_for_entry(&file);
                let text = extract_text(
                    reader
                        .by_ref()
                        .take(infrastructure::constants::IMPORT_TEXT_INDEX_FILE_LIMIT_BYTES),
                    &file.id.0,
                    mime,
                );
                if text.extractable && !text.content.is_empty() {
                    let previous = shared.indexed_total.fetch_add(1, Ordering::Relaxed);
                    if previous < infrastructure::constants::IMPORT_TEXT_INDEX_LIMIT {
                        stats.indexed_count += 1;
                        index_docs.push(IndexDocRow {
                            file_id: file.id.0.clone(),
                            path: file.path.clone(),
                            text: text.content,
                            language: text.encoding,
                            truncated: text.byte_count
                                >= infrastructure::constants::IMPORT_TEXT_INDEX_FILE_LIMIT_BYTES,
                        });
                    }
                }
            } else {
                stats.warning_count = stats.warning_count.saturating_add(1);
                stats.skipped_count = stats.skipped_count.saturating_add(1);
            }
        }

        if artifacts.len() >= WORKER_INSERT_BATCH
            || timeline_events.len() >= WORKER_INSERT_BATCH
            || index_docs.len() >= INDEX_DOC_INSERT_BATCH
        {
            flush_worker_rows(
                &staging_conn,
                &mut artifacts,
                &mut timeline_events,
                &mut index_docs,
            )?;
            persist_worker_stats(&staging_conn, &stats)?;
        }
    }

    flush_worker_rows(
        &staging_conn,
        &mut artifacts,
        &mut timeline_events,
        &mut index_docs,
    )?;
    persist_worker_stats(&staging_conn, &stats)?;

    let status = if options.cancel_token.load(Ordering::Relaxed) {
        "cancelled"
    } else {
        "done"
    };
    staging::set_worker_meta(&staging_conn, "status", status).map_err(|e| e.to_string())?;
    if status == "cancelled" {
        staging::set_worker_meta(&staging_conn, "error", "cancelled").map_err(|e| e.to_string())?;
    }
    Ok(stats)
}

fn clear_analysis_worker_rows(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(
        "DELETE FROM artifact_rows;
         DELETE FROM timeline_rows;
         DELETE FROM index_docs;",
    )
}

fn persist_worker_stats(conn: &Connection, stats: &WorkerStats) -> Result<(), String> {
    staging::set_worker_meta(conn, "processed_count", &stats.processed_count.to_string())
        .map_err(|e| e.to_string())?;
    staging::set_worker_meta(conn, "artifact_count", &stats.artifact_count.to_string())
        .map_err(|e| e.to_string())?;
    staging::set_worker_meta(conn, "timeline_count", &stats.timeline_count.to_string())
        .map_err(|e| e.to_string())?;
    staging::set_worker_meta(conn, "indexed_count", &stats.indexed_count.to_string())
        .map_err(|e| e.to_string())?;
    staging::set_worker_meta(conn, "warning_count", &stats.warning_count.to_string())
        .map_err(|e| e.to_string())?;
    staging::set_worker_meta(conn, "skipped_count", &stats.skipped_count.to_string())
        .map_err(|e| e.to_string())?;
    staging::set_worker_meta(conn, "failed_count", &stats.failed_count.to_string())
        .map_err(|e| e.to_string())
}

fn flush_worker_rows(
    conn: &Connection,
    artifacts: &mut Vec<domain::Artifact>,
    timeline_events: &mut Vec<domain::TimelineEvent>,
    index_docs: &mut Vec<IndexDocRow>,
) -> Result<(), String> {
    if artifacts.is_empty() && timeline_events.is_empty() && index_docs.is_empty() {
        return Ok(());
    }
    let tx = conn
        .unchecked_transaction()
        .map_err(|e| format!("Begin worker staging tx: {e}"))?;
    {
        let mut artifact_stmt = tx
            .prepare_cached(
                "INSERT OR IGNORE INTO artifact_rows
                 (id, file_id, artifact_type, extractor_id, extractor_version, confidence, source_attribution, display_name, summary, data_json, source_path, created_at)
                  VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
            )
            .map_err(|e| format!("Prepare artifact staging insert: {e}"))?;
        for artifact in artifacts.iter() {
            artifact_stmt
                .execute(params![
                    artifact.id.0,
                    artifact.source_object_id.as_ref().map(|id| &id.0),
                    artifact.family,
                    artifact.extractor_id,
                    artifact.extractor_version,
                    artifact.confidence,
                    artifact.source_attribution,
                    artifact.title,
                    artifact.summary,
                    serde_json::to_string(&artifact.attrs).unwrap_or_else(|_| "{}".to_string()),
                    "",
                    artifact.created_at.to_rfc3339(),
                ])
                .map_err(|e| format!("Insert artifact staging row: {e}"))?;
        }
    }
    {
        let mut timeline_stmt = tx
            .prepare_cached(
                "INSERT OR IGNORE INTO timeline_rows
                 (id, file_id, timestamp, event_type, parser_id, parser_version, confidence, source_attribution, title, description, data_json)
                  VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            )
            .map_err(|e| format!("Prepare timeline staging insert: {e}"))?;
        for event in timeline_events.iter() {
            timeline_stmt
                .execute(params![
                    event.id.0,
                    event.source_object_id,
                    event.timestamp.to_rfc3339(),
                    event.event_type,
                    event.parser_id,
                    event.parser_version,
                    event.confidence,
                    event.source_attribution,
                    event.title,
                    event.description,
                    serde_json::to_string(&event.attrs).unwrap_or_else(|_| "{}".to_string()),
                ])
                .map_err(|e| format!("Insert timeline staging row: {e}"))?;
        }
    }
    {
        let mut index_stmt = tx
            .prepare_cached(
                "INSERT OR REPLACE INTO index_docs
                 (file_id, path, text, language, truncated)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
            )
            .map_err(|e| format!("Prepare index staging insert: {e}"))?;
        for doc in index_docs.iter() {
            index_stmt
                .execute(params![
                    doc.file_id,
                    doc.path,
                    doc.text,
                    doc.language,
                    doc.truncated as i32,
                ])
                .map_err(|e| format!("Insert index staging row: {e}"))?;
        }
    }
    tx.commit()
        .map_err(|e| format!("Commit worker staging tx: {e}"))?;
    artifacts.clear();
    timeline_events.clear();
    index_docs.clear();
    Ok(())
}

#[derive(Debug)]
struct IndexDocRow {
    file_id: String,
    path: String,
    text: String,
    language: String,
    truncated: bool,
}

fn add_worker_stats(stats: &mut ImportAnalysisStats, worker: WorkerStats) {
    stats.processed_count += worker.processed_count;
    stats.artifact_count += worker.artifact_count;
    stats.timeline_count += worker.timeline_count;
    stats.indexed_count += worker.indexed_count;
    stats.warning_count = stats.warning_count.saturating_add(worker.warning_count);
    stats.skipped_count = stats.skipped_count.saturating_add(worker.skipped_count);
    stats.failed_count = stats.failed_count.saturating_add(worker.failed_count);
}

fn analysis_task_queue_bound(worker_count: usize) -> usize {
    worker_count.max(1) * TASKS_PER_WORKER_BOUND
}

fn mime_hint_for_entry(file: &FileEntry) -> Option<&'static str> {
    let ext = normalized_ext(file);
    if matches!(ext, "txt" | "log" | "csv" | "json" | "xml" | "html" | "md") {
        Some("text/plain")
    } else {
        None
    }
}

fn normalized_ext(file: &FileEntry) -> &str {
    file.ext
        .as_deref()
        .or_else(|| file.name.rsplit_once('.').map(|(_, ext)| ext))
        .unwrap_or("")
        .trim_start_matches('.')
}

fn should_index_file(file: &FileEntry) -> bool {
    let Some(size) = file.size else {
        return false;
    };
    if size > infrastructure::constants::IMPORT_TEXT_INDEX_FILE_LIMIT_BYTES {
        return false;
    }

    matches!(
        normalized_ext(file).to_ascii_lowercase().as_str(),
        "txt" | "log" | "csv" | "json" | "xml" | "html" | "htm" | "md"
    )
}

fn should_extract_artifact(registry: &artifacts_core::ExtractorRegistry, file: &FileEntry) -> bool {
    if registry.find_for_path(&file.path).is_empty() {
        return false;
    }
    file.size
        .is_some_and(|size| size <= infrastructure::constants::IMPORT_TEXT_INDEX_FILE_LIMIT_BYTES)
}

fn reserve_content_budget(
    budget: &ContentBudget,
    file: &FileEntry,
    shared: &SharedAnalysisState,
) -> bool {
    let Some(size) = file.size else {
        return false;
    };
    if budget.max_files == 0 || budget.max_bytes_total == 0 || budget.max_bytes_per_file == 0 {
        return false;
    }
    if size > budget.max_bytes_per_file {
        return false;
    }
    if !budget.allowed_extensions.is_empty() {
        let ext = normalized_ext(file).to_ascii_lowercase();
        if !budget
            .allowed_extensions
            .iter()
            .any(|allowed| allowed == &ext)
        {
            return false;
        }
    }
    let previous_files = shared.content_files_used.fetch_add(1, Ordering::Relaxed);
    if previous_files >= budget.max_files {
        shared.content_files_used.fetch_sub(1, Ordering::Relaxed);
        return false;
    }
    let previous_bytes = shared.content_bytes_used.fetch_add(size, Ordering::Relaxed);
    if previous_bytes.saturating_add(size) > budget.max_bytes_total {
        shared.content_files_used.fetch_sub(1, Ordering::Relaxed);
        shared.content_bytes_used.fetch_sub(size, Ordering::Relaxed);
        return false;
    }
    true
}

pub fn current_rss_mb() -> u64 {
    #[cfg(test)]
    if let Some(value) = test_rss_override_mb() {
        return value;
    }
    current_rss_bytes() / (1024 * 1024)
}

fn rows_per_sec(rows: u64, duration: Duration) -> u64 {
    let secs = duration.as_secs_f64();
    if secs <= 0.0 {
        rows
    } else {
        (rows as f64 / secs).round() as u64
    }
}

fn bool_word(value: bool) -> &'static str {
    if value {
        "true"
    } else {
        "false"
    }
}

fn scheduling_state(
    cancel_requested: bool,
    rss_mb: u64,
    memory_soft_limit_mb: u64,
) -> &'static str {
    if cancel_requested {
        "draining"
    } else if rss_mb >= memory_soft_limit_mb {
        "throttled"
    } else {
        "running"
    }
}

fn memory_hard_limit_exceeded(limit_mb: u64) -> bool {
    let rss_mb = current_rss_mb();
    rss_mb > 0 && rss_mb >= limit_mb
}

#[cfg(test)]
static TEST_RSS_OVERRIDE_MB: AtomicU64 = AtomicU64::new(0);

#[cfg(test)]
fn test_rss_override_mb() -> Option<u64> {
    match TEST_RSS_OVERRIDE_MB.load(Ordering::Relaxed) {
        0 => None,
        value => Some(value),
    }
}

#[cfg(test)]
fn set_test_rss_override_mb(value: Option<u64>) {
    TEST_RSS_OVERRIDE_MB.store(value.unwrap_or(0), Ordering::Relaxed);
}

#[cfg(target_os = "windows")]
fn current_rss_bytes() -> u64 {
    #[repr(C)]
    #[allow(non_snake_case)]
    struct ProcessMemoryCounters {
        cb: u32,
        PageFaultCount: u32,
        PeakWorkingSetSize: usize,
        WorkingSetSize: usize,
        QuotaPeakPagedPoolUsage: usize,
        QuotaPagedPoolUsage: usize,
        QuotaPeakNonPagedPoolUsage: usize,
        QuotaNonPagedPoolUsage: usize,
        PagefileUsage: usize,
        PeakPagefileUsage: usize,
    }

    extern "system" {
        fn GetCurrentProcess() -> *mut std::ffi::c_void;
        fn GetProcessMemoryInfo(
            process: *mut std::ffi::c_void,
            counters: *mut ProcessMemoryCounters,
            size: u32,
        ) -> i32;
    }

    let mut counters = ProcessMemoryCounters {
        cb: std::mem::size_of::<ProcessMemoryCounters>() as u32,
        PageFaultCount: 0,
        PeakWorkingSetSize: 0,
        WorkingSetSize: 0,
        QuotaPeakPagedPoolUsage: 0,
        QuotaPagedPoolUsage: 0,
        QuotaPeakNonPagedPoolUsage: 0,
        QuotaNonPagedPoolUsage: 0,
        PagefileUsage: 0,
        PeakPagefileUsage: 0,
    };
    let ok = unsafe {
        GetProcessMemoryInfo(
            GetCurrentProcess(),
            &mut counters,
            std::mem::size_of::<ProcessMemoryCounters>() as u32,
        )
    };
    if ok == 0 {
        0
    } else {
        counters.WorkingSetSize as u64
    }
}

#[cfg(not(target_os = "windows"))]
fn current_rss_bytes() -> u64 {
    0
}

fn count_analysis_file_tasks(db_path: &Path, data_source_id: &DataSourceId) -> Result<u64, String> {
    let conn = persistence_sqlite::open_or_create(db_path).map_err(|e| e.to_string())?;
    let count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM file_entries
             WHERE data_source_id = ?1 AND LOWER(entry_type) = 'file'",
            params![data_source_id.0],
            |row| row.get(0),
        )
        .map_err(|e| e.to_string())?;
    Ok(count as u64)
}

fn fetch_analysis_file_page(
    conn: &Connection,
    data_source_id: &DataSourceId,
    offset: u64,
    limit: u64,
) -> Result<Vec<FileTask>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT id, parent_id, data_source_id, path, name, entry_type,
                    size, ext, deleted, created_at, modified_at, accessed_at, changed_at, hash_sha256
             FROM file_entries
             WHERE data_source_id = ?1 AND LOWER(entry_type) = 'file'
             ORDER BY path ASC
             LIMIT ?2 OFFSET ?3",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(params![data_source_id.0, limit, offset], row_to_file_task)
        .map_err(|e| e.to_string())?;
    let mut files = Vec::new();
    for row in rows {
        files.push(row.map_err(|e| e.to_string())?);
    }
    Ok(files)
}

fn row_to_file_task(row: &rusqlite::Row<'_>) -> rusqlite::Result<FileTask> {
    let entry_type_str: String = row.get(5)?;
    Ok(FileTask {
        id: FileEntryId(row.get::<_, String>(0)?),
        data_source_id: DataSourceId(row.get::<_, String>(2)?),
        path: row.get(3)?,
        name: row.get(4)?,
        entry_type: if entry_type_str.eq_ignore_ascii_case("directory") {
            EntryType::Directory
        } else {
            EntryType::File
        },
        size: row.get(6)?,
        ext: row.get(7)?,
        deleted: row.get::<_, i32>(8)? != 0,
        created_at: row
            .get::<_, Option<String>>(9)?
            .and_then(|s| chrono::DateTime::parse_from_rfc3339(&s).ok())
            .map(|dt| dt.with_timezone(&chrono::Utc)),
        modified_at: row
            .get::<_, Option<String>>(10)?
            .and_then(|s| chrono::DateTime::parse_from_rfc3339(&s).ok())
            .map(|dt| dt.with_timezone(&chrono::Utc)),
        accessed_at: row
            .get::<_, Option<String>>(11)?
            .and_then(|s| chrono::DateTime::parse_from_rfc3339(&s).ok())
            .map(|dt| dt.with_timezone(&chrono::Utc)),
        changed_at: row
            .get::<_, Option<String>>(12)?
            .and_then(|s| chrono::DateTime::parse_from_rfc3339(&s).ok())
            .map(|dt| dt.with_timezone(&chrono::Utc)),
        hash_sha256: row.get(13)?,
    })
}

#[allow(dead_code)]
fn _keep_file_repo_import_used_for_docs(_: &FileRepo<'_>) {}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};
    use persistence_sqlite::runner;
    use tempfile::TempDir;

    fn setup_case_db(tmp: &TempDir) -> (PathBuf, DataSourceId) {
        let db_path = tmp.path().join("app.db");
        let conn = persistence_sqlite::open_or_create(&db_path).unwrap();
        runner::run_all(&conn).unwrap();
        conn.execute(
            "INSERT INTO cases (id, name, created_at, updated_at)
             VALUES ('case-1', 'case', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO data_sources (id, case_id, name, kind, source_path, imported_at)
             VALUES ('ds-1', 'case-1', 'logical', 'logical_directory', ?1, '2026-01-01T00:00:00Z')",
            params![tmp.path().join("evidence").display().to_string()],
        )
        .unwrap();
        (db_path, DataSourceId("ds-1".to_string()))
    }

    fn insert_file_with_type(
        conn: &Connection,
        id: &str,
        ds: &DataSourceId,
        path: &str,
        entry_type: &str,
    ) {
        conn.execute(
            "INSERT INTO file_entries
             (id, data_source_id, path, name, entry_type, size, ext, deleted, modified_at)
             VALUES (?1, ?2, ?3, ?4, ?5, 12, 'txt', 0, ?6)",
            params![
                id,
                ds.0,
                path,
                path.rsplit('/').next().unwrap_or(path),
                entry_type,
                Utc.with_ymd_and_hms(2026, 1, 1, 1, 0, 0)
                    .unwrap()
                    .to_rfc3339()
            ],
        )
        .unwrap();
    }

    fn insert_file(conn: &Connection, id: &str, ds: &DataSourceId, path: &str) {
        insert_file_with_type(conn, id, ds, path, "file");
    }

    fn insert_staged_index_doc(conn: &Connection, file_id: &str, text: &str) {
        conn.execute(
            "INSERT OR REPLACE INTO index_docs
             (file_id, path, text, language, truncated)
             VALUES (?1, ?2, ?3, 'unknown', 0)",
            params![file_id, format!("{file_id}.txt"), text],
        )
        .unwrap();
    }

    fn set_done_worker_meta(
        conn: &Connection,
        worker_count: usize,
        merged: bool,
        processed_count: u64,
    ) {
        staging::set_worker_meta(conn, "status", "done").unwrap();
        staging::set_worker_meta(conn, "merged", if merged { "true" } else { "false" }).unwrap();
        staging::set_worker_meta(conn, "worker_count", &worker_count.to_string()).unwrap();
        staging::set_worker_meta(conn, "processed_count", &processed_count.to_string()).unwrap();
    }

    fn analysis_options(
        tmp: &TempDir,
        db_path: PathBuf,
        ds_id: DataSourceId,
        mode: ImportAnalysisMode,
    ) -> ImportAnalysisOptions {
        ImportAnalysisOptions {
            case_root: tmp.path().to_path_buf(),
            db_path,
            case_id: "case-1".to_string(),
            data_source_id: ds_id,
            index_dir: tmp.path().join("indexes").join("tantivy"),
            max_analysis_workers: Some(1),
            cancel_token: Arc::new(AtomicBool::new(false)),
            enable_timeline_projection: true,
            enable_content_extraction: mode.allows_content(),
            enable_text_indexing: mode.allows_content(),
            analysis_mode: mode,
            content_budget: content_budget_for_mode(mode),
            memory_soft_limit_mb: default_memory_soft_limit_mb(),
            memory_hard_limit_mb: default_memory_hard_limit_mb(),
        }
    }

    fn post_import_options(
        tmp: &TempDir,
        db_path: PathBuf,
        ds_id: DataSourceId,
        mode: ImportAnalysisMode,
    ) -> PostImportPipelineOptions {
        PostImportPipelineOptions {
            case_root: tmp.path().to_path_buf(),
            db_path,
            case_id: "case-1".to_string(),
            data_source_id: ds_id,
            index_dir: tmp.path().join("indexes").join("tantivy"),
            max_analysis_workers: Some(1),
            cancel_token: Arc::new(AtomicBool::new(false)),
            enable_timeline_projection: true,
            enable_content_extraction: mode.allows_content(),
            enable_text_indexing: mode.allows_content(),
            analysis_mode: mode,
        }
    }

    #[test]
    fn post_import_skip_uses_progress_sink_without_running_workers() {
        let tmp = TempDir::new().unwrap();
        let options = PostImportPipelineOptions {
            case_root: tmp.path().to_path_buf(),
            db_path: tmp.path().join("app.db"),
            case_id: "case-1".to_string(),
            data_source_id: DataSourceId("ds-1".to_string()),
            index_dir: tmp.path().join("indexes").join("tantivy"),
            max_analysis_workers: Some(1),
            cancel_token: Arc::new(AtomicBool::new(false)),
            enable_timeline_projection: false,
            enable_content_extraction: false,
            enable_text_indexing: false,
            analysis_mode: ImportAnalysisMode::MetadataOnly,
        };
        let events = std::sync::Mutex::new(Vec::new());
        let progress = |pct: u32, detail: &str| {
            events.lock().unwrap().push((pct, detail.to_string()));
        };

        let (message, counts) = run_post_import_pipeline_with_counts(options, Some(&progress))
            .expect("post import skip");

        assert_eq!(
            message,
            "Timeline: deferred until Timeline page. Artifacts: 0. Index: 0 indexed"
        );
        assert_eq!(counts, JobOutcomeCounts::default());
        let events = events.lock().unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].0, 84);
        assert!(events[0].1.contains("phase=post-import-skip"));
        assert!(events[0].1.contains("scheduling=deferred"));
        assert!(events[0].1.contains("workerBudget=1"));
        assert!(events[0].1.contains("activeWorkers=0"));
        assert!(events[0].1.contains("queuedTasks=0"));
        assert!(events[0].1.contains("pendingTasks=0"));
        assert!(events[0].1.contains("contentDeferred=true"));
        assert!(events[0].1.contains("textDeferred=true"));
    }

    #[test]
    fn post_import_worker_staging_success_preserves_summary_and_counts() {
        let tmp = TempDir::new().unwrap();
        std::fs::create_dir_all(tmp.path().join("evidence")).unwrap();
        let (db_path, ds_id) = setup_case_db(&tmp);
        let conn = persistence_sqlite::open_or_create(&db_path).unwrap();
        insert_file(&conn, "f-a", &ds_id, "a.txt");
        insert_file(&conn, "f-b", &ds_id, "b.txt");
        drop(conn);
        let options = post_import_options(
            &tmp,
            db_path,
            ds_id.clone(),
            ImportAnalysisMode::MetadataOnly,
        );
        let events = std::sync::Mutex::new(Vec::new());
        let progress = |pct: u32, detail: &str| {
            events.lock().unwrap().push((pct, detail.to_string()));
        };

        let (message, counts) = run_post_import_pipeline_with_counts(options, Some(&progress))
            .expect("post import success");

        assert!(message.starts_with("Timeline: 2 events"));
        assert!(message.contains("Artifacts: 0. Index: 0 indexed"));
        assert_eq!(counts, JobOutcomeCounts::default());
        let events = events.lock().unwrap();
        let scheduled = events
            .iter()
            .find(|(_, detail)| detail.contains("Post-import analysis scheduled"))
            .expect("scheduled progress");
        assert!(scheduled.1.contains("scheduling=queued"));
        assert!(scheduled.1.contains("workerBudget=1"));
        assert!(scheduled.1.contains("contentDeferred=true"));
        assert!(scheduled.1.contains("textDeferred=true"));
        let started = events
            .iter()
            .find(|(_, detail)| detail.contains("Analysis staging:"))
            .expect("analysis start progress");
        assert!(started.1.contains("scheduling=queued"));
        assert!(started.1.contains("queueBound=256"));
        assert!(started.1.contains("pendingTasks=2"));
        assert!(events
            .iter()
            .any(|(_, detail)| detail.contains("Analysis workers complete")
                && detail.contains("workerBudget=1")
                && detail.contains("pendingTasks=0")));
        let main_conn = persistence_sqlite::open_or_create(&tmp.path().join("app.db")).unwrap();
        let timeline_count: i64 = main_conn
            .query_row("SELECT COUNT(*) FROM timeline_events", [], |row| row.get(0))
            .unwrap();
        assert_eq!(timeline_count, 2);
    }

    #[test]
    fn post_import_cancel_failure_preserves_partial_counts() {
        let tmp = TempDir::new().unwrap();
        std::fs::create_dir_all(tmp.path().join("evidence")).unwrap();
        let (db_path, ds_id) = setup_case_db(&tmp);
        let conn = persistence_sqlite::open_or_create(&db_path).unwrap();
        insert_file(&conn, "f-a", &ds_id, "a.txt");
        drop(conn);
        let cancel = Arc::new(AtomicBool::new(true));
        let mut options = post_import_options(
            &tmp,
            db_path,
            ds_id.clone(),
            ImportAnalysisMode::MetadataOnly,
        );
        options.cancel_token = cancel;

        let error = run_post_import_pipeline_with_counts(options, None).unwrap_err();

        assert!(error.message.contains("cancelled"));
        assert_eq!(error.counts.warning_count, 1);
        assert_eq!(error.counts.skipped_count, 1);
        assert_eq!(error.counts.failed_count, 0);
        assert!(error.counts.is_partial());
    }

    #[test]
    fn done_merged_analysis_worker_dbs_are_left_untouched() {
        let tmp = TempDir::new().unwrap();
        let (db_path, ds_id) = setup_case_db(&tmp);
        let options = analysis_options(
            &tmp,
            db_path,
            ds_id.clone(),
            ImportAnalysisMode::MetadataOnly,
        );
        let worker_conn = staging::open_analysis_staging(tmp.path(), &ds_id.0, 0).unwrap();
        insert_staged_index_doc(&worker_conn, "already-merged", "keep me");
        set_done_worker_meta(&worker_conn, 1, true, 7);

        let action =
            prepare_analysis_staging_startup(&options, &[0], 1, None).expect("startup plan");

        assert_eq!(action, AnalysisStartupAction::AlreadyMerged);
        let (_artifacts, _timeline, index) =
            staging::analysis_staging_counts(&worker_conn).unwrap();
        assert_eq!(index, 1);
        assert_eq!(
            staging::get_worker_meta(&worker_conn, "merged")
                .unwrap()
                .as_deref(),
            Some("true")
        );
        assert_eq!(
            staging::get_worker_meta(&worker_conn, "processed_count")
                .unwrap()
                .as_deref(),
            Some("7")
        );
    }

    #[test]
    fn stale_unmerged_worker_layout_is_reinitialized_when_worker_count_changes() {
        let tmp = TempDir::new().unwrap();
        let (db_path, ds_id) = setup_case_db(&tmp);
        let options = analysis_options(
            &tmp,
            db_path,
            ds_id.clone(),
            ImportAnalysisMode::MetadataOnly,
        );

        let worker0 = staging::open_analysis_staging(tmp.path(), &ds_id.0, 0).unwrap();
        insert_staged_index_doc(&worker0, "old-worker-0", "stale");
        set_done_worker_meta(&worker0, 2, false, 11);
        drop(worker0);

        let worker1 = staging::open_analysis_staging(tmp.path(), &ds_id.0, 1).unwrap();
        insert_staged_index_doc(&worker1, "old-worker-1", "stale");
        set_done_worker_meta(&worker1, 2, false, 13);
        drop(worker1);
        assert!(staging::analysis_staging_db_path(tmp.path(), &ds_id.0, 1).exists());

        let action =
            prepare_analysis_staging_startup(&options, &[0], 1, None).expect("startup plan");

        assert_eq!(action, AnalysisStartupAction::RunWorkers);
        let worker0 = staging::open_analysis_staging(tmp.path(), &ds_id.0, 0).unwrap();
        let (_artifacts, _timeline, index) = staging::analysis_staging_counts(&worker0).unwrap();
        assert_eq!(index, 0);
        assert_eq!(
            staging::get_worker_meta(&worker0, "status")
                .unwrap()
                .as_deref(),
            Some("pending")
        );
        assert_eq!(
            staging::get_worker_meta(&worker0, "worker_count")
                .unwrap()
                .as_deref(),
            Some("1")
        );
        assert!(!staging::analysis_staging_db_path(tmp.path(), &ds_id.0, 1).exists());
    }

    #[test]
    fn done_unmerged_matching_layout_resumes_with_merge_only() {
        let tmp = TempDir::new().unwrap();
        let (db_path, ds_id) = setup_case_db(&tmp);
        let options = analysis_options(
            &tmp,
            db_path,
            ds_id.clone(),
            ImportAnalysisMode::MetadataOnly,
        );
        let worker_conn = staging::open_analysis_staging(tmp.path(), &ds_id.0, 0).unwrap();
        insert_staged_index_doc(&worker_conn, "ready-to-merge", "keep for merge");
        set_done_worker_meta(&worker_conn, 1, false, 5);

        let action =
            prepare_analysis_staging_startup(&options, &[0], 1, None).expect("startup plan");

        assert_eq!(action, AnalysisStartupAction::MergeOnly);
        let (_artifacts, _timeline, index) =
            staging::analysis_staging_counts(&worker_conn).unwrap();
        assert_eq!(index, 1);
        assert_eq!(
            staging::get_worker_meta(&worker_conn, "merged")
                .unwrap()
                .as_deref(),
            Some("false")
        );
    }

    #[test]
    fn analysis_worker_staging_open_creates_expected_tables() {
        let tmp = TempDir::new().unwrap();
        let conn = staging::open_analysis_staging(tmp.path(), "ds-1", 0).unwrap();
        for table in [
            "artifact_rows",
            "timeline_rows",
            "index_docs",
            "worker_meta",
        ] {
            let name: String = conn
                .query_row(
                    "SELECT name FROM sqlite_master WHERE type='table' AND name=?1",
                    params![table],
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(name, table);
        }
    }

    #[test]
    fn analysis_pool_respects_worker_limit_and_writes_temp_db() {
        let tmp = TempDir::new().unwrap();
        std::fs::create_dir_all(tmp.path().join("evidence")).unwrap();
        std::fs::write(tmp.path().join("evidence").join("a.txt"), "marker").unwrap();
        std::fs::write(tmp.path().join("evidence").join("b.txt"), "marker").unwrap();
        let (db_path, ds_id) = setup_case_db(&tmp);
        let conn = persistence_sqlite::open_or_create(&db_path).unwrap();
        insert_file(&conn, "f-a", &ds_id, "a.txt");
        insert_file(&conn, "f-b", &ds_id, "b.txt");
        drop(conn);

        let cancel = Arc::new(AtomicBool::new(false));
        let mut options = analysis_options(
            &tmp,
            db_path,
            ds_id.clone(),
            ImportAnalysisMode::BudgetedContent,
        );
        options.cancel_token = cancel;
        let stats = run_import_analysis_staging(options, None).unwrap();

        assert_eq!(stats.worker_ids, vec![0]);
        assert_eq!(stats.processed_count, 2);
        let worker_conn = staging::open_analysis_staging(tmp.path(), &ds_id.0, 0).unwrap();
        let (_artifacts, timeline, index) = staging::analysis_staging_counts(&worker_conn).unwrap();
        assert!(timeline > 0);
        assert!(index > 0);
        assert_eq!(
            staging::get_worker_meta(&worker_conn, "merged")
                .unwrap()
                .as_deref(),
            Some("true")
        );
    }

    #[test]
    fn analysis_worker_writes_only_own_temp_db() {
        let tmp = TempDir::new().unwrap();
        std::fs::create_dir_all(tmp.path().join("evidence")).unwrap();
        std::fs::write(tmp.path().join("evidence").join("a.txt"), "alpha").unwrap();
        let (db_path, ds_id) = setup_case_db(&tmp);
        let conn = persistence_sqlite::open_or_create(&db_path).unwrap();
        insert_file(&conn, "f-a", &ds_id, "a.txt");
        drop(conn);

        let cancel = Arc::new(AtomicBool::new(false));
        let mut options = analysis_options(
            &tmp,
            db_path,
            ds_id.clone(),
            ImportAnalysisMode::BudgetedContent,
        );
        options.cancel_token = cancel;
        let stats = run_import_analysis_staging(options, None).unwrap();

        assert_eq!(stats.worker_ids, vec![0]);
        assert!(staging::analysis_staging_db_path(tmp.path(), &ds_id.0, 0).exists());
        assert!(!staging::analysis_staging_db_path(tmp.path(), &ds_id.0, 1).exists());
    }

    #[test]
    fn analysis_tasks_include_title_case_file_entry_type() {
        let tmp = TempDir::new().unwrap();
        std::fs::create_dir_all(tmp.path().join("evidence")).unwrap();
        std::fs::write(tmp.path().join("evidence").join("a.txt"), "alpha").unwrap();
        let (db_path, ds_id) = setup_case_db(&tmp);
        let conn = persistence_sqlite::open_or_create(&db_path).unwrap();
        insert_file_with_type(&conn, "f-a", &ds_id, "a.txt", "File");
        drop(conn);

        assert_eq!(count_analysis_file_tasks(&db_path, &ds_id).unwrap(), 1);
        let page = fetch_analysis_file_page(
            &persistence_sqlite::open_or_create(&db_path).unwrap(),
            &ds_id,
            0,
            10,
        )
        .unwrap();
        assert_eq!(page.len(), 1);

        let stats = run_import_analysis_staging(
            analysis_options(
                &tmp,
                db_path,
                ds_id.clone(),
                ImportAnalysisMode::BudgetedContent,
            ),
            None,
        )
        .unwrap();
        assert_eq!(stats.processed_count, 1);
        assert!(stats.timeline_count > 0);
    }

    #[test]
    fn analysis_indexing_skips_large_or_unknown_extension_files() {
        let small_text = FileEntry {
            id: FileEntryId("small".to_string()),
            parent_id: None,
            data_source_id: DataSourceId("ds".to_string()),
            path: "small.txt".to_string(),
            name: "small.txt".to_string(),
            entry_type: EntryType::File,
            size: Some(512),
            ext: Some("txt".to_string()),
            deleted: false,
            created_at: None,
            modified_at: None,
            accessed_at: None,
            changed_at: None,
            hash_sha256: None,
        };
        let large_text = FileEntry {
            size: Some(infrastructure::constants::IMPORT_TEXT_INDEX_FILE_LIMIT_BYTES + 1),
            ..small_text.clone()
        };
        let unknown = FileEntry {
            path: "blob.bin".to_string(),
            name: "blob.bin".to_string(),
            ext: Some("bin".to_string()),
            ..small_text.clone()
        };

        assert!(should_index_file(&small_text));
        assert!(!should_index_file(&large_text));
        assert!(!should_index_file(&unknown));
    }

    #[test]
    fn analysis_artifact_extraction_skips_large_candidates() {
        let registry = artifact_service::create_registry();
        let small_prefetch = FileEntry {
            id: FileEntryId("small-pf".to_string()),
            parent_id: None,
            data_source_id: DataSourceId("ds".to_string()),
            path: "Windows/Prefetch/APP.EXE-12345678.pf".to_string(),
            name: "APP.EXE-12345678.pf".to_string(),
            entry_type: EntryType::File,
            size: Some(128 * 1024),
            ext: Some("pf".to_string()),
            deleted: false,
            created_at: None,
            modified_at: None,
            accessed_at: None,
            changed_at: None,
            hash_sha256: None,
        };
        let large_prefetch = FileEntry {
            size: Some(infrastructure::constants::IMPORT_TEXT_INDEX_FILE_LIMIT_BYTES + 1),
            ..small_prefetch.clone()
        };
        let non_candidate = FileEntry {
            path: "notes.txt".to_string(),
            name: "notes.txt".to_string(),
            ext: Some("txt".to_string()),
            ..small_prefetch.clone()
        };

        assert!(should_extract_artifact(&registry, &small_prefetch));
        assert!(!should_extract_artifact(&registry, &large_prefetch));
        assert!(!should_extract_artifact(&registry, &non_candidate));
    }

    #[test]
    fn disabled_import_content_reads_keep_analysis_bounded() {
        let tmp = TempDir::new().unwrap();
        std::fs::create_dir_all(tmp.path().join("evidence")).unwrap();
        let (db_path, ds_id) = setup_case_db(&tmp);
        let conn = persistence_sqlite::open_or_create(&db_path).unwrap();
        insert_file(&conn, "missing-text", &ds_id, "missing.txt");
        insert_file(
            &conn,
            "missing-pf",
            &ds_id,
            "Windows/Prefetch/MISSING.EXE-12345678.pf",
        );
        drop(conn);

        let stats = run_import_analysis_staging(
            analysis_options(
                &tmp,
                db_path,
                ds_id.clone(),
                ImportAnalysisMode::MetadataOnly,
            ),
            None,
        )
        .unwrap();

        assert_eq!(stats.processed_count, 2);
        assert!(stats.timeline_count > 0);
        assert_eq!(stats.artifact_count, 0);
        assert_eq!(stats.indexed_count, 0);
        assert_eq!(stats.warning_count, 0);
        assert_eq!(stats.skipped_count, 0);
    }

    #[test]
    fn analysis_warning_partial_semantics_are_preserved_after_startup_guard() {
        let tmp = TempDir::new().unwrap();
        std::fs::create_dir_all(tmp.path().join("evidence")).unwrap();
        let (db_path, ds_id) = setup_case_db(&tmp);
        let conn = persistence_sqlite::open_or_create(&db_path).unwrap();
        insert_file(&conn, "missing-text", &ds_id, "missing.txt");
        drop(conn);

        let stats = run_import_analysis_staging(
            analysis_options(
                &tmp,
                db_path,
                ds_id.clone(),
                ImportAnalysisMode::BudgetedContent,
            ),
            None,
        )
        .unwrap();

        assert_eq!(stats.processed_count, 1);
        assert_eq!(stats.warning_count, 1);
        assert_eq!(stats.skipped_count, 1);
        assert_eq!(stats.failed_count, 0);
        let worker_conn = staging::open_analysis_staging(tmp.path(), &ds_id.0, 0).unwrap();
        assert_eq!(
            staging::get_worker_meta(&worker_conn, "status")
                .unwrap()
                .as_deref(),
            Some("done")
        );
    }

    #[test]
    fn cancelled_analysis_keeps_cancel_error_and_unmerged_worker_status() {
        let tmp = TempDir::new().unwrap();
        std::fs::create_dir_all(tmp.path().join("evidence")).unwrap();
        let (db_path, ds_id) = setup_case_db(&tmp);
        let conn = persistence_sqlite::open_or_create(&db_path).unwrap();
        insert_file(&conn, "f-a", &ds_id, "a.txt");
        drop(conn);

        let cancel = Arc::new(AtomicBool::new(true));
        let mut options = analysis_options(
            &tmp,
            db_path,
            ds_id.clone(),
            ImportAnalysisMode::MetadataOnly,
        );
        options.cancel_token = cancel;

        let result = run_import_analysis_staging(options, None);

        assert!(matches!(result, Err(ref error) if error.contains("cancelled")));
        let worker_conn = staging::open_analysis_staging(tmp.path(), &ds_id.0, 0).unwrap();
        assert_eq!(
            staging::get_worker_meta(&worker_conn, "status")
                .unwrap()
                .as_deref(),
            Some("cancelled")
        );
        assert_eq!(
            staging::get_worker_meta(&worker_conn, "merged")
                .unwrap()
                .as_deref(),
            Some("false")
        );
    }

    #[test]
    fn producer_never_buffers_more_than_queue_bound() {
        assert_eq!(analysis_task_queue_bound(1), 256);
        assert_eq!(analysis_task_queue_bound(4), 1024);
    }

    #[test]
    fn content_budget_blocks_large_file_and_disabled_mode() {
        let shared = SharedAnalysisState::new();
        let file = FileEntry {
            id: FileEntryId("large".to_string()),
            parent_id: None,
            data_source_id: DataSourceId("ds".to_string()),
            path: "large.txt".to_string(),
            name: "large.txt".to_string(),
            entry_type: EntryType::File,
            size: Some(infrastructure::constants::IMPORT_TEXT_INDEX_FILE_LIMIT_BYTES + 1),
            ext: Some("txt".to_string()),
            deleted: false,
            created_at: None,
            modified_at: None,
            accessed_at: None,
            changed_at: None,
            hash_sha256: None,
        };

        assert!(!reserve_content_budget(
            &ContentBudget::disabled(),
            &file,
            &shared
        ));
        assert!(!reserve_content_budget(
            &ContentBudget::conservative(),
            &file,
            &shared
        ));
    }

    #[test]
    fn analysis_memory_guard_cancels_over_limit() {
        let tmp = TempDir::new().unwrap();
        let (db_path, ds_id) = setup_case_db(&tmp);
        let cancel = Arc::new(AtomicBool::new(false));
        let mut options = analysis_options(&tmp, db_path, ds_id, ImportAnalysisMode::MetadataOnly);
        options.cancel_token = cancel.clone();
        options.memory_soft_limit_mb = 1;
        options.memory_hard_limit_mb = 2;
        set_test_rss_override_mb(Some(3));

        let result = run_import_analysis_staging(options, None);
        set_test_rss_override_mb(None);

        assert!(result.is_err());
        assert!(cancel.load(Ordering::Relaxed));
    }
}
