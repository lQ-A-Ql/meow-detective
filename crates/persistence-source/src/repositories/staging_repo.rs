use persistence_sqlite::connection::DbResult;
use rusqlite::Connection;
use std::path::{Path, PathBuf};

const STAGING_DIR_NAME: &str = "staging";
const STAGING_CACHE_SIZE_KIB: i64 = 16 * 1024;
const STAGING_MMAP_SIZE_BYTES: i64 = 64 * 1024 * 1024;

pub struct StagingRepo;

pub const ANALYSIS_STAGING_SCHEMA: &str = r#"
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

fn staging_dir(case_root: &Path, data_source_id: &str) -> PathBuf {
    case_root.join(STAGING_DIR_NAME).join(data_source_id)
}

fn enum_staging_db_path(case_root: &Path, data_source_id: &str, partition_index: usize) -> PathBuf {
    staging_dir(case_root, data_source_id).join(format!("enum_partition_{partition_index}.db"))
}

fn analysis_staging_db_path(case_root: &Path, data_source_id: &str, worker_id: usize) -> PathBuf {
    staging_dir(case_root, data_source_id).join(format!("analysis_worker_{worker_id}.db"))
}

impl StagingRepo {
    pub fn open_partition_staging_conn(
        case_root: &Path,
        data_source_id: &str,
        partition_index: usize,
    ) -> DbResult<Connection> {
        let path = enum_staging_db_path(case_root, data_source_id, partition_index);
        let conn = open_staging_with_schema(
            &path,
            include_str!("../../../persistence-sqlite/src/migrations/scripts/staging_001.sql"),
        )?;
        persistence_sqlite::repositories::ntfs_staging_repo::NtfsStagingRepo::ensure_schema(&conn)?;
        schema::validate_enum_staging(&conn)?;
        Ok(conn)
    }

    pub fn open_analysis_staging_conn(
        case_root: &Path,
        data_source_id: &str,
        worker_id: usize,
    ) -> DbResult<Connection> {
        let path = analysis_staging_db_path(case_root, data_source_id, worker_id);
        let conn = open_staging_with_schema(&path, ANALYSIS_STAGING_SCHEMA)?;
        schema::validate_analysis_staging(&conn)?;
        Ok(conn)
    }

    pub fn staging_db_row_count(conn: &Connection) -> DbResult<u64> {
        let count: u64 =
            conn.query_row("SELECT COUNT(*) FROM file_entries", [], |row| row.get(0))?;
        Ok(count)
    }

    pub fn analysis_staging_counts(conn: &Connection) -> DbResult<(u64, u64, u64)> {
        let artifacts =
            conn.query_row("SELECT COUNT(*) FROM artifact_rows", [], |row| row.get(0))?;
        let timeline =
            conn.query_row("SELECT COUNT(*) FROM timeline_rows", [], |row| row.get(0))?;
        let index = conn.query_row("SELECT COUNT(*) FROM index_docs", [], |row| row.get(0))?;
        Ok((artifacts, timeline, index))
    }

    pub fn reset_partition_staging(conn: &Connection) -> DbResult<()> {
        persistence_sqlite::repositories::ntfs_staging_repo::NtfsStagingRepo::reset_layout(conn)?;
        conn.execute_batch(
            "DELETE FROM file_entries;
             DELETE FROM staging_meta
              WHERE key IN ('status', 'error', 'merged', 'mft_fallback_warning');",
        )?;
        Ok(())
    }

    pub fn merge_enum_staging_to_main(
        main_conn: &Connection,
        staging_conn: &Connection,
        data_source_id: &str,
        partition_index: usize,
        partition_name: &str,
    ) -> DbResult<u64> {
        crate::staging::merge_staging_catalog(
            main_conn,
            staging_conn,
            data_source_id,
            partition_index,
            partition_name,
        )
    }

    pub fn merge_enum_staging_to_main_in_transaction(
        main_conn: &rusqlite::Transaction<'_>,
        staging_conn: &Connection,
        data_source_id: &str,
        partition_index: usize,
        partition_name: &str,
    ) -> DbResult<u64> {
        Self::merge_enum_staging_to_main(
            main_conn,
            staging_conn,
            data_source_id,
            partition_index,
            partition_name,
        )
    }

    pub fn merge_analysis_staging_to_main_in_transaction_with_alias(
        main_conn: &rusqlite::Transaction<'_>,
        staging_conn: &Connection,
        case_id: &str,
        data_source_id: &str,
        _alias: &str,
    ) -> DbResult<(u64, u64)> {
        crate::staging::analysis::merge(main_conn, staging_conn, case_id, data_source_id)?
            .ok_or_else(|| {
                persistence_sqlite::DbError::System("source detail storage is missing".into())
            })
    }
}

fn open_staging_with_schema(path: &Path, schema: &str) -> DbResult<Connection> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let conn = Connection::open(path)?;
    conn.execute_batch(&format!(
        "PRAGMA page_size=8192;
         PRAGMA journal_mode=WAL;
         PRAGMA synchronous=OFF;
         PRAGMA temp_store=MEMORY;
         PRAGMA foreign_keys=ON;
         PRAGMA busy_timeout=5000;
         PRAGMA cache_size=-{STAGING_CACHE_SIZE_KIB};
         PRAGMA mmap_size={STAGING_MMAP_SIZE_BYTES};
         PRAGMA locking_mode=NORMAL;"
    ))?;
    conn.execute_batch(schema)?;
    Ok(conn)
}

mod analysis_index;
mod completion;
mod fallback;
mod metadata;
mod schema;
