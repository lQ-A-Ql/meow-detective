use super::error::ImportAnalysisError;
use super::extractor_policy::PlatformExtractorPolicy;
use super::options::ImportAnalysisOptions;
use super::priority_queue::{PriorityTaskQueue, TaskPriority};
use super::worker_runtime::{FileTask, SharedAnalysisState};
use crossbeam_channel::Sender;
use domain::{DataSourceId, EntryType, FileEntryId};
use rusqlite::Connection;
use std::path::Path;
use std::sync::atomic::Ordering;
use std::sync::Arc;

const TASKS_PER_WORKER_BOUND: usize = 256;
pub(super) const FILE_PAGE_SIZE: u64 = 750;

pub(super) fn analysis_task_queue_bound(worker_count: usize) -> usize {
    worker_count.max(1) * TASKS_PER_WORKER_BOUND
}

pub(super) fn count_analysis_file_tasks(
    db_path: &Path,
    data_source_id: &DataSourceId,
) -> Result<u64, ImportAnalysisError> {
    let conn = persistence_sqlite::open_existing_source_read_only(db_path)?;
    let store = persistence_sqlite::source_fjall::open_store(&conn)?.ok_or_else(|| {
        ImportAnalysisError::Other("source detail storage is missing".to_string())
    })?;
    let catalog = persistence_fjall::CatalogRecordStore::from_source_store((*store).clone());
    catalog
        .file_count(&catalog.read_snapshot(), &data_source_id.0)
        .map_err(|error| ImportAnalysisError::Other(error.to_string()))
}

pub(super) fn fetch_analysis_file_page(
    conn: &Connection,
    data_source_id: &DataSourceId,
    after_path: Option<&str>,
    after_id: Option<&str>,
    limit: u64,
) -> Result<Vec<FileTask>, ImportAnalysisError> {
    let after = match (after_path, after_id) {
        (None, None) => None,
        (Some(path), Some(id)) => Some((path, id)),
        _ => {
            return Err(ImportAnalysisError::Other(
                "analysis file cursor requires both path and id".into(),
            ))
        }
    };
    let store = persistence_sqlite::source_fjall::open_store(conn)?.ok_or_else(|| {
        ImportAnalysisError::Other("source detail storage is missing".to_string())
    })?;
    let catalog = persistence_fjall::CatalogRecordStore::from_source_store((*store).clone());
    let page = catalog
        .list_files_by_path(
            &catalog.read_snapshot(),
            &data_source_id.0,
            after,
            usize::try_from(limit.min(FILE_PAGE_SIZE))
                .map_err(|error| ImportAnalysisError::Other(error.to_string()))?,
        )
        .map_err(|error| ImportAnalysisError::Other(error.to_string()))?;
    Ok(page
        .records
        .into_iter()
        .map(file_task_from_record)
        .collect())
}

/// Enqueue file tasks with priority classification.
///
/// Each file row is classified and pushed with the appropriate
/// [`TaskPriority`]:
///
/// | Condition | Priority |
/// |---|---|
/// | File matches an artifact extractor and is a content candidate | [`TaskPriority::Normal`] |
/// | All other files | [`TaskPriority::Low`] |
///
/// Artifact-candidate files are processed before plain enumeration tasks.
/// When derived-analysis tasks are added in a future iteration they will
/// be pushed at [`TaskPriority::High`].
pub(super) fn enqueue_analysis_tasks_prioritized(
    options: &ImportAnalysisOptions,
    task_tx: &Sender<FileTask>,
    shared: Arc<SharedAnalysisState>,
) -> Result<(), ImportAnalysisError> {
    let extractor_policy = PlatformExtractorPolicy::for_platform(options.platform)?;
    let conn = persistence_sqlite::open_existing_source_read_only(&options.db_path)?;
    let mut cursor: Option<(String, String)> = None;

    loop {
        if options.cancel_token.load(Ordering::Relaxed) {
            break;
        }
        let page = fetch_analysis_file_page(
            &conn,
            &options.data_source_id,
            cursor.as_ref().map(|value| value.0.as_str()),
            cursor.as_ref().map(|value| value.1.as_str()),
            FILE_PAGE_SIZE,
        )?;
        if page.is_empty() {
            break;
        }
        cursor = page
            .last()
            .map(|file| (file.path.clone(), file.id.0.clone()));

        let mut pq = PriorityTaskQueue::new();
        for file in page {
            if options.cancel_token.load(Ordering::Relaxed) {
                break;
            }
            let entry = file.to_file_entry();
            let priority = if extractor_policy.should_extract(&entry) {
                // Artifact-candidate files get Normal priority so they are
                // processed before plain file enumeration tasks.
                TaskPriority::Normal
            } else {
                TaskPriority::Low
            };
            pq.push(file, priority);
        }

        // Drain the priority queue into the channel (high → normal → low).
        // This preserves backpressure on the bounded channel.
        while let Some(task) = pq.pop() {
            task_tx
                .send(task)
                .map_err(|e| ImportAnalysisError::Other(format!("Queue analysis task: {e}")))?;
            shared.queued_total.fetch_add(1, Ordering::Relaxed);
        }
    }
    Ok(())
}

fn file_task_from_record(record: persistence_fjall::FileEntryRecord) -> FileTask {
    FileTask {
        id: FileEntryId(record.id),
        data_source_id: DataSourceId(record.data_source_id),
        path: record.path,
        name: record.name,
        entry_type: EntryType::File,
        size: record.size,
        ext: record.ext,
        deleted: record.deleted,
        hidden: record.hidden,
        system: record.system,
        encrypted: record.encrypted.unwrap_or(true),
        created_at: record.created,
        modified_at: record.modified,
        accessed_at: record.accessed,
        changed_at: record.changed,
        hash_sha256: record.hash_sha256,
    }
}
