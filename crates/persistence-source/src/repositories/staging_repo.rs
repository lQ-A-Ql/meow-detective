use persistence_sqlite::connection::DbResult;
use rusqlite::{params, Connection};
use std::path::{Path, PathBuf};

/// Directory name for staging databases inside a case directory.
const STAGING_DIR_NAME: &str = "staging";

pub struct StagingRepo;

const STAGING_CACHE_SIZE_KIB: i64 = 16 * 1024;
const STAGING_MMAP_SIZE_BYTES: i64 = 64 * 1024 * 1024;

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

// ---------------------------------------------------------------------------
// Path construction
// ---------------------------------------------------------------------------

fn staging_dir(case_root: &Path, data_source_id: &str) -> PathBuf {
    case_root.join(STAGING_DIR_NAME).join(data_source_id)
}

fn enum_staging_db_path(case_root: &Path, data_source_id: &str, partition_index: usize) -> PathBuf {
    staging_dir(case_root, data_source_id).join(format!("enum_partition_{}.db", partition_index))
}

fn analysis_staging_db_path(case_root: &Path, data_source_id: &str, worker_id: usize) -> PathBuf {
    staging_dir(case_root, data_source_id).join(format!("analysis_worker_{}.db", worker_id))
}

// ---------------------------------------------------------------------------
// Connection opening
// ---------------------------------------------------------------------------

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
        persistence_sqlite::repositories::ntfs_staging_repo::NtfsStagingRepo::rebuild_previous_layout(&conn)?;
        Self::ensure_enum_staging_columns(&conn)?;
        persistence_sqlite::repositories::ntfs_staging_repo::NtfsStagingRepo::ensure_schema(&conn)?;
        Ok(conn)
    }

    pub fn open_analysis_staging_conn(
        case_root: &Path,
        data_source_id: &str,
        worker_id: usize,
    ) -> DbResult<Connection> {
        let path = analysis_staging_db_path(case_root, data_source_id, worker_id);
        let conn = open_staging_with_schema(&path, ANALYSIS_STAGING_SCHEMA)?;
        Self::ensure_analysis_staging_columns(&conn)?;
        Ok(conn)
    }
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
        "PRAGMA page_size=8192;
         PRAGMA journal_mode=WAL;
         PRAGMA synchronous=OFF;
         PRAGMA temp_store=MEMORY;
         PRAGMA foreign_keys=ON;
         PRAGMA busy_timeout=5000;
         PRAGMA cache_size=-{STAGING_CACHE_SIZE_KIB};
         PRAGMA mmap_size={STAGING_MMAP_SIZE_BYTES};
         -- The merge phase ATTACHes this database from the source connection.
         -- Keep normal locking so an idle staging handle does not retain an
         -- exclusive lock while its file is being folded into source.db.
         PRAGMA locking_mode=NORMAL;"
    ))?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Row counts
// ---------------------------------------------------------------------------

impl StagingRepo {
    pub fn staging_db_row_count(conn: &Connection) -> DbResult<u64> {
        let count: i64 =
            conn.query_row("SELECT COUNT(*) FROM file_entries", [], |row| row.get(0))?;
        Ok(count as u64)
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
}

// ---------------------------------------------------------------------------
// Metadata
// ---------------------------------------------------------------------------

impl StagingRepo {
    pub fn reset_partition_staging(conn: &Connection) -> DbResult<()> {
        persistence_sqlite::repositories::ntfs_staging_repo::NtfsStagingRepo::reset_layout(conn)?;
        conn.execute_batch(
            "DELETE FROM file_entries;
             DELETE FROM staging_meta
              WHERE key IN ('status', 'error', 'merged', 'mft_fallback_warning');",
        )?;
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Placeholder helpers
// ---------------------------------------------------------------------------

impl StagingRepo {
    pub fn find_partition_placeholder_root_id_by_index(
        conn: &Connection,
        data_source_id: &str,
        partition_index: usize,
    ) -> rusqlite::Result<Option<String>> {
        let pattern = format!("__partition_placeholder__/{partition_index}/*");
        match conn.query_row(
            "SELECT id FROM file_entries WHERE data_source_id = ?1 AND parent_id IS NULL AND path GLOB ?2 LIMIT 1",
            params![data_source_id, pattern],
            |row| row.get(0),
        ) {
            Ok(id) => Ok(Some(id)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(other) => Err(other),
        }
    }

    pub fn promote_partition_placeholder_root(
        conn: &Connection,
        root_id: &str,
        partition_name: &str,
        partition_index: usize,
    ) -> rusqlite::Result<()> {
        conn.execute(
            "UPDATE file_entries
             SET path = '', name = ?2, partition_index = ?3
             WHERE id = ?1",
            params![root_id, partition_name, partition_index as i64],
        )?;
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Enumeration merge
// ---------------------------------------------------------------------------

impl StagingRepo {
    /// Merge rows from a staging database into main file_entries, resolving
    /// or synthesizing a partition placeholder root atomically inside the
    /// same transaction so a failed merge rolls the placeholder back too.
    pub fn merge_enum_staging_to_main(
        main_conn: &Connection,
        staging_conn: &Connection,
        data_source_id: &str,
        partition_index: usize,
        partition_name: &str,
    ) -> rusqlite::Result<u64> {
        if let Some(count) = crate::staging::merge_staging_catalog(
            main_conn,
            staging_conn,
            data_source_id,
            partition_index,
            partition_name,
        )
        .map_err(|error| rusqlite::Error::ToSqlConversionFailure(Box::new(error)))?
        {
            return Ok(count);
        }
        super::staging_merge_validation::validate_enum_merge_target(main_conn)?;
        if persistence_sqlite::repositories::ntfs_staging_repo::NtfsStagingRepo::is_numeric(
            staging_conn,
        )? {
            persistence_sqlite::repositories::ntfs_staging_repo::NtfsStagingRepo::validate_paths(
                staging_conn,
            )?;
        }
        let staging_path = staging_db_file_path(staging_conn)?;
        let escaped = staging_path.replace('\'', "''");
        main_conn.execute_batch(&format!("ATTACH DATABASE '{}' AS staging", escaped))?;
        main_conn.execute_batch("BEGIN IMMEDIATE")?;
        let result = (|| {
            let existing = Self::find_partition_placeholder_root_id_by_index(
                main_conn,
                data_source_id,
                partition_index,
            )?;
            let placeholder_root_id = match existing {
                Some(id) => id,
                None => {
                    let id = uuid::Uuid::new_v4().to_string();
                    main_conn.execute(
                        "INSERT INTO main.file_entries
                         (id, parent_id, data_source_id, path, name, entry_type, encrypted, partition_index)
                         VALUES (?1, NULL, ?2, ?3, ?4, 'directory', 0, ?5)",
                        params![
                            id,
                            data_source_id,
                            format!("__partition_placeholder__/{partition_index}/queued"),
                            partition_name,
                            partition_index as i64,
                        ],
                    )?;
                    id
                }
            };
            Self::promote_partition_placeholder_root(
                main_conn,
                &placeholder_root_id,
                partition_name,
                partition_index,
            )?;
            let inserted =
                match persistence_sqlite::repositories::ntfs_staging_merge::merge_completed_numeric(
                    main_conn,
                    staging_conn,
                    data_source_id,
                    partition_index,
                    &placeholder_root_id,
                )? {
                    Some(rows) => rows,
                    None => main_conn.execute(
                        include_str!("staging_enum_merge.sql"),
                        params![placeholder_root_id, partition_index as i64],
                    )? as u64,
                };
            main_conn.execute_batch("COMMIT")?;
            Ok(inserted)
        })();
        if result.is_err() {
            let _ = main_conn.execute_batch("ROLLBACK");
        }
        let _ = main_conn.execute_batch("DETACH DATABASE staging");
        result
    }

    pub fn merge_enum_staging_to_main_in_transaction(
        main_conn: &rusqlite::Transaction<'_>,
        staging_conn: &Connection,
        data_source_id: &str,
        partition_index: usize,
        partition_name: &str,
    ) -> rusqlite::Result<u64> {
        if let Some(count) = crate::staging::merge_staging_catalog(
            main_conn,
            staging_conn,
            data_source_id,
            partition_index,
            partition_name,
        )
        .map_err(|error| rusqlite::Error::ToSqlConversionFailure(Box::new(error)))?
        {
            return Ok(count);
        }
        super::staging_merge_validation::validate_enum_merge_target(main_conn)?;
        if persistence_sqlite::repositories::ntfs_staging_repo::NtfsStagingRepo::is_numeric(
            staging_conn,
        )? {
            persistence_sqlite::repositories::ntfs_staging_repo::NtfsStagingRepo::validate_paths(
                staging_conn,
            )?;
        }
        let staging_path = staging_db_file_path(staging_conn)?;
        let escaped = staging_path.replace('\'', "''");
        main_conn.execute_batch(&format!("ATTACH DATABASE '{}' AS staging", escaped))?;
        let result = (|| {
            let existing = Self::find_partition_placeholder_root_id_by_index(
                main_conn,
                data_source_id,
                partition_index,
            )?;
            let placeholder_root_id = match existing {
                Some(id) => id,
                None => {
                    let id = uuid::Uuid::new_v4().to_string();
                    main_conn.execute(
                        "INSERT INTO main.file_entries
                         (id, parent_id, data_source_id, path, name, entry_type, encrypted, partition_index)
                         VALUES (?1, NULL, ?2, ?3, ?4, 'directory', 0, ?5)",
                        params![
                            id,
                            data_source_id,
                            format!("__partition_placeholder__/{partition_index}/queued"),
                            partition_name,
                            partition_index as i64,
                        ],
                    )?;
                    id
                }
            };
            Self::promote_partition_placeholder_root(
                main_conn,
                &placeholder_root_id,
                partition_name,
                partition_index,
            )?;

            let inserted =
                match persistence_sqlite::repositories::ntfs_staging_merge::merge_completed_numeric(
                    main_conn,
                    staging_conn,
                    data_source_id,
                    partition_index,
                    &placeholder_root_id,
                )? {
                    Some(rows) => rows,
                    None => main_conn.execute(
                        include_str!("staging_enum_merge.sql"),
                        params![placeholder_root_id, partition_index as i64],
                    )? as u64,
                };
            Ok(inserted)
        })();
        let _ = main_conn.execute_batch("DETACH DATABASE staging");
        result
    }
}
fn staging_db_file_path(conn: &Connection) -> rusqlite::Result<String> {
    conn.query_row(
        "SELECT COALESCE(file, '') FROM pragma_database_list WHERE name = 'main'",
        [],
        |row| row.get(0),
    )
}

// ---------------------------------------------------------------------------
// Analysis merge
// ---------------------------------------------------------------------------

impl StagingRepo {
    pub fn merge_analysis_staging_to_main_in_transaction_with_alias(
        main_conn: &rusqlite::Transaction<'_>,
        staging_conn: &Connection,
        case_id: &str,
        data_source_id: &str,
        _alias: &str,
    ) -> rusqlite::Result<(u64, u64)> {
        crate::staging::analysis::merge(main_conn, staging_conn, case_id, data_source_id)
            .map_err(|error| rusqlite::Error::ToSqlConversionFailure(Box::new(error)))?
            .ok_or_else(|| {
                rusqlite::Error::ToSqlConversionFailure(Box::new(
                    persistence_sqlite::DbError::System("source detail storage is missing".into()),
                ))
            })
    }
}
mod analysis_index;
mod completion;
mod fallback;
mod metadata;
mod schema;
pub use schema::table_has_column;
