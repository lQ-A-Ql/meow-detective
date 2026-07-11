use super::error::ImportAnalysisError;
use super::extractor_policy::PlatformExtractorPolicy;
use super::options::ImportAnalysisOptions;
use super::priority_queue::{PriorityTaskQueue, TaskPriority};
use super::worker_runtime::{FileTask, SharedAnalysisState};
use crossbeam_channel::Sender;
use domain::{DataSourceId, EntryType, FileEntryId};
use rusqlite::{params, Connection};
use std::path::Path;
use std::sync::atomic::Ordering;
use std::sync::Arc;

const TASKS_PER_WORKER_BOUND: usize = 256;
const FILE_PAGE_SIZE: u64 = 750;

pub(super) fn analysis_task_queue_bound(worker_count: usize) -> usize {
    worker_count.max(1) * TASKS_PER_WORKER_BOUND
}

pub(super) fn count_analysis_file_tasks(
    db_path: &Path,
    data_source_id: &DataSourceId,
) -> Result<u64, ImportAnalysisError> {
    let conn = persistence_sqlite::open_or_create(db_path)?;
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
    offset: u64,
    limit: u64,
) -> Result<Vec<FileTask>, ImportAnalysisError> {
    let mut stmt = conn
        .prepare(
            "SELECT id, parent_id, data_source_id, path, name, entry_type,
                    size, ext, deleted, hidden, system, created_at, modified_at, accessed_at, changed_at, hash_sha256
             FROM file_entries
             WHERE data_source_id = ?1 AND LOWER(entry_type) = 'file'
             ORDER BY path ASC
             LIMIT ?2 OFFSET ?3",
        )?;
    let rows = stmt.query_map(params![data_source_id.0, limit, offset], row_to_file_task)?;
    let mut files = Vec::new();
    for row in rows {
        files.push(row?);
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
    let conn = persistence_sqlite::open_or_create(&options.db_path)?;
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

        offset += FILE_PAGE_SIZE;
    }
    Ok(())
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
        hidden: row.get::<_, i32>(9)? != 0,
        system: row.get::<_, i32>(10)? != 0,
        encrypted: false,
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
