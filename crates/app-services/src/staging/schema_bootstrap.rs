use super::db_paths::{analysis_staging_db_path, existing_enum_staging_db_path};
use persistence_sqlite::DbResult;
use rusqlite::{params, Connection};
use std::path::Path;

pub(super) const STAGING_CACHE_SIZE_KIB: i64 = 256 * 1024;
const STAGING_MMAP_SIZE_BYTES: i64 = 256 * 1024 * 1024;

/// Open (or create) a staging DB for a partition.
pub fn open_partition_staging(
    case_root: &Path,
    data_source_id: &str,
    partition_index: usize,
) -> DbResult<Connection> {
    open_enum_staging(case_root, data_source_id, partition_index)
}

/// Open (or create) an enumeration staging DB for a partition.
pub fn open_enum_staging(
    case_root: &Path,
    data_source_id: &str,
    partition_index: usize,
) -> DbResult<Connection> {
    let path = existing_enum_staging_db_path(case_root, data_source_id, partition_index);
    let conn = open_staging_with_schema(
        &path,
        include_str!("../../../persistence-sqlite/src/migrations/scripts/staging_001.sql"),
    )?;
    ensure_enum_staging_visibility_columns(&conn)?;
    Ok(conn)
}

/// Open (or create) an analysis staging DB for a worker.
pub fn open_analysis_staging(
    case_root: &Path,
    data_source_id: &str,
    worker_id: usize,
) -> DbResult<Connection> {
    let path = analysis_staging_db_path(case_root, data_source_id, worker_id);
    let conn = open_staging_with_schema(&path, ANALYSIS_STAGING_SCHEMA)?;
    ensure_analysis_staging_provenance_columns(&conn)?;
    Ok(conn)
}

fn open_staging_with_schema(path: &Path, schema: &str) -> DbResult<Connection> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let conn = Connection::open(path)?;
    apply_staging_connection_pragmas(&conn)?;
    conn.execute_batch(schema)?;
    Ok(conn)
}

fn apply_staging_connection_pragmas(conn: &Connection) -> DbResult<()> {
    conn.execute_batch(&format!(
        "PRAGMA journal_mode=WAL;
         PRAGMA synchronous=OFF;
         PRAGMA temp_store=MEMORY;
         PRAGMA foreign_keys=ON;
         PRAGMA busy_timeout=5000;
         PRAGMA cache_size=-{STAGING_CACHE_SIZE_KIB};
         PRAGMA mmap_size={STAGING_MMAP_SIZE_BYTES};"
    ))?;
    Ok(())
}

fn ensure_analysis_staging_provenance_columns(conn: &Connection) -> DbResult<()> {
    for (table, column, sql_type) in [
        ("artifact_rows", "extractor_id", "TEXT"),
        ("artifact_rows", "extractor_version", "TEXT"),
        ("artifact_rows", "confidence", "REAL"),
        ("artifact_rows", "source_attribution", "TEXT"),
        ("timeline_rows", "parser_id", "TEXT"),
        ("timeline_rows", "parser_version", "TEXT"),
        ("timeline_rows", "confidence", "REAL"),
        ("timeline_rows", "source_attribution", "TEXT"),
    ] {
        if !table_has_column(conn, table, column)? {
            conn.execute(
                &format!("ALTER TABLE {table} ADD COLUMN {column} {sql_type}"),
                [],
            )?;
        }
    }
    Ok(())
}

fn ensure_enum_staging_visibility_columns(conn: &Connection) -> DbResult<()> {
    for column in ["hidden", "system"] {
        if !table_has_column(conn, "file_entries", column)? {
            conn.execute(
                &format!("ALTER TABLE file_entries ADD COLUMN {column} INTEGER NOT NULL DEFAULT 0"),
                [],
            )?;
        }
    }
    Ok(())
}

pub(super) fn table_has_column(conn: &Connection, table: &str, column: &str) -> DbResult<bool> {
    conn.query_row(
        &format!("SELECT COUNT(*) > 0 FROM pragma_table_info('{table}') WHERE name = ?1"),
        [column],
        |row| row.get(0),
    )
    .map_err(Into::into)
}

/// Get the count of rows in a staging DB.
pub fn staging_db_row_count(conn: &Connection) -> DbResult<u64> {
    let count: i64 = conn.query_row("SELECT COUNT(*) FROM file_entries", [], |row| row.get(0))?;
    Ok(count as u64)
}

/// Set a metadata key in a staging DB.
pub fn set_staging_meta(conn: &Connection, key: &str, value: &str) -> DbResult<()> {
    conn.execute(
        "INSERT OR REPLACE INTO staging_meta (key, value) VALUES (?1, ?2)",
        params![key, value],
    )?;
    Ok(())
}

/// Get a metadata key from a staging DB.
pub fn get_staging_meta(conn: &Connection, key: &str) -> DbResult<Option<String>> {
    let mut stmt = conn.prepare("SELECT value FROM staging_meta WHERE key = ?1")?;
    let mut rows = stmt.query_map(params![key], |row| row.get::<_, String>(0))?;
    match rows.next() {
        Some(Ok(v)) => Ok(Some(v)),
        _ => Ok(None),
    }
}

/// Set a metadata key in an analysis staging DB.
pub fn set_worker_meta(conn: &Connection, key: &str, value: &str) -> DbResult<()> {
    conn.execute(
        "INSERT OR REPLACE INTO worker_meta (key, value) VALUES (?1, ?2)",
        params![key, value],
    )?;
    Ok(())
}

/// Get a metadata key from an analysis staging DB.
pub fn get_worker_meta(conn: &Connection, key: &str) -> DbResult<Option<String>> {
    let mut stmt = conn.prepare("SELECT value FROM worker_meta WHERE key = ?1")?;
    let mut rows = stmt.query_map(params![key], |row| row.get::<_, String>(0))?;
    match rows.next() {
        Some(Ok(v)) => Ok(Some(v)),
        _ => Ok(None),
    }
}

pub fn analysis_staging_counts(conn: &Connection) -> DbResult<(u64, u64, u64)> {
    let artifact_count: i64 =
        conn.query_row("SELECT COUNT(*) FROM artifact_rows", [], |row| row.get(0))?;
    let timeline_count: i64 =
        conn.query_row("SELECT COUNT(*) FROM timeline_rows", [], |row| row.get(0))?;
    let index_count: i64 =
        conn.query_row("SELECT COUNT(*) FROM index_docs", [], |row| row.get(0))?;
    Ok((
        artifact_count as u64,
        timeline_count as u64,
        index_count as u64,
    ))
}

const ANALYSIS_STAGING_SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS artifact_rows (
    id TEXT PRIMARY KEY NOT NULL,
    file_id TEXT,
    artifact_type TEXT NOT NULL,
    extractor_id TEXT,
    extractor_version TEXT,
    confidence REAL,
    source_attribution TEXT,
    display_name TEXT NOT NULL,
    summary TEXT NOT NULL DEFAULT '',
    data_json TEXT NOT NULL DEFAULT '{}',
    source_path TEXT NOT NULL DEFAULT '',
    created_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS timeline_rows (
    id TEXT PRIMARY KEY NOT NULL,
    file_id TEXT NOT NULL,
    timestamp TEXT NOT NULL,
    event_type TEXT NOT NULL,
    parser_id TEXT,
    parser_version TEXT,
    confidence REAL,
    source_attribution TEXT,
    title TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    data_json TEXT NOT NULL DEFAULT '{}'
);

CREATE TABLE IF NOT EXISTS index_docs (
    file_id TEXT PRIMARY KEY NOT NULL,
    path TEXT NOT NULL,
    text TEXT NOT NULL,
    language TEXT NOT NULL DEFAULT 'unknown',
    truncated INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE IF NOT EXISTS worker_meta (
    key TEXT PRIMARY KEY NOT NULL,
    value TEXT NOT NULL
);
"#;
