//! Staging management for parallel import.
//!
//! Manages temporary per-partition databases during import:
//! - Manifest tracking (partition state, progress, resume cursor)
//! - Staging DB lifecycle (create, query, merge, cleanup)

use infrastructure::constants::{MANIFEST_FILE_NAME, STAGING_DIR_NAME};
use persistence_sqlite::DbResult;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

const INDEX_DOC_MERGE_PAGE_SIZE: i64 = 50;
const STAGING_CACHE_SIZE_KIB: i64 = 256 * 1024;
const STAGING_MMAP_SIZE_BYTES: i64 = 256 * 1024 * 1024;

// ---------------------------------------------------------------------------
// Manifest types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ImportPhase {
    Enumerating,
    Merging,
    PostProcessing,
    Completed,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum PartitionStatus {
    Pending,
    Running,
    Done,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PartitionEntry {
    pub index: usize,
    pub name: String,
    pub fs_kind: String,
    pub staging_db: String,
    pub status: PartitionStatus,
    pub file_count: u64,
    pub dir_count: u64,
    pub total_size: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StagingManifest {
    pub data_source_id: String,
    pub source_path: String,
    pub source_kind: String,
    pub created_at: String,
    pub phase: ImportPhase,
    pub partitions: Vec<PartitionEntry>,
}

impl StagingManifest {
    /// Create a new manifest for a data source import.
    pub fn create(data_source_id: &str, source_path: &str, source_kind: &str) -> Self {
        Self {
            data_source_id: data_source_id.to_string(),
            source_path: source_path.to_string(),
            source_kind: source_kind.to_string(),
            created_at: chrono::Utc::now().to_rfc3339(),
            phase: ImportPhase::Enumerating,
            partitions: Vec::new(),
        }
    }

    /// Load an existing manifest from disk, if it exists.
    pub fn load(case_root: &Path, data_source_id: &str) -> Option<Self> {
        let path = manifest_path(case_root, data_source_id);
        if !path.exists() {
            return None;
        }
        let data = std::fs::read_to_string(&path).ok()?;
        serde_json::from_str(&data).ok()
    }

    /// Save manifest to disk atomically (write .tmp then rename).
    pub fn save(&self, case_root: &Path) -> Result<(), String> {
        let path = manifest_path(case_root, &self.data_source_id);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let json = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        let tmp_path = path.with_extension("json.tmp");
        std::fs::write(&tmp_path, &json).map_err(|e| e.to_string())?;
        std::fs::rename(&tmp_path, &path).map_err(|e| e.to_string())?;
        Ok(())
    }

    /// Get partitions that need to be (re-)enumerated.
    pub fn pending_partitions(&self) -> Vec<&PartitionEntry> {
        self.partitions
            .iter()
            .filter(|p| p.status == PartitionStatus::Pending || p.status == PartitionStatus::Failed)
            .collect()
    }

    /// Check if all partitions are done.
    pub fn all_partitions_done(&self) -> bool {
        !self.partitions.is_empty()
            && self
                .partitions
                .iter()
                .all(|p| p.status == PartitionStatus::Done)
    }
}

// ---------------------------------------------------------------------------
// Staging DB paths
// ---------------------------------------------------------------------------

/// Get the staging directory for a data source.
pub fn staging_dir(case_root: &Path, data_source_id: &str) -> PathBuf {
    case_root.join(STAGING_DIR_NAME).join(data_source_id)
}

/// Get the manifest file path.
fn manifest_path(case_root: &Path, data_source_id: &str) -> PathBuf {
    staging_dir(case_root, data_source_id).join(MANIFEST_FILE_NAME)
}

/// Get the staging DB path for a partition.
pub fn staging_db_path(case_root: &Path, data_source_id: &str, partition_index: usize) -> PathBuf {
    enum_staging_db_path(case_root, data_source_id, partition_index)
}

/// Get the enumeration staging DB path for a partition.
pub fn enum_staging_db_path(
    case_root: &Path,
    data_source_id: &str,
    partition_index: usize,
) -> PathBuf {
    staging_dir(case_root, data_source_id).join(format!("enum_partition_{}.db", partition_index))
}

fn legacy_partition_staging_db_path(
    case_root: &Path,
    data_source_id: &str,
    partition_index: usize,
) -> PathBuf {
    staging_dir(case_root, data_source_id).join(format!("partition_{}.db", partition_index))
}

/// Resolve existing enum staging DBs created by older builds.
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

/// Get the analysis staging DB path for an analysis worker.
pub fn analysis_staging_db_path(
    case_root: &Path,
    data_source_id: &str,
    worker_id: usize,
) -> PathBuf {
    staging_dir(case_root, data_source_id).join(format!("analysis_worker_{}.db", worker_id))
}

// ---------------------------------------------------------------------------
// Staging DB operations
// ---------------------------------------------------------------------------

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
    open_staging_with_schema(
        &path,
        include_str!("../../persistence-sqlite/src/migrations/scripts/staging_001.sql"),
    )
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

fn table_has_column(conn: &Connection, table: &str, column: &str) -> DbResult<bool> {
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

// ---------------------------------------------------------------------------
// Merge
// ---------------------------------------------------------------------------

/// Merge all staging DBs into the main case.db.
///
/// For each staging DB:
/// 1. ATTACH DATABASE
/// 2. INSERT INTO main.file_entries SELECT * FROM staging.file_entries (in batches)
/// 3. DETACH DATABASE
///
/// Returns total merged file count.
pub fn merge_all_staging_to_main(
    main_conn: &Connection,
    case_root: &Path,
    data_source_id: &str,
    manifest: &StagingManifest,
    progress_cb: Option<&dyn Fn(usize, usize)>, // (completed_partitions, total)
) -> Result<u64, String> {
    let total = manifest.partitions.len();
    let mut merged_total = 0u64;

    for (i, partition) in manifest.partitions.iter().enumerate() {
        if partition.status != PartitionStatus::Done {
            continue;
        }

        let db_path = existing_enum_staging_db_path(case_root, data_source_id, partition.index);
        if !db_path.exists() {
            continue;
        }

        let staging_conn = open_partition_staging(case_root, data_source_id, partition.index)
            .map_err(|e| format!("Open staging DB {}: {}", partition.index, e))?;
        if get_staging_meta(&staging_conn, "merged")
            .map_err(|e| format!("Read staging merge state {}: {}", partition.index, e))?
            .as_deref()
            == Some("true")
        {
            if let Some(cb) = progress_cb {
                cb(i + 1, total);
            }
            continue;
        }
        drop(staging_conn);

        let merge_started = Instant::now();
        let inserted = merge_one_staging_partition(main_conn, &db_path, partition.index)?;
        tracing::info!(
            "Enum staging merge profile: partition={} rows={} elapsedMs={} rowsPerSec={}",
            partition.index,
            inserted,
            merge_started.elapsed().as_millis(),
            rows_per_sec(inserted as u64, merge_started.elapsed())
        );
        let staging_conn = open_partition_staging(case_root, data_source_id, partition.index)
            .map_err(|e| format!("Reopen staging DB {}: {}", partition.index, e))?;
        set_staging_meta(&staging_conn, "merged", "true")
            .map_err(|e| format!("Mark staging DB {} merged: {}", partition.index, e))?;
        merged_total += inserted as u64;

        if let Some(cb) = progress_cb {
            cb(i + 1, total);
        }
    }

    Ok(merged_total)
}

fn merge_one_staging_partition(
    main_conn: &Connection,
    db_path: &Path,
    partition_index: usize,
) -> Result<usize, String> {
    let db_path_str = db_path.to_string_lossy().replace('\'', "''");
    let attach_sql = format!("ATTACH DATABASE '{}' AS staging", db_path_str);
    let result = (|| {
        main_conn
            .execute_batch(&attach_sql)
            .map_err(|e| format!("Attach staging DB {}: {}", partition_index, e))?;
        main_conn
            .execute_batch("BEGIN IMMEDIATE")
            .map_err(|e| format!("Begin merge transaction {}: {}", partition_index, e))?;

        let inserted = main_conn
            .execute(
                "INSERT OR IGNORE INTO main.file_entries
                 (id, parent_id, data_source_id, path, name, entry_type,
                  size, ext, deleted, created_at, modified_at, accessed_at, changed_at, hash_sha256)
                 SELECT id, parent_id, data_source_id, path, name, LOWER(entry_type),
                  size, ext, deleted, created_at, modified_at, accessed_at, changed_at, hash_sha256
                 FROM staging.file_entries",
                [],
            )
            .map_err(|e| format!("Merge partition {}: {}", partition_index, e))?;

        main_conn
            .execute_batch("COMMIT")
            .map_err(|e| format!("Commit merge transaction {}: {}", partition_index, e))?;
        main_conn
            .execute_batch("DETACH DATABASE staging")
            .map_err(|e| format!("Detach staging DB {}: {}", partition_index, e))?;
        Ok(inserted)
    })();

    if result.is_err() {
        let _ = main_conn.execute_batch("ROLLBACK");
        let _ = main_conn.execute_batch("DETACH DATABASE staging");
    }

    result
}

/// Merge analysis worker staging DBs into the main DB and search index.
pub fn merge_analysis_staging_to_main(
    main_conn: &Connection,
    case_root: &Path,
    data_source_id: &str,
    worker_ids: &[usize],
    case_id: &str,
    index_dir: &Path,
    progress_cb: Option<&dyn Fn(usize, usize)>,
) -> Result<AnalysisMergeStats, String> {
    let mut stats = AnalysisMergeStats::default();
    let total = worker_ids.len().max(1);

    for (position, worker_id) in worker_ids.iter().enumerate() {
        let db_path = analysis_staging_db_path(case_root, data_source_id, *worker_id);
        if !db_path.exists() {
            if let Some(cb) = progress_cb {
                cb(position + 1, total);
            }
            continue;
        }

        let worker_conn = open_analysis_staging(case_root, data_source_id, *worker_id)
            .map_err(|e| format!("Open analysis staging DB {}: {}", worker_id, e))?;
        if get_worker_meta(&worker_conn, "merged")
            .map_err(|e| format!("Read analysis merge state {}: {}", worker_id, e))?
            .as_deref()
            == Some("true")
        {
            if let Some(cb) = progress_cb {
                cb(position + 1, total);
            }
            continue;
        }
        drop(worker_conn);

        let worker_merge_started = Instant::now();
        let worker_stats =
            merge_one_analysis_worker(main_conn, &db_path, *worker_id, case_id, data_source_id)?;
        tracing::info!(
            "Analysis DB merge profile: worker={} artifacts={} timeline={} elapsedMs={} rowsPerSec={}",
            worker_id,
            worker_stats.artifact_count,
            worker_stats.timeline_count,
            worker_merge_started.elapsed().as_millis(),
            rows_per_sec(
                worker_stats.artifact_count + worker_stats.timeline_count,
                worker_merge_started.elapsed()
            )
        );
        stats.artifact_count += worker_stats.artifact_count;
        stats.timeline_count += worker_stats.timeline_count;

        let index_merge_started = Instant::now();
        let indexed = merge_one_analysis_index_docs(&db_path, index_dir, *worker_id)?;
        tracing::info!(
            "Analysis index merge profile: worker={} indexed={} elapsedMs={} rowsPerSec={}",
            worker_id,
            indexed,
            index_merge_started.elapsed().as_millis(),
            rows_per_sec(indexed, index_merge_started.elapsed())
        );
        stats.indexed_count += indexed;

        let worker_conn = open_analysis_staging(case_root, data_source_id, *worker_id)
            .map_err(|e| format!("Reopen analysis staging DB {}: {}", worker_id, e))?;
        set_worker_meta(&worker_conn, "merged", "true")
            .map_err(|e| format!("Mark analysis staging DB {} merged: {}", worker_id, e))?;

        if let Some(cb) = progress_cb {
            cb(position + 1, total);
        }
    }

    Ok(stats)
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct AnalysisMergeStats {
    pub artifact_count: u64,
    pub timeline_count: u64,
    pub indexed_count: u64,
}

fn merge_one_analysis_worker(
    main_conn: &Connection,
    db_path: &Path,
    worker_id: usize,
    case_id: &str,
    data_source_id: &str,
) -> Result<AnalysisMergeStats, String> {
    let db_path_str = db_path.to_string_lossy().replace('\'', "''");
    let attach_sql = format!("ATTACH DATABASE '{}' AS analysis_stage", db_path_str);
    let result = (|| {
        main_conn
            .execute_batch(&attach_sql)
            .map_err(|e| format!("Attach analysis DB {}: {}", worker_id, e))?;
        main_conn
            .execute_batch("BEGIN IMMEDIATE")
            .map_err(|e| format!("Begin analysis merge transaction {}: {}", worker_id, e))?;

        let artifact_count = main_conn
            .execute(
                "INSERT OR IGNORE INTO main.artifacts
                 (id, case_id, data_source_id, artifact_type, source_object_id, extractor_id, extractor_version, confidence, source_attribution, title, summary, attrs, created_at)
                  SELECT id, ?1, ?2, artifact_type, file_id, extractor_id, extractor_version, confidence, source_attribution, display_name, summary, data_json, created_at
                  FROM analysis_stage.artifact_rows",
                params![case_id, data_source_id],
            )
            .map_err(|e| format!("Merge analysis artifacts {}: {}", worker_id, e))?;

        let timeline_count = main_conn
            .execute(
                "INSERT OR IGNORE INTO main.timeline_events
                 (id, case_id, source_object_id, event_type, ts, title, description, parser_id, parser_version, confidence, source_attribution, attrs)
                  SELECT id, ?1, file_id, event_type, timestamp, title, description, parser_id, parser_version, confidence, source_attribution, data_json
                  FROM analysis_stage.timeline_rows",
                params![case_id],
            )
            .map_err(|e| format!("Merge analysis timeline {}: {}", worker_id, e))?;

        main_conn
            .execute_batch("COMMIT")
            .map_err(|e| format!("Commit analysis merge transaction {}: {}", worker_id, e))?;
        main_conn
            .execute_batch("DETACH DATABASE analysis_stage")
            .map_err(|e| format!("Detach analysis DB {}: {}", worker_id, e))?;

        Ok(AnalysisMergeStats {
            artifact_count: artifact_count as u64,
            timeline_count: timeline_count as u64,
            indexed_count: 0,
        })
    })();

    if result.is_err() {
        let _ = main_conn.execute_batch("ROLLBACK");
        let _ = main_conn.execute_batch("DETACH DATABASE analysis_stage");
    }

    result
}

fn merge_one_analysis_index_docs(
    db_path: &Path,
    index_dir: &Path,
    worker_id: usize,
) -> Result<u64, String> {
    let conn = Connection::open(db_path)
        .map_err(|e| format!("Open analysis index docs {}: {}", worker_id, e))?;
    let index = match search::SearchIndex::open(index_dir) {
        Ok(index) => index,
        Err(_) => search::SearchIndex::create(index_dir).map_err(|e| e.to_string())?,
    };
    let mut indexed_total = 0u64;
    let mut offset = 0i64;
    loop {
        let mut stmt = conn
            .prepare(
                "SELECT file_id, path, text, language
                 FROM index_docs
                 WHERE text <> ''
                 ORDER BY file_id
                 LIMIT ?1 OFFSET ?2",
            )
            .map_err(|e| format!("Prepare index docs {}: {}", worker_id, e))?;
        let rows = stmt
            .query_map(params![INDEX_DOC_MERGE_PAGE_SIZE, offset], |row| {
                let file_id: String = row.get(0)?;
                let path: String = row.get(1)?;
                let text: String = row.get(2)?;
                let language: String = row.get(3)?;
                Ok((file_id, path, text, language))
            })
            .map_err(|e| format!("Read index docs {}: {}", worker_id, e))?;

        let mut texts = Vec::new();
        let mut paths = Vec::new();
        for row in rows {
            let (file_id, path, text, language) =
                row.map_err(|e| format!("Map index docs {}: {}", worker_id, e))?;
            texts.push(search::ExtractedText {
                file_id: file_id.clone(),
                content: text,
                encoding: language,
                extractable: true,
                byte_count: 0,
            });
            paths.push((file_id, path));
        }
        if texts.is_empty() {
            break;
        }

        indexed_total += index
            .index_documents(&texts, &paths)
            .map_err(|e| e.to_string())?;
        if texts.len() < INDEX_DOC_MERGE_PAGE_SIZE as usize {
            break;
        }
        offset += INDEX_DOC_MERGE_PAGE_SIZE;
    }
    Ok(indexed_total)
}

fn rows_per_sec(rows: u64, duration: Duration) -> u64 {
    let secs = duration.as_secs_f64();
    if secs <= 0.0 {
        rows
    } else {
        (rows as f64 / secs).round() as u64
    }
}

/// Clean up staging directory for a data source.
pub fn cleanup_staging(case_root: &Path, data_source_id: &str) {
    let dir = staging_dir(case_root, data_source_id);
    if dir.exists() {
        checkpoint_staging_wal_files(&dir);
        if let Err(err) = std::fs::remove_dir_all(&dir) {
            tracing::warn!(
                "Failed to remove staging directory {}: {}",
                dir.display(),
                err
            );
        }
    }
}

fn checkpoint_staging_wal_files(dir: &Path) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("db") {
            continue;
        }

        match Connection::open(&path) {
            Ok(conn) => {
                if let Err(err) = conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);") {
                    tracing::debug!(
                        "Failed to checkpoint staging WAL {}: {}",
                        path.display(),
                        err
                    );
                }
            }
            Err(err) => {
                tracing::debug!("Failed to open staging DB {}: {}", path.display(), err);
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifest_create_and_serialize() {
        let m = StagingManifest::create("ds-1", "/evidence/disk.E01", "E01");
        assert_eq!(m.phase, ImportPhase::Enumerating);
        assert!(m.partitions.is_empty());

        let json = serde_json::to_string(&m).unwrap();
        assert!(json.contains("ds-1"));
        let deserialized: StagingManifest = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.data_source_id, "ds-1");
    }

    #[test]
    fn manifest_pending_partitions() {
        let mut m = StagingManifest::create("ds-1", "/evidence/disk.E01", "E01");
        m.partitions.push(PartitionEntry {
            index: 0,
            name: "P0".to_string(),
            fs_kind: "Ntfs".to_string(),
            staging_db: "partition_0.db".to_string(),
            status: PartitionStatus::Done,
            file_count: 100,
            dir_count: 10,
            total_size: 5000,
            last_path: None,
            completed_at: None,
            error: None,
        });
        m.partitions.push(PartitionEntry {
            index: 1,
            name: "P1".to_string(),
            fs_kind: "Ntfs".to_string(),
            staging_db: "partition_1.db".to_string(),
            status: PartitionStatus::Pending,
            file_count: 0,
            dir_count: 0,
            total_size: 0,
            last_path: None,
            completed_at: None,
            error: None,
        });

        assert_eq!(m.pending_partitions().len(), 1);
        assert!(!m.all_partitions_done());
    }

    #[test]
    fn manifest_save_and_load_roundtrip() {
        let tmp = tempfile::TempDir::new().unwrap();
        let mut m = StagingManifest::create("ds-test", "/evidence/test.E01", "E01");
        m.partitions.push(PartitionEntry {
            index: 0,
            name: "P0".to_string(),
            fs_kind: "Ntfs".to_string(),
            staging_db: "partition_0.db".to_string(),
            status: PartitionStatus::Done,
            file_count: 42,
            dir_count: 5,
            total_size: 12345,
            last_path: None,
            completed_at: Some("2026-01-01T00:00:00Z".to_string()),
            error: None,
        });
        m.save(tmp.path()).unwrap();

        let loaded = StagingManifest::load(tmp.path(), "ds-test").unwrap();
        assert_eq!(loaded.partitions.len(), 1);
        assert_eq!(loaded.partitions[0].file_count, 42);
    }

    #[test]
    fn staging_db_create_and_insert() {
        let tmp = tempfile::TempDir::new().unwrap();
        let conn = open_partition_staging(tmp.path(), "ds-1", 0).unwrap();

        conn.execute(
            "INSERT INTO file_entries (id, data_source_id, path, name, entry_type)
             VALUES ('f1', 'ds-1', '/test/file.txt', 'file.txt', 'File')",
            [],
        )
        .unwrap();

        let count = staging_db_row_count(&conn).unwrap();
        assert_eq!(count, 1);

        set_staging_meta(&conn, "status", "done").unwrap();
        let status = get_staging_meta(&conn, "status").unwrap();
        assert_eq!(status.as_deref(), Some("done"));
    }

    #[test]
    fn enum_staging_bulk_schema_has_no_secondary_indexes_during_insert() {
        let tmp = tempfile::TempDir::new().unwrap();
        let conn = open_enum_staging(tmp.path(), "ds-idx", 0).unwrap();

        let indexes: Vec<String> = {
            let mut stmt = conn
                .prepare("SELECT name FROM sqlite_master WHERE type='index' ORDER BY name")
                .unwrap();
            stmt.query_map([], |row| row.get::<_, String>(0))
                .unwrap()
                .collect::<Result<Vec<_>, _>>()
                .unwrap()
        };

        assert!(!indexes.iter().any(|idx| idx == "idx_staging_parent"));
        assert!(!indexes.iter().any(|idx| idx == "idx_staging_path"));
        assert!(!indexes.iter().any(|idx| idx == "idx_staging_data_source"));
    }

    #[test]
    fn enum_and_analysis_staging_use_aggressive_temp_pragmas() {
        let tmp = tempfile::TempDir::new().unwrap();
        let enum_conn = open_enum_staging(tmp.path(), "ds-pragmas", 0).unwrap();
        let analysis_conn = open_analysis_staging(tmp.path(), "ds-pragmas", 0).unwrap();

        for conn in [&enum_conn, &analysis_conn] {
            let journal_mode: String = conn
                .query_row("PRAGMA journal_mode", [], |row| row.get(0))
                .unwrap();
            let synchronous: i64 = conn
                .query_row("PRAGMA synchronous", [], |row| row.get(0))
                .unwrap();
            let temp_store: i64 = conn
                .query_row("PRAGMA temp_store", [], |row| row.get(0))
                .unwrap();
            let foreign_keys: i64 = conn
                .query_row("PRAGMA foreign_keys", [], |row| row.get(0))
                .unwrap();
            let cache_size: i64 = conn
                .query_row("PRAGMA cache_size", [], |row| row.get(0))
                .unwrap();

            assert_eq!(journal_mode, "wal");
            assert_eq!(synchronous, 0);
            assert_eq!(temp_store, 2);
            assert_eq!(foreign_keys, 1);
            assert_eq!(cache_size, -STAGING_CACHE_SIZE_KIB);
        }
    }

    #[test]
    fn merge_staging_to_main() {
        let tmp = tempfile::TempDir::new().unwrap();
        let main_conn = persistence_sqlite::connection::open_in_memory().unwrap();
        main_conn
            .execute_batch(
                "CREATE TABLE IF NOT EXISTS file_entries (
                    id TEXT PRIMARY KEY NOT NULL,
                    parent_id TEXT,
                    data_source_id TEXT NOT NULL,
                    path TEXT NOT NULL,
                    name TEXT NOT NULL,
                    entry_type TEXT NOT NULL,
                    size INTEGER,
                    ext TEXT,
                    deleted INTEGER NOT NULL DEFAULT 0,
                    created_at TEXT,
                    modified_at TEXT,
                    accessed_at TEXT,
                    changed_at TEXT,
                    hash_sha256 TEXT
                )",
            )
            .unwrap();

        // Create staging DB with some entries
        let ds_id = "ds-merge-test";
        let staging_conn = open_partition_staging(tmp.path(), ds_id, 0).unwrap();
        for i in 0..5 {
            staging_conn
                .execute(
                    "INSERT INTO file_entries (id, data_source_id, path, name, entry_type)
                     VALUES (?1, ?2, ?3, ?4, 'File')",
                    params![
                        format!("f{}", i),
                        ds_id,
                        format!("/test/file{}.txt", i),
                        format!("file{}.txt", i),
                    ],
                )
                .unwrap();
        }

        let mut manifest = StagingManifest::create(ds_id, "/test.E01", "E01");
        manifest.partitions.push(PartitionEntry {
            index: 0,
            name: "P0".to_string(),
            fs_kind: "Ntfs".to_string(),
            staging_db: "partition_0.db".to_string(),
            status: PartitionStatus::Done,
            file_count: 5,
            dir_count: 0,
            total_size: 0,
            last_path: None,
            completed_at: None,
            error: None,
        });

        let merged =
            merge_all_staging_to_main(&main_conn, tmp.path(), ds_id, &manifest, None).unwrap();
        assert_eq!(merged, 5);

        let count: i64 = main_conn
            .query_row("SELECT COUNT(*) FROM file_entries", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 5);

        let mixed_case_types: i64 = main_conn
            .query_row(
                "SELECT COUNT(*) FROM file_entries WHERE entry_type NOT IN ('file', 'directory')",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(mixed_case_types, 0);
    }

    fn create_main_file_entries_table(conn: &Connection) {
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS file_entries (
                id TEXT PRIMARY KEY NOT NULL,
                parent_id TEXT,
                data_source_id TEXT NOT NULL,
                path TEXT NOT NULL,
                name TEXT NOT NULL,
                entry_type TEXT NOT NULL,
                size INTEGER,
                ext TEXT,
                deleted INTEGER NOT NULL DEFAULT 0,
                created_at TEXT,
                modified_at TEXT,
                accessed_at TEXT,
                changed_at TEXT,
                hash_sha256 TEXT
            )",
        )
        .unwrap();
    }

    fn make_done_partition(index: usize, file_count: u64) -> PartitionEntry {
        PartitionEntry {
            index,
            name: format!("P{}", index),
            fs_kind: "Ntfs".to_string(),
            staging_db: format!("partition_{}.db", index),
            status: PartitionStatus::Done,
            file_count,
            dir_count: 0,
            total_size: 0,
            last_path: None,
            completed_at: None,
            error: None,
        }
    }

    fn attached_db_names(conn: &Connection) -> Vec<String> {
        let mut stmt = conn.prepare("PRAGMA database_list").unwrap();
        stmt.query_map([], |row| row.get::<_, String>(1))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap()
    }

    #[test]
    fn merge_all_staging_two_partitions() {
        let tmp = tempfile::TempDir::new().unwrap();
        let main_conn = persistence_sqlite::connection::open_in_memory().unwrap();
        create_main_file_entries_table(&main_conn);

        let ds_id = "ds-two-part";

        // Partition 0: 3 files
        let s0 = open_partition_staging(tmp.path(), ds_id, 0).unwrap();
        for i in 0..3 {
            s0.execute(
                "INSERT INTO file_entries (id, data_source_id, path, name, entry_type)
                 VALUES (?1, ?2, ?3, ?4, 'File')",
                params![
                    format!("p0f{}", i),
                    ds_id,
                    format!("/p0/file{}.txt", i),
                    format!("file{}.txt", i),
                ],
            )
            .unwrap();
        }

        // Partition 1: 2 files
        let s1 = open_partition_staging(tmp.path(), ds_id, 1).unwrap();
        for i in 0..2 {
            s1.execute(
                "INSERT INTO file_entries (id, data_source_id, path, name, entry_type)
                 VALUES (?1, ?2, ?3, ?4, 'File')",
                params![
                    format!("p1f{}", i),
                    ds_id,
                    format!("/p1/file{}.txt", i),
                    format!("file{}.txt", i),
                ],
            )
            .unwrap();
        }

        let mut manifest = StagingManifest::create(ds_id, "/test.E01", "E01");
        for idx in 0..2 {
            manifest.partitions.push(PartitionEntry {
                index: idx,
                name: format!("P{}", idx),
                fs_kind: "Ntfs".to_string(),
                staging_db: format!("partition_{}.db", idx),
                status: PartitionStatus::Done,
                file_count: if idx == 0 { 3 } else { 2 },
                dir_count: 0,
                total_size: 0,
                last_path: None,
                completed_at: None,
                error: None,
            });
        }

        let merged =
            merge_all_staging_to_main(&main_conn, tmp.path(), ds_id, &manifest, None).unwrap();
        assert_eq!(merged, 5);

        let count: i64 = main_conn
            .query_row("SELECT COUNT(*) FROM file_entries", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 5);
    }

    #[test]
    fn merge_marks_staging_merged_and_repeat_is_idempotent() {
        let tmp = tempfile::TempDir::new().unwrap();
        let main_conn = persistence_sqlite::connection::open_in_memory().unwrap();
        create_main_file_entries_table(&main_conn);

        let ds_id = "ds-idempotent";
        let staging_conn = open_partition_staging(tmp.path(), ds_id, 0).unwrap();
        staging_conn
            .execute(
                "INSERT INTO file_entries (id, data_source_id, path, name, entry_type)
                 VALUES ('f1', ?1, '/f1', 'f1', 'File')",
                params![ds_id],
            )
            .unwrap();
        drop(staging_conn);

        let mut manifest = StagingManifest::create(ds_id, "/test.E01", "E01");
        manifest.partitions.push(make_done_partition(0, 1));

        let first =
            merge_all_staging_to_main(&main_conn, tmp.path(), ds_id, &manifest, None).unwrap();
        let second =
            merge_all_staging_to_main(&main_conn, tmp.path(), ds_id, &manifest, None).unwrap();

        assert_eq!(first, 1);
        assert_eq!(second, 0);
        let count: i64 = main_conn
            .query_row("SELECT COUNT(*) FROM file_entries", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 1);

        let staging_conn = open_partition_staging(tmp.path(), ds_id, 0).unwrap();
        assert_eq!(
            get_staging_meta(&staging_conn, "merged")
                .unwrap()
                .as_deref(),
            Some("true")
        );
    }

    #[test]
    fn merge_skips_partition_already_marked_merged() {
        let tmp = tempfile::TempDir::new().unwrap();
        let main_conn = persistence_sqlite::connection::open_in_memory().unwrap();
        create_main_file_entries_table(&main_conn);

        let ds_id = "ds-skip-merged";
        let staging_conn = open_partition_staging(tmp.path(), ds_id, 0).unwrap();
        staging_conn
            .execute(
                "INSERT INTO file_entries (id, data_source_id, path, name, entry_type)
                 VALUES ('f1', ?1, '/f1', 'f1', 'File')",
                params![ds_id],
            )
            .unwrap();
        set_staging_meta(&staging_conn, "merged", "true").unwrap();
        drop(staging_conn);

        let mut manifest = StagingManifest::create(ds_id, "/test.E01", "E01");
        manifest.partitions.push(make_done_partition(0, 1));

        let merged =
            merge_all_staging_to_main(&main_conn, tmp.path(), ds_id, &manifest, None).unwrap();

        assert_eq!(merged, 0);
        let count: i64 = main_conn
            .query_row("SELECT COUNT(*) FROM file_entries", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn merge_failure_rolls_back_and_detaches_staging() {
        let tmp = tempfile::TempDir::new().unwrap();
        let main_conn = persistence_sqlite::connection::open_in_memory().unwrap();
        main_conn
            .execute_batch(
                "CREATE TABLE file_entries (
                    id TEXT PRIMARY KEY NOT NULL,
                    data_source_id TEXT NOT NULL,
                    path TEXT NOT NULL,
                    name TEXT NOT NULL,
                    entry_type TEXT NOT NULL
                );
                INSERT INTO file_entries (id, data_source_id, path, name, entry_type)
                VALUES ('existing', 'ds', '/existing', 'existing', 'File');",
            )
            .unwrap();

        let ds_id = "ds-fail";
        let staging_conn = open_partition_staging(tmp.path(), ds_id, 0).unwrap();
        staging_conn
            .execute(
                "INSERT INTO file_entries (id, data_source_id, path, name, entry_type)
                 VALUES ('f1', ?1, '/f1', 'f1', 'File')",
                params![ds_id],
            )
            .unwrap();
        drop(staging_conn);

        let mut manifest = StagingManifest::create(ds_id, "/test.E01", "E01");
        manifest.partitions.push(make_done_partition(0, 1));

        assert!(merge_all_staging_to_main(&main_conn, tmp.path(), ds_id, &manifest, None).is_err());
        let count: i64 = main_conn
            .query_row("SELECT COUNT(*) FROM file_entries", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 1);
        assert!(!attached_db_names(&main_conn)
            .iter()
            .any(|name| name == "staging"));
    }

    #[test]
    fn merge_all_staging_empty_db() {
        let tmp = tempfile::TempDir::new().unwrap();
        let main_conn = persistence_sqlite::connection::open_in_memory().unwrap();
        create_main_file_entries_table(&main_conn);

        let ds_id = "ds-empty";
        // Create staging DB but insert nothing
        let _s0 = open_partition_staging(tmp.path(), ds_id, 0).unwrap();

        let mut manifest = StagingManifest::create(ds_id, "/test.E01", "E01");
        manifest.partitions.push(PartitionEntry {
            index: 0,
            name: "P0".to_string(),
            fs_kind: "Ntfs".to_string(),
            staging_db: "partition_0.db".to_string(),
            status: PartitionStatus::Done,
            file_count: 0,
            dir_count: 0,
            total_size: 0,
            last_path: None,
            completed_at: None,
            error: None,
        });

        let merged =
            merge_all_staging_to_main(&main_conn, tmp.path(), ds_id, &manifest, None).unwrap();
        assert_eq!(merged, 0);
    }

    #[test]
    fn merge_all_staging_progress_callback_invoked() {
        let tmp = tempfile::TempDir::new().unwrap();
        let main_conn = persistence_sqlite::connection::open_in_memory().unwrap();
        create_main_file_entries_table(&main_conn);

        let ds_id = "ds-cb";
        let s0 = open_partition_staging(tmp.path(), ds_id, 0).unwrap();
        s0.execute(
            "INSERT INTO file_entries (id, data_source_id, path, name, entry_type)
             VALUES ('f1', 'ds-cb', '/f1', 'f1', 'File')",
            [],
        )
        .unwrap();

        let mut manifest = StagingManifest::create(ds_id, "/test.E01", "E01");
        manifest.partitions.push(PartitionEntry {
            index: 0,
            name: "P0".to_string(),
            fs_kind: "Ntfs".to_string(),
            staging_db: "partition_0.db".to_string(),
            status: PartitionStatus::Done,
            file_count: 1,
            dir_count: 0,
            total_size: 0,
            last_path: None,
            completed_at: None,
            error: None,
        });

        let cb_invoked = std::sync::atomic::AtomicBool::new(false);
        let cb = |completed: usize, total: usize| {
            assert_eq!(completed, 1);
            assert_eq!(total, 1);
            cb_invoked.store(true, std::sync::atomic::Ordering::Relaxed);
        };

        merge_all_staging_to_main(&main_conn, tmp.path(), ds_id, &manifest, Some(&cb)).unwrap();
        assert!(cb_invoked.load(std::sync::atomic::Ordering::Relaxed));
    }

    fn create_main_analysis_tables(conn: &Connection) {
        conn.execute_batch(
            "CREATE TABLE artifacts (
                id TEXT PRIMARY KEY NOT NULL,
                case_id TEXT NOT NULL DEFAULT '',
                data_source_id TEXT NOT NULL DEFAULT '',
                artifact_type TEXT NOT NULL,
                source_object_id TEXT,
                extractor_id TEXT,
                extractor_version TEXT,
                confidence REAL,
                source_attribution TEXT,
                title TEXT NOT NULL,
                summary TEXT NOT NULL DEFAULT '',
                attrs TEXT NOT NULL DEFAULT '{}',
                created_at TEXT NOT NULL DEFAULT (datetime('now'))
            );
            CREATE TABLE timeline_events (
                id TEXT PRIMARY KEY NOT NULL,
                case_id TEXT NOT NULL DEFAULT '',
                source_object_id TEXT NOT NULL,
                event_type TEXT NOT NULL,
                ts TEXT NOT NULL,
                title TEXT NOT NULL,
                description TEXT NOT NULL DEFAULT '',
                parser_id TEXT,
                parser_version TEXT,
                confidence REAL,
                source_attribution TEXT,
                attrs TEXT NOT NULL DEFAULT '{}'
            );",
        )
        .unwrap();
    }

    #[test]
    fn analysis_staging_open_creates_expected_tables() {
        let tmp = tempfile::TempDir::new().unwrap();
        let conn = open_analysis_staging(tmp.path(), "ds-analysis", 0).unwrap();
        for table in [
            "artifact_rows",
            "timeline_rows",
            "index_docs",
            "worker_meta",
        ] {
            let name: String = conn
                .query_row(
                    "SELECT name FROM sqlite_master WHERE type='table' AND name=?1",
                    params![table],
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(name, table);
        }
    }

    #[test]
    fn analysis_staging_open_upgrades_legacy_provenance_columns() {
        let tmp = tempfile::TempDir::new().unwrap();
        let db_path = analysis_staging_db_path(tmp.path(), "ds-analysis", 0);
        std::fs::create_dir_all(db_path.parent().unwrap()).unwrap();
        let legacy = Connection::open(&db_path).unwrap();
        legacy
            .execute_batch(
                "CREATE TABLE artifact_rows (
                    id TEXT PRIMARY KEY NOT NULL,
                    file_id TEXT,
                    artifact_type TEXT NOT NULL,
                    display_name TEXT NOT NULL,
                    summary TEXT NOT NULL DEFAULT '',
                    data_json TEXT NOT NULL DEFAULT '{}',
                    source_path TEXT NOT NULL DEFAULT '',
                    created_at TEXT NOT NULL
                );
                CREATE TABLE timeline_rows (
                    id TEXT PRIMARY KEY NOT NULL,
                    file_id TEXT NOT NULL,
                    timestamp TEXT NOT NULL,
                    event_type TEXT NOT NULL,
                    title TEXT NOT NULL,
                    description TEXT NOT NULL DEFAULT '',
                    data_json TEXT NOT NULL DEFAULT '{}'
                );
                CREATE TABLE index_docs (
                    file_id TEXT PRIMARY KEY NOT NULL,
                    path TEXT NOT NULL,
                    text TEXT NOT NULL,
                    language TEXT NOT NULL DEFAULT 'unknown',
                    truncated INTEGER NOT NULL DEFAULT 0
                );
                CREATE TABLE worker_meta (
                    key TEXT PRIMARY KEY NOT NULL,
                    value TEXT NOT NULL
                );",
            )
            .unwrap();
        drop(legacy);

        let conn = open_analysis_staging(tmp.path(), "ds-analysis", 0).unwrap();

        for (table, column) in [
            ("artifact_rows", "extractor_id"),
            ("artifact_rows", "extractor_version"),
            ("artifact_rows", "confidence"),
            ("artifact_rows", "source_attribution"),
            ("timeline_rows", "parser_id"),
            ("timeline_rows", "parser_version"),
            ("timeline_rows", "confidence"),
            ("timeline_rows", "source_attribution"),
        ] {
            assert!(table_has_column(&conn, table, column).unwrap());
        }
    }

    #[test]
    fn analysis_merge_skips_already_merged_worker_db() {
        let tmp = tempfile::TempDir::new().unwrap();
        let main_conn = persistence_sqlite::connection::open_in_memory().unwrap();
        create_main_analysis_tables(&main_conn);
        let worker = open_analysis_staging(tmp.path(), "ds-analysis", 0).unwrap();
        worker
            .execute(
                "INSERT INTO artifact_rows
                 (id, file_id, artifact_type, display_name, summary, data_json, source_path, created_at)
                 VALUES ('a1', 'f1', 'Prefetch', 'Artifact', '', '{}', '', '2026-01-01T00:00:00Z')",
                [],
            )
            .unwrap();
        set_worker_meta(&worker, "merged", "true").unwrap();

        let stats = merge_analysis_staging_to_main(
            &main_conn,
            tmp.path(),
            "ds-analysis",
            &[0],
            "case-1",
            &tmp.path().join("index"),
            None,
        )
        .unwrap();

        assert_eq!(stats.artifact_count, 0);
        let count: i64 = main_conn
            .query_row("SELECT COUNT(*) FROM artifacts", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn index_merge_uses_pages_not_full_vec() {
        let tmp = tempfile::TempDir::new().unwrap();
        let worker = open_analysis_staging(tmp.path(), "ds-analysis", 0).unwrap();
        for i in 0..(INDEX_DOC_MERGE_PAGE_SIZE + 5) {
            worker
                .execute(
                    "INSERT INTO index_docs (file_id, path, text, language, truncated)
                     VALUES (?1, ?2, ?3, 'utf-8', 0)",
                    params![
                        format!("f-{i:03}"),
                        format!("file-{i:03}.txt"),
                        format!("marker page-test-{i:03}")
                    ],
                )
                .unwrap();
        }
        drop(worker);

        let indexed = merge_one_analysis_index_docs(
            &analysis_staging_db_path(tmp.path(), "ds-analysis", 0),
            &tmp.path().join("idx"),
            0,
        )
        .unwrap();

        assert_eq!(indexed, (INDEX_DOC_MERGE_PAGE_SIZE + 5) as u64);
    }

    #[test]
    fn analysis_merge_rolls_back_and_detaches_on_failure() {
        let tmp = tempfile::TempDir::new().unwrap();
        let main_conn = persistence_sqlite::connection::open_in_memory().unwrap();
        main_conn
            .execute_batch(
                "CREATE TABLE artifacts (
                    id TEXT PRIMARY KEY NOT NULL,
                    case_id TEXT NOT NULL DEFAULT '',
                    data_source_id TEXT NOT NULL DEFAULT '',
                    artifact_type TEXT NOT NULL,
                    source_object_id TEXT,
                    extractor_id TEXT,
                    extractor_version TEXT,
                    confidence REAL,
                    source_attribution TEXT,
                    title TEXT NOT NULL,
                    summary TEXT NOT NULL DEFAULT '',
                    attrs TEXT NOT NULL DEFAULT '{}',
                    created_at TEXT NOT NULL DEFAULT (datetime('now'))
                );
                INSERT INTO artifacts (id, case_id, data_source_id, artifact_type, title)
                VALUES ('existing', 'case-1', 'ds-analysis', 'x', 'existing');",
            )
            .unwrap();
        let worker = open_analysis_staging(tmp.path(), "ds-analysis", 0).unwrap();
        worker
            .execute(
                "INSERT INTO artifact_rows
                 (id, file_id, artifact_type, display_name, summary, data_json, source_path, created_at)
                 VALUES ('a1', 'f1', 'Prefetch', 'Artifact', '', '{}', '', '2026-01-01T00:00:00Z')",
                [],
            )
            .unwrap();

        let result = merge_analysis_staging_to_main(
            &main_conn,
            tmp.path(),
            "ds-analysis",
            &[0],
            "case-1",
            &tmp.path().join("index"),
            None,
        );
        assert!(result.is_err());
        let count: i64 = main_conn
            .query_row("SELECT COUNT(*) FROM artifacts", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 1);
        assert!(!attached_db_names(&main_conn)
            .iter()
            .any(|name| name == "analysis_stage"));
    }

    #[test]
    fn cleanup_staging_removes_directory() {
        let tmp = tempfile::TempDir::new().unwrap();
        let ds_id = "ds-cleanup";

        // Create staging dir + a DB, then drop connection before cleanup
        {
            let _conn = open_partition_staging(tmp.path(), ds_id, 0).unwrap();
        }
        let dir = staging_dir(tmp.path(), ds_id);
        assert!(dir.exists());

        cleanup_staging(tmp.path(), ds_id);
        assert!(!dir.exists());
    }

    #[test]
    fn staging_db_row_count_empty_returns_zero() {
        let tmp = tempfile::TempDir::new().unwrap();
        let conn = open_partition_staging(tmp.path(), "ds-1", 0).unwrap();
        let count = staging_db_row_count(&conn).unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn set_and_get_staging_meta_roundtrip() {
        let tmp = tempfile::TempDir::new().unwrap();
        let conn = open_partition_staging(tmp.path(), "ds-1", 0).unwrap();

        set_staging_meta(&conn, "status", "done").unwrap();
        set_staging_meta(&conn, "file_count", "42").unwrap();

        assert_eq!(
            get_staging_meta(&conn, "status").unwrap().as_deref(),
            Some("done")
        );
        assert_eq!(
            get_staging_meta(&conn, "file_count").unwrap().as_deref(),
            Some("42")
        );
        assert_eq!(get_staging_meta(&conn, "nonexistent").unwrap(), None);
    }

    #[test]
    fn manifest_all_partitions_done_true_when_all_done() {
        let mut m = StagingManifest::create("ds-1", "/test.E01", "E01");
        m.partitions.push(PartitionEntry {
            index: 0,
            name: "P0".to_string(),
            fs_kind: "Ntfs".to_string(),
            staging_db: "partition_0.db".to_string(),
            status: PartitionStatus::Done,
            file_count: 10,
            dir_count: 1,
            total_size: 500,
            last_path: None,
            completed_at: None,
            error: None,
        });
        m.partitions.push(PartitionEntry {
            index: 1,
            name: "P1".to_string(),
            fs_kind: "Fat32".to_string(),
            staging_db: "partition_1.db".to_string(),
            status: PartitionStatus::Done,
            file_count: 5,
            dir_count: 1,
            total_size: 200,
            last_path: None,
            completed_at: None,
            error: None,
        });
        assert!(m.all_partitions_done());
    }

    #[test]
    fn manifest_all_partitions_done_false_when_one_pending() {
        let mut m = StagingManifest::create("ds-1", "/test.E01", "E01");
        m.partitions.push(PartitionEntry {
            index: 0,
            name: "P0".to_string(),
            fs_kind: "Ntfs".to_string(),
            staging_db: "partition_0.db".to_string(),
            status: PartitionStatus::Done,
            file_count: 10,
            dir_count: 1,
            total_size: 500,
            last_path: None,
            completed_at: None,
            error: None,
        });
        m.partitions.push(PartitionEntry {
            index: 1,
            name: "P1".to_string(),
            fs_kind: "Fat32".to_string(),
            staging_db: "partition_1.db".to_string(),
            status: PartitionStatus::Pending,
            file_count: 0,
            dir_count: 0,
            total_size: 0,
            last_path: None,
            completed_at: None,
            error: None,
        });
        assert!(!m.all_partitions_done());
    }

    #[test]
    fn manifest_all_partitions_done_false_when_empty() {
        let m = StagingManifest::create("ds-1", "/test.E01", "E01");
        assert!(!m.all_partitions_done());
    }
}
