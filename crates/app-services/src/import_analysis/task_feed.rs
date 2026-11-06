use super::error::ImportAnalysisError;
use super::extractor_policy::PlatformExtractorPolicy;
use super::options::ImportAnalysisOptions;
use super::priority_queue::{PriorityTaskQueue, TaskPriority};
use super::worker_runtime::{FileTask, SharedAnalysisState};
use crossbeam_channel::Sender;
use domain::{DataSourceId, EntryType, FileEntryId};
use persistence_sqlite::repositories::file_repo::file_encryption_status_from_row;
use rusqlite::{params, Connection};
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
    if let Some(store) = persistence_sqlite::source_fjall::open_store(&conn)? {
        let catalog = persistence_fjall::CatalogRecordStore::from_source_store((*store).clone());
        let snapshot = catalog.read_snapshot();
        let mut cursor: Option<(String, String)> = None;
        let mut total = 0;
        loop {
            let page = catalog
                .list_files_by_path(
                    &snapshot,
                    &data_source_id.0,
                    cursor
                        .as_ref()
                        .map(|(path, id)| (path.as_str(), id.as_str())),
                    FILE_PAGE_SIZE as usize,
                )
                .map_err(|error| ImportAnalysisError::Other(error.to_string()))?;
            total += page.records.len() as u64;
            if page.next_cursor.is_none() {
                return Ok(total);
            }
            cursor = page
                .records
                .last()
                .map(|record| (record.path.clone(), record.id.clone()));
        }
    }
    let count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM file_entries
             WHERE data_source_id = ?1 AND LOWER(entry_type) = 'file'",
        params![data_source_id.0],
        |row| row.get(0),
    )?;
    Ok(count as u64)
}

pub(super) fn fetch_analysis_file_page(
    conn: &Connection,
    data_source_id: &DataSourceId,
    after_path: Option<&str>,
    after_id: Option<&str>,
    limit: u64,
) -> Result<Vec<FileTask>, ImportAnalysisError> {
    if let Some(store) = persistence_sqlite::source_fjall::open_store(conn)? {
        let after = match (after_path, after_id) {
            (None, None) => None,
            (Some(path), Some(id)) => Some((path, id)),
            _ => {
                return Err(ImportAnalysisError::Other(
                    "analysis file cursor requires both path and id".into(),
                ))
            }
        };
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
        return Ok(page
            .records
            .into_iter()
            .map(file_task_from_record)
            .collect());
    }
    let (sql, cursor) = match (after_path, after_id) {
        (None, None) => (
            "SELECT id, parent_id, data_source_id, path, name, entry_type,
                    size, ext, deleted, hidden, system, created_at, modified_at, accessed_at, changed_at, hash_sha256, encrypted
             FROM file_entries
             WHERE data_source_id = ?1 AND LOWER(entry_type) = 'file'
             ORDER BY path ASC, id ASC
             LIMIT ?2",
            None,
        ),
        (Some(path), Some(id)) => (
            "SELECT id, parent_id, data_source_id, path, name, entry_type,
                    size, ext, deleted, hidden, system, created_at, modified_at, accessed_at, changed_at, hash_sha256, encrypted
             FROM file_entries
             WHERE data_source_id = ?1 AND LOWER(entry_type) = 'file'
               AND (path > ?2 OR (path = ?2 AND id > ?3))
             ORDER BY path ASC, id ASC
             LIMIT ?4",
            Some((path, id)),
        ),
        _ => {
            return Err(ImportAnalysisError::Other(
                "analysis file cursor requires both path and id".to_string(),
            ))
        }
    };
    let mut stmt = conn.prepare(sql)?;
    let mut rows = match cursor {
        Some((path, id)) => stmt.query(params![data_source_id.0, path, id, limit])?,
        None => stmt.query(params![data_source_id.0, limit])?,
    };
    let mut files = Vec::new();
    while let Some(row) = rows.next()? {
        files.push(row_to_file_task(row)?);
    }
    Ok(files)
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

fn row_to_file_task(row: &rusqlite::Row<'_>) -> rusqlite::Result<FileTask> {
    let entry_type_str: String = row.get(5)?;
    let encryption_status = file_encryption_status_from_row(row, 16)?;
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
        hidden: row.get::<_, i32>(9)? != 0,
        system: row.get::<_, i32>(10)? != 0,
        encrypted: encryption_status.blocks_content(),
        created_at: row
            .get::<_, Option<String>>(11)?
            .and_then(|s| chrono::DateTime::parse_from_rfc3339(&s).ok())
            .map(|dt| dt.with_timezone(&chrono::Utc)),
        modified_at: row
            .get::<_, Option<String>>(12)?
            .and_then(|s| chrono::DateTime::parse_from_rfc3339(&s).ok())
            .map(|dt| dt.with_timezone(&chrono::Utc)),
        accessed_at: row
            .get::<_, Option<String>>(13)?
            .and_then(|s| chrono::DateTime::parse_from_rfc3339(&s).ok())
            .map(|dt| dt.with_timezone(&chrono::Utc)),
        changed_at: row
            .get::<_, Option<String>>(14)?
            .and_then(|s| chrono::DateTime::parse_from_rfc3339(&s).ok())
            .map(|dt| dt.with_timezone(&chrono::Utc)),
        hash_sha256: row.get(15)?,
    })
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
