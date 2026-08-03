use crate::connection::DbResult;
use rusqlite::{params, Connection};
use std::path::{Path, PathBuf};

/// Directory name for staging databases inside a case directory.
const STAGING_DIR_NAME: &str = "staging";

pub struct StagingRepo;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

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

fn legacy_partition_staging_db_path(
    case_root: &Path,
    data_source_id: &str,
    partition_index: usize,
) -> PathBuf {
    staging_dir(case_root, data_source_id).join(format!("partition_{}.db", partition_index))
}

fn existing_enum_staging_db_path(
    case_root: &Path,
    data_source_id: &str,
    partition_index: usize,
) -> PathBuf {
    let current = enum_staging_db_path(case_root, data_source_id, partition_index);
    if current.exists() {
        return current;
    }
    let legacy = legacy_partition_staging_db_path(case_root, data_source_id, partition_index);
    if legacy.exists() {
        legacy
    } else {
        current
    }
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
        let path = existing_enum_staging_db_path(case_root, data_source_id, partition_index);
        let conn =
            open_staging_with_schema(&path, include_str!("../migrations/scripts/staging_001.sql"))?;
        Self::ensure_enum_staging_columns(&conn)?;
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
         PRAGMA locking_mode=EXCLUSIVE;"
    ))?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Schema upgrade helpers
// ---------------------------------------------------------------------------

impl StagingRepo {
    pub fn ensure_enum_staging_columns(conn: &Connection) -> DbResult<()> {
        for column in ["hidden", "system", "read_only"] {
            if !table_has_column(conn, "file_entries", column)? {
                conn.execute(
                    &format!(
                        "ALTER TABLE file_entries ADD COLUMN {column} INTEGER NOT NULL DEFAULT 0"
                    ),
                    [],
                )?;
            }
        }
        if !table_has_column(conn, "file_entries", "encrypted")? {
            conn.execute_batch(
                "ALTER TABLE file_entries
                 ADD COLUMN encrypted INTEGER
                 CHECK (encrypted IS NULL OR encrypted IN (0, 1));",
            )?;
        }
        if !table_has_column(conn, "file_entries", "partition_index")? {
            conn.execute(
                "ALTER TABLE file_entries ADD COLUMN partition_index INTEGER",
                [],
            )?;
        }
        Ok(())
    }

    pub fn ensure_analysis_staging_columns(conn: &Connection) -> DbResult<()> {
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
}

pub fn table_has_column(conn: &Connection, table: &str, column: &str) -> DbResult<bool> {
    conn.query_row(
        &format!("SELECT COUNT(*) > 0 FROM pragma_table_info('{table}') WHERE name = ?1"),
        [column],
        |row| row.get(0),
    )
    .map_err(Into::into)
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
        conn.execute_batch(
            "DELETE FROM file_entries;
             DELETE FROM staging_meta
              WHERE key IN ('status', 'error', 'merged', 'mft_fallback_warning');",
        )?;
        Ok(())
    }

    pub fn get_staging_meta(conn: &Connection, key: &str) -> DbResult<Option<String>> {
        let mut stmt = conn.prepare("SELECT value FROM staging_meta WHERE key = ?1")?;
        let mut rows = stmt.query_map(params![key], |row| row.get::<_, String>(0))?;
        match rows.next() {
            Some(Ok(v)) => Ok(Some(v)),
            _ => Ok(None),
        }
    }

    pub fn set_staging_meta(conn: &Connection, key: &str, value: &str) -> DbResult<()> {
        conn.execute(
            "INSERT OR REPLACE INTO staging_meta (key, value) VALUES (?1, ?2)",
            params![key, value],
        )?;
        Ok(())
    }

    pub fn get_worker_meta(conn: &Connection, key: &str) -> DbResult<Option<String>> {
        let mut stmt = conn.prepare("SELECT value FROM worker_meta WHERE key = ?1")?;
        let mut rows = stmt.query_map(params![key], |row| row.get::<_, String>(0))?;
        match rows.next() {
            Some(Ok(v)) => Ok(Some(v)),
            _ => Ok(None),
        }
    }

    pub fn set_worker_meta(conn: &Connection, key: &str, value: &str) -> DbResult<()> {
        conn.execute(
            "INSERT OR REPLACE INTO worker_meta (key, value) VALUES (?1, ?2)",
            params![key, value],
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
        super::staging_merge_validation::validate_enum_merge_target(main_conn)?;
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

            let inserted = main_conn.execute(
                "INSERT INTO main.file_entries
                    (id, parent_id, data_source_id, path, name, entry_type,
                      size, ext, deleted, hidden, system, read_only, encrypted, created_at, modified_at, accessed_at, changed_at, hash_sha256,
                      partition_index)
                     SELECT
                        id,
                        CASE
                          WHEN parent_id IS NULL THEN ?1
                          WHEN parent_id = id THEN ?1
                          WHEN parent_id IN (
                            SELECT id FROM staging.file_entries
                            WHERE entry_type = 'directory'
                              AND (
                                parent_id IS NULL
                                OR parent_id = id
                              )
                              AND name IN ('\\', '/', '.')
                          ) THEN ?1
                          ELSE parent_id
                        END,
                        data_source_id, path, name, LOWER(entry_type),
                        size, ext, deleted, hidden, system, read_only, encrypted,
                        created_at, modified_at, accessed_at, changed_at, hash_sha256,
                        ?2
                     FROM staging.file_entries
                     WHERE NOT (
                        entry_type = 'directory'
                        AND (parent_id IS NULL OR parent_id = id)
                        AND name IN ('\\', '/', '.')
                     )",
                params![placeholder_root_id, partition_index as i64],
            )?;
            main_conn.execute_batch("COMMIT")?;
            Ok(inserted as u64)
        })();

        if result.is_err() {
            let _ = main_conn.execute_batch("ROLLBACK");
        }
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
    /// Merge analysis staging rows into main artifacts and timeline_events via ATTACH.
    pub fn merge_analysis_staging_to_main(
        main_conn: &Connection,
        staging_conn: &Connection,
        case_id: &str,
        data_source_id: &str,
    ) -> rusqlite::Result<(u64, u64)> {
        let staging_path = staging_db_file_path(staging_conn)?;
        let escaped = staging_path.replace('\'', "''");
        main_conn.execute_batch(&format!("ATTACH DATABASE '{}' AS analysis_stage", escaped))?;
        main_conn.execute_batch("BEGIN IMMEDIATE")?;

        let result = (|| {
            let artifact_count = main_conn.execute(
                "INSERT INTO main.artifacts
                 (id, case_id, data_source_id, artifact_type, source_object_id, extractor_id, extractor_version, confidence, source_attribution, title, summary, attrs, created_at)
                 SELECT id, ?1, ?2, artifact_type, file_id, extractor_id, extractor_version, confidence, source_attribution, display_name, summary, data_json, created_at
                 FROM analysis_stage.artifact_rows",
                params![case_id, data_source_id],
            )?;
            let timeline_count = main_conn.execute(
                "INSERT INTO main.timeline_events
                 (id, case_id, source_object_id, event_type, ts, title, description, parser_id, parser_version, confidence, source_attribution, attrs)
                 SELECT id, ?1, file_id, event_type, timestamp, title, description, parser_id, parser_version, confidence, source_attribution, data_json
                 FROM analysis_stage.timeline_rows",
                params![case_id],
            )?;
            main_conn.execute_batch("COMMIT")?;
            Ok((artifact_count as u64, timeline_count as u64))
        })();

        if result.is_err() {
            let _ = main_conn.execute_batch("ROLLBACK");
        }
        let _ = main_conn.execute_batch("DETACH DATABASE analysis_stage");
        result
    }

    pub fn read_analysis_index_docs_page(
        conn: &Connection,
        limit: i64,
        offset: i64,
    ) -> rusqlite::Result<Vec<(String, String, String, String)>> {
        let mut stmt = conn.prepare(
            "SELECT file_id, path, text, language FROM index_docs WHERE text <> '' ORDER BY file_id LIMIT ?1 OFFSET ?2",
        )?;
        let rows = stmt.query_map(params![limit, offset], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
            ))
        })?;
        rows.collect()
    }
}
