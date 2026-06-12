use super::options::ImportAnalysisOptions;
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

pub(super) fn enqueue_analysis_tasks(
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

pub(super) fn count_analysis_file_tasks(
    db_path: &Path,
    data_source_id: &DataSourceId,
) -> Result<u64, String> {
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

pub(super) fn fetch_analysis_file_page(
    conn: &Connection,
    data_source_id: &DataSourceId,
    offset: u64,
    limit: u64,
) -> Result<Vec<FileTask>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT id, parent_id, data_source_id, path, name, entry_type,
                    size, ext, deleted, hidden, system, created_at, modified_at, accessed_at, changed_at, hash_sha256
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
        hidden: row.get::<_, i32>(9)? != 0,
        system: row.get::<_, i32>(10)? != 0,
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
