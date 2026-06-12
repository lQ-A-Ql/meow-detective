use super::ntfs_mft::{enumerate_ntfs_mft_to_staging, visibility_flags_for_name};
use super::progress::ENUM_PROGRESS_INTERVAL;
use super::staging_meta::clear_staging_file_entries;
use crate::staging;
use evidence_core::FileSystemReader;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

/// Info needed to enumerate one partition.
pub struct PartitionWork {
    pub index: usize,
    pub name: String,
    pub fs_kind: String,
    pub fs: Box<dyn FileSystemReader + Send>,
    pub source_path: PathBuf,
    pub source_kind: String,
    pub volume_offset: u64,
}

/// Result from a single partition enumeration.
pub struct PartitionResult {
    pub index: usize,
    pub file_count: u64,
    pub dir_count: u64,
    pub total_size: u64,
    pub warnings: Vec<String>,
    pub error: Option<String>,
}

/// Enumerate a single partition into its staging DB.
pub(super) fn enumerate_single_partition(
    case_root: &Path,
    ds_id: &str,
    partition: PartitionWork,
    cancel_token: &AtomicBool,
    progress_cb: Option<&dyn Fn(u64, u64)>, // (total_entries, total_size)
) -> PartitionResult {
    let idx = partition.index;

    // Open staging DB
    let conn = match staging::open_partition_staging(case_root, ds_id, idx) {
        Ok(conn) => conn,
        Err(e) => {
            return PartitionResult {
                index: idx,
                file_count: 0,
                dir_count: 0,
                total_size: 0,
                warnings: Vec::new(),
                error: Some(format!("Failed to open staging DB: {}", e)),
            };
        }
    };

    // Check if already completed (resume)
    if let Ok(Some(status)) = staging::get_staging_meta(&conn, "status") {
        if status == "done" {
            let file_count = staging::staging_db_row_count(&conn).unwrap_or(0);
            return PartitionResult {
                index: idx,
                file_count,
                dir_count: 0,
                total_size: 0,
                warnings: Vec::new(),
                error: None,
            };
        }
    }

    // Mark as running
    let _ = staging::set_staging_meta(&conn, "status", "running");

    // Enumerate filesystem into staging DB. NTFS gets a best-effort MFT fast
    // path first; any error falls back to recursive reader enumeration.
    let mut warnings = Vec::new();
    let stats = if partition.fs_kind.eq_ignore_ascii_case("ntfs") {
        match enumerate_ntfs_mft_to_staging(&conn, &partition, ds_id, cancel_token, progress_cb) {
            Ok(stats) => Ok(stats),
            Err(error) => {
                tracing::warn!(
                    "MFT fast path failed for partition {}: {}; falling back to recursive enum",
                    idx,
                    error
                );
                let _ = clear_staging_file_entries(&conn);
                let _ = staging::set_staging_meta(&conn, "mft_fallback_warning", &error);
                warnings.push(format!("MFT fast path fallback: {error}"));
                enumerate_fs_to_staging(&conn, &*partition.fs, ds_id, cancel_token, progress_cb)
            }
        }
    } else {
        enumerate_fs_to_staging(&conn, &*partition.fs, ds_id, cancel_token, progress_cb)
    };

    match stats {
        Ok((file_count, dir_count, total_size)) => {
            let _ = staging::set_staging_meta(&conn, "status", "done");
            let _ = staging::set_staging_meta(&conn, "file_count", &file_count.to_string());
            let _ = staging::set_staging_meta(&conn, "dir_count", &dir_count.to_string());
            PartitionResult {
                index: idx,
                file_count,
                dir_count,
                total_size,
                warnings,
                error: None,
            }
        }
        Err(e) => {
            let _ = staging::set_staging_meta(&conn, "status", "failed");
            let _ = staging::set_staging_meta(&conn, "error", &e);
            PartitionResult {
                index: idx,
                file_count: 0,
                dir_count: 0,
                total_size: 0,
                warnings,
                error: Some(e),
            }
        }
    }
}

/// Enumerate a filesystem into a staging DB connection.
///
/// Uses one staging transaction so cancellation/failure can roll back the
/// partition write atomically. Staging DB connections use aggressive temp-DB
/// pragmas, so this avoids extra commit/reprepare churn without changing the
/// conservative main DB merge behavior.
fn enumerate_fs_to_staging(
    conn: &rusqlite::Connection,
    fs: &dyn FileSystemReader,
    ds_id: &str,
    cancel_token: &AtomicBool,
    progress_cb: Option<&dyn Fn(u64, u64)>, // (total_entries, total_size)
) -> Result<(u64, u64, u64), String> {
    if cancel_token.load(Ordering::Relaxed) {
        return Err("Cancelled".to_string());
    }
    let root = fs.root().map_err(|e| e.to_string())?;
    if cancel_token.load(Ordering::Relaxed) {
        return Err("Cancelled".to_string());
    }
    let root_entries = fs.list_children(&root.path).map_err(|e| e.to_string())?;
    if cancel_token.load(Ordering::Relaxed) {
        return Err("Cancelled".to_string());
    }
    let mut file_count = 0u64;
    let mut dir_count = 0u64;
    let mut total_size = 0u64;

    let transaction_result = (|| {
        conn.execute_batch("BEGIN TRANSACTION")
            .map_err(|e| format!("Begin transaction: {}", e))?;

        let mut stmt = conn
            .prepare_cached(
                "INSERT INTO file_entries
                 (id, parent_id, data_source_id, path, name, entry_type,
                  size, ext, deleted, hidden, system, created_at, modified_at, accessed_at, changed_at, hash_sha256)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)",
            )
            .map_err(|e| format!("Prepare error: {}", e))?;

        let mut stack: Vec<(Vec<evidence_core::FsNode>, Option<String>)> = Vec::new();
        stack.push((root_entries, None));

        while let Some((entries, parent_id)) = stack.pop() {
            if cancel_token.load(Ordering::Relaxed) {
                conn.execute_batch("ROLLBACK").ok();
                return Err("Cancelled".to_string());
            }

            for entry in entries {
                if cancel_token.load(Ordering::Relaxed) {
                    conn.execute_batch("ROLLBACK").ok();
                    return Err("Cancelled".to_string());
                }

                let entry_id = uuid::Uuid::new_v4().to_string();
                let is_dir = entry.is_dir;
                let size = entry.size;
                let (hidden, system) =
                    visibility_flags_for_name(&entry.name, entry.hidden, entry.system);

                stmt.execute(rusqlite::params![
                    entry_id,
                    parent_id,
                    ds_id,
                    entry.path,
                    entry.name,
                    if is_dir { "directory" } else { "file" },
                    Some(size),
                    entry.name.rsplit('.').next().map(|e| e.to_lowercase()),
                    0i32,
                    hidden as i32,
                    system as i32,
                    entry.created_at.as_ref().map(|dt| dt.to_rfc3339()),
                    entry.modified_at.as_ref().map(|dt| dt.to_rfc3339()),
                    entry.accessed_at.as_ref().map(|dt| dt.to_rfc3339()),
                    None::<String>,
                    None::<String>,
                ])
                .map_err(|e| format!("Insert error: {}", e))?;

                if is_dir {
                    dir_count += 1;
                } else {
                    file_count += 1;
                    total_size += size;
                }

                // Report progress every 5000 entries
                let total_entries = file_count + dir_count;
                if total_entries.is_multiple_of(ENUM_PROGRESS_INTERVAL) {
                    if let Some(cb) = progress_cb {
                        cb(total_entries, total_size);
                    }
                }

                if is_dir {
                    if cancel_token.load(Ordering::Relaxed) {
                        conn.execute_batch("ROLLBACK").ok();
                        return Err("Cancelled".to_string());
                    }
                    match fs.list_children(&entry.path) {
                        Ok(children) => {
                            if cancel_token.load(Ordering::Relaxed) {
                                conn.execute_batch("ROLLBACK").ok();
                                return Err("Cancelled".to_string());
                            }
                            stack.push((children, Some(entry_id)));
                        }
                        Err(e) => {
                            tracing::warn!("Failed to list {}: {}", entry.path, e);
                        }
                    }
                }
            }
        }

        drop(stmt);
        conn.execute_batch("COMMIT")
            .map_err(|e| format!("Commit error: {}", e))?;
        Ok((file_count, dir_count, total_size))
    })();

    if transaction_result.is_err() {
        conn.execute_batch("ROLLBACK").ok();
    }
    transaction_result
}
