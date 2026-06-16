use super::budget::ContentBudget;
use super::options::{ImportAnalysisOptions, ImportAnalysisStats};
use crate::{artifact_service, file_service, staging};
use artifacts_core::VecSink;
use crossbeam_channel::Receiver;
use domain::{DataSourceId, EntryType, FileEntry, FileEntryId};
use rusqlite::{params, Connection};
use search::extract_text;
use std::io::Read;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;

const WORKER_INSERT_BATCH: usize = 200;
const INDEX_DOC_INSERT_BATCH: usize = 25;

#[derive(Debug, Clone, Default)]
pub(super) struct WorkerStats {
    pub(super) processed_count: u64,
    pub(super) artifact_count: u64,
    pub(super) timeline_count: u64,
    pub(super) indexed_count: u64,
    pub(super) warning_count: u32,
    pub(super) skipped_count: u32,
    pub(super) failed_count: u32,
}

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

pub(super) fn clear_analysis_worker_rows(conn: &Connection) -> rusqlite::Result<()> {
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

pub(super) fn add_worker_stats(stats: &mut ImportAnalysisStats, worker: WorkerStats) {
    stats.processed_count += worker.processed_count;
    stats.artifact_count += worker.artifact_count;
    stats.timeline_count += worker.timeline_count;
    stats.indexed_count += worker.indexed_count;
    stats.warning_count = stats.warning_count.saturating_add(worker.warning_count);
    stats.skipped_count = stats.skipped_count.saturating_add(worker.skipped_count);
    stats.failed_count = stats.failed_count.saturating_add(worker.failed_count);
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

pub(super) fn should_index_file(file: &FileEntry) -> bool {
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

pub(crate) fn should_extract_artifact(
    registry: &artifacts_core::ExtractorRegistry,
    file: &FileEntry,
) -> bool {
    if registry.find_for_path(&file.path).is_empty() {
        return false;
    }
    file.size
        .is_some_and(|size| size <= infrastructure::constants::IMPORT_TEXT_INDEX_FILE_LIMIT_BYTES)
}

pub(super) fn reserve_content_budget(
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
