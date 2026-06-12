pub(super) fn clear_staging_file_entries(conn: &rusqlite::Connection) -> rusqlite::Result<()> {
    conn.execute_batch("DELETE FROM file_entries")
}

/// Get the default number of workers (all logical cores).
pub fn default_worker_count() -> usize {
    std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4)
}

/// Resolve worker count from settings or default.
pub fn resolve_worker_count(max_import_workers: Option<usize>) -> usize {
    match max_import_workers {
        Some(n) if n > 0 => n.min(default_worker_count()),
        _ => default_worker_count(),
    }
}
