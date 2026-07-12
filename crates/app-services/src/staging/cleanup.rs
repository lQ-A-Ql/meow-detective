use super::schema::staging_dir;
use rusqlite::Connection;
use std::path::Path;

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
