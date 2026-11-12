use rusqlite::Connection;

/// Diagnostic-only sample; absent/in-memory/non-WAL connections are unavailable.
pub(crate) fn sample_wal_size(conn: &Connection) -> Option<u64> {
    let path = conn
        .path()
        .filter(|path| !path.is_empty() && *path != ":memory:")?;
    std::fs::metadata(format!("{path}-wal"))
        .ok()
        .map(|metadata| metadata.len())
}
