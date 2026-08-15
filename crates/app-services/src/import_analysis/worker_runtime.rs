use super::budget::ContentBudget;
use super::error::ImportAnalysisError;
use super::extractor_policy::PlatformExtractorPolicy;
use super::options::{ImportAnalysisOptions, ImportAnalysisStats};
use super::search_policy::{
    mime_hint_for_entry, normalized_extension, search_budget_allows_file, should_index_file,
};
use super::source_reader::AnalysisSourceReader;
use super::worker_model::WorkerStats;
use super::worker_staging::{flush_worker_rows, persist_worker_stats, IndexDocRow};
use crate::{file_service, staging};
use artifacts_core::VecSink;
use crossbeam_channel::Receiver;
use domain::{DataSourceId, EntryType, FileEntry, FileEntryId};
use rusqlite::Connection;
use search::extract_text;
use std::io::Cursor;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;

const WORKER_INSERT_BATCH: usize = 200;
const INDEX_DOC_INSERT_BATCH: usize = 25;

#[derive(Debug, Clone)]
pub(super) struct FileTask {
    pub(super) id: FileEntryId,
    pub(super) data_source_id: DataSourceId,
    pub(super) path: String,
    pub(super) name: String,
    pub(super) entry_type: EntryType,
    pub(super) size: Option<u64>,
    pub(super) ext: Option<String>,
    pub(super) deleted: bool,
    pub(super) hidden: bool,
    pub(super) system: bool,
    pub(super) encrypted: bool,
    pub(super) created_at: Option<chrono::DateTime<chrono::Utc>>,
    pub(super) modified_at: Option<chrono::DateTime<chrono::Utc>>,
    pub(super) accessed_at: Option<chrono::DateTime<chrono::Utc>>,
    pub(super) changed_at: Option<chrono::DateTime<chrono::Utc>>,
    pub(super) hash_sha256: Option<String>,
}

impl FileTask {
    pub(crate) fn to_file_entry(&self) -> FileEntry {
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
            hidden: self.hidden,
            system: self.system,
            encrypted: self.encrypted,
            read_only: false,
            archive: false,
            unix_mode: None,
            created_at: self.created_at,
            modified_at: self.modified_at,
            accessed_at: self.accessed_at,
            changed_at: self.changed_at,
            hash_sha256: self.hash_sha256.clone(),
        }
    }
}

#[derive(Debug)]
pub(super) struct SharedAnalysisState {
    pub(super) processed_total: AtomicUsize,
    pub(super) active_workers: AtomicUsize,
    pub(super) indexed_total: AtomicUsize,
    pub(super) queued_total: AtomicUsize,
    pub(super) content_files_used: AtomicU64,
    pub(super) content_bytes_used: AtomicU64,
}

impl SharedAnalysisState {
    pub(super) fn new() -> Self {
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

pub(super) fn run_analysis_worker(
    worker_id: usize,
    options: ImportAnalysisOptions,
    derived_runtime: Option<Arc<crate::ceph_reconstruction::DerivedRbdRuntime>>,
    task_rx: Receiver<FileTask>,
    shared: Arc<SharedAnalysisState>,
) -> Result<WorkerStats, ImportAnalysisError> {
    let mut runtime = AnalysisWorkerRuntime::open(worker_id, &options, derived_runtime)?;
    while let Ok(task) = task_rx.recv() {
        if options.cancel_token.load(Ordering::Relaxed) {
            break;
        }
        runtime.process_task(task, &options, &shared)?;
    }
    runtime.finish(options.cancel_token.load(Ordering::Relaxed))
}

struct AnalysisWorkerRuntime {
    extractor_policy: PlatformExtractorPolicy,
    main_conn: Connection,
    staging_conn: Connection,
    stats: WorkerStats,
    source_reader: AnalysisSourceReader,
    artifacts: Vec<domain::Artifact>,
    timeline_events: Vec<domain::TimelineEvent>,
    index_docs: Vec<IndexDocRow>,
}

impl AnalysisWorkerRuntime {
    fn open(
        worker_id: usize,
        options: &ImportAnalysisOptions,
        derived_runtime: Option<Arc<crate::ceph_reconstruction::DerivedRbdRuntime>>,
    ) -> Result<Self, ImportAnalysisError> {
        let extractor_policy = PlatformExtractorPolicy::for_platform(options.platform)?;
        let main_conn = persistence_sqlite::open_existing_source_read_only(&options.db_path)?;
        let staging_conn = staging::open_analysis_staging(
            &options.case_root,
            &options.data_source_id.0,
            worker_id,
        )?;
        staging::set_worker_meta(&staging_conn, "status", "running")?;
        Ok(Self {
            extractor_policy,
            main_conn,
            staging_conn,
            stats: WorkerStats::default(),
            source_reader: AnalysisSourceReader::for_options(options, derived_runtime),
            artifacts: Vec::with_capacity(WORKER_INSERT_BATCH),
            timeline_events: Vec::with_capacity(WORKER_INSERT_BATCH),
            index_docs: Vec::with_capacity(INDEX_DOC_INSERT_BATCH),
        })
    }

    fn process_task(
        &mut self,
        task: FileTask,
        options: &ImportAnalysisOptions,
        shared: &SharedAnalysisState,
    ) -> Result<(), ImportAnalysisError> {
        let file = task.to_file_entry();
        self.stats.processed_count += 1;
        shared.processed_total.fetch_add(1, Ordering::Relaxed);
        self.extract_artifacts(&file, options, shared);
        self.index_text(&file, options, shared);
        self.flush_if_needed()
    }

    fn extract_artifacts(
        &mut self,
        file: &FileEntry,
        options: &ImportAnalysisOptions,
        shared: &SharedAnalysisState,
    ) {
        if options.analysis_mode.allows_content()
            && options.enable_content_extraction
            && self.extractor_policy.should_extract(file)
            && reserve_content_budget(&options.content_budget, file, shared)
        {
            match read_artifact_bytes(&mut self.source_reader, &self.main_conn, &file.id) {
                Ok(bytes) => {
                    let mut sink = VecSink::new();
                    match self.extractor_policy.run_extractors(
                        &file.id,
                        &file.path,
                        Box::new(Cursor::new(bytes)),
                        &mut sink,
                    ) {
                        Ok(extract_stats) => {
                            self.stats.warning_count = self
                                .stats
                                .warning_count
                                .saturating_add(extract_stats.warning_count);
                            self.stats.skipped_count = self
                                .stats
                                .skipped_count
                                .saturating_add(extract_stats.skipped_count);
                            self.stats.failed_count = self
                                .stats
                                .failed_count
                                .saturating_add(extract_stats.failed_count);
                        }
                        Err(error) => {
                            self.stats.warning_count = self.stats.warning_count.saturating_add(1);
                            self.stats.skipped_count = self.stats.skipped_count.saturating_add(1);
                            tracing::warn!(
                                "Artifact extraction failed for {}: {}",
                                file.path,
                                error
                            );
                        }
                    }
                    self.stats.artifact_count += sink.artifacts.len() as u64;
                    crate::timeline_service::retain_analysis_events(&mut sink.timeline_events);
                    self.stats.timeline_count += sink.timeline_events.len() as u64;
                    self.artifacts.extend(sink.artifacts);
                    self.timeline_events.extend(sink.timeline_events);
                }
                Err(error) => {
                    self.stats.warning_count = self.stats.warning_count.saturating_add(1);
                    self.stats.skipped_count = self.stats.skipped_count.saturating_add(1);
                    tracing::warn!(
                        "Artifact extraction skipped unreadable file {}: {}",
                        file.path,
                        error
                    );
                }
            }
        }
    }

    fn index_text(
        &mut self,
        file: &FileEntry,
        options: &ImportAnalysisOptions,
        shared: &SharedAnalysisState,
    ) {
        if options.enable_text_indexing
            && should_index_file(file, options.platform)
            && options.analysis_mode.allows_content()
            && search_budget_allows_file(&options.content_budget, file, options.platform)
            && reserve_content_quota(&options.content_budget, file, shared)
            && shared.indexed_total.load(Ordering::Relaxed)
                < infrastructure::constants::IMPORT_TEXT_INDEX_LIMIT
        {
            if let Ok(bytes) =
                read_text_index_bytes(&mut self.source_reader, &self.main_conn, &file.id)
            {
                let mime = mime_hint_for_entry(file, options.platform);
                let text = extract_text(Cursor::new(bytes), &file.id.0, mime);
                if text.extractable && !text.content.is_empty() {
                    let previous = shared.indexed_total.fetch_add(1, Ordering::Relaxed);
                    if previous < infrastructure::constants::IMPORT_TEXT_INDEX_LIMIT {
                        self.stats.indexed_count += 1;
                        self.index_docs.push(IndexDocRow {
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
                self.stats.warning_count = self.stats.warning_count.saturating_add(1);
                self.stats.skipped_count = self.stats.skipped_count.saturating_add(1);
            }
        }
    }

    fn flush_if_needed(&mut self) -> Result<(), ImportAnalysisError> {
        if self.artifacts.len() >= WORKER_INSERT_BATCH
            || self.timeline_events.len() >= WORKER_INSERT_BATCH
            || self.index_docs.len() >= INDEX_DOC_INSERT_BATCH
        {
            flush_worker_rows(
                &self.staging_conn,
                &mut self.artifacts,
                &mut self.timeline_events,
                &mut self.index_docs,
            )?;
            persist_worker_stats(&self.staging_conn, &self.stats)?;
        }
        Ok(())
    }

    fn finish(mut self, cancelled: bool) -> Result<WorkerStats, ImportAnalysisError> {
        flush_worker_rows(
            &self.staging_conn,
            &mut self.artifacts,
            &mut self.timeline_events,
            &mut self.index_docs,
        )?;
        persist_worker_stats(&self.staging_conn, &self.stats)?;
        let status = if cancelled { "cancelled" } else { "done" };
        staging::set_worker_meta(&self.staging_conn, "status", status)?;
        if cancelled {
            staging::set_worker_meta(&self.staging_conn, "error", "cancelled")?;
        }
        Ok(self.stats)
    }
}

fn read_text_index_bytes(
    source_reader: &mut AnalysisSourceReader,
    conn: &Connection,
    file_id: &FileEntryId,
) -> Result<Vec<u8>, file_service::FileServiceError> {
    read_text_index_bytes_impl(source_reader, conn, file_id)
}

fn read_artifact_bytes(
    source_reader: &mut AnalysisSourceReader,
    conn: &Connection,
    file_id: &FileEntryId,
) -> Result<Vec<u8>, file_service::FileServiceError> {
    read_artifact_bytes_impl(source_reader, conn, file_id)
}

fn read_artifact_bytes_impl(
    source_reader: &mut AnalysisSourceReader,
    conn: &Connection,
    file_id: &FileEntryId,
) -> Result<Vec<u8>, file_service::FileServiceError> {
    source_reader.read_file_header_by_id(
        conn,
        file_id,
        infrastructure::constants::ARTIFACT_FILE_LIMIT_BYTES as usize,
    )
}

fn read_text_index_bytes_impl(
    source_reader: &mut AnalysisSourceReader,
    conn: &Connection,
    file_id: &FileEntryId,
) -> Result<Vec<u8>, file_service::FileServiceError> {
    source_reader.read_file_header_by_id(
        conn,
        file_id,
        infrastructure::constants::IMPORT_TEXT_INDEX_FILE_LIMIT_BYTES as usize,
    )
}

pub(super) fn add_worker_stats(stats: &mut ImportAnalysisStats, worker: WorkerStats) {
    stats.processed_count += worker.processed_count;
    stats.artifact_count += worker.artifact_count;
    stats.timeline_count += worker.timeline_count;
    stats.indexed_count += worker.indexed_count;
    stats.warning_count = stats.warning_count.saturating_add(worker.warning_count);
    stats.skipped_count = stats.skipped_count.saturating_add(worker.skipped_count);
    stats.failed_count = stats.failed_count.saturating_add(worker.failed_count);
}

pub(super) fn reserve_content_budget(
    budget: &ContentBudget,
    file: &FileEntry,
    shared: &SharedAnalysisState,
) -> bool {
    if !budget.allowed_extensions.is_empty() {
        let ext = normalized_extension(file).to_ascii_lowercase();
        if !budget
            .allowed_extensions
            .iter()
            .any(|allowed| allowed == &ext)
        {
            return false;
        }
    }
    reserve_content_quota(budget, file, shared)
}

pub(super) fn reserve_content_quota(
    budget: &ContentBudget,
    file: &FileEntry,
    shared: &SharedAnalysisState,
) -> bool {
    let Some(size) = file.size else {
        return false;
    };
    if budget.max_files == 0
        || budget.max_bytes_total == 0
        || budget.max_bytes_per_file == 0
        || size > budget.max_bytes_per_file
    {
        return false;
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
