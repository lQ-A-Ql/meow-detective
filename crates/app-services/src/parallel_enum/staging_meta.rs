pub(super) fn clear_staging_file_entries(conn: &rusqlite::Connection) -> rusqlite::Result<()> {
    conn.execute_batch("DELETE FROM file_entries")
}

/// Get the opt-in upper bound for explicit worker settings.
pub fn default_worker_count() -> usize {
    std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4)
}

/// Resolve worker count from settings.
///
/// `None` defaults to one worker because evidence-image imports are heavy I/O
/// workloads and each source DB has a single writer.
pub fn resolve_worker_count(max_import_workers: Option<usize>) -> usize {
    match max_import_workers {
        Some(n) if n > 0 => n.min(default_worker_count()),
        _ => 1,
    }
}
