use super::browser_preload::{locator_from_entry, locator_from_row};
use crate::analysis_service::candidates::{normalize_evidence_path, EvidenceCandidate};
use persistence_sqlite::repositories::file_repo::file_encryption_status_from_row;
use rusqlite::{params, Connection};
use uuid::Uuid;

pub(super) fn locate_master_key_files(
    conn: &Connection,
    data_source_id: &str,
) -> Vec<EvidenceCandidate> {
    if persistence_sqlite::source_fjall::has_source_store(conn).unwrap_or(false) {
        let mut candidates = Vec::new();
        if persistence_sqlite::repositories::file_repo::FileRepo::new(conn)
            .visit_all(Some(data_source_id), |entry| {
                if is_master_key_path(&entry.path) && !entry.deleted && !entry.is_directory() {
                    candidates.push(locator_from_entry(data_source_id, &entry, "BrowserPreload"));
                }
                Ok(())
            })
            .is_err()
        {
            return Vec::new();
        }
        return candidates;
    }
    let Ok(mut statement) = conn.prepare(
        "SELECT id, path, COALESCE(size, 0), partition_index, encrypted
         FROM file_entries
         WHERE data_source_id = ?1 AND entry_type = 'file' COLLATE NOCASE
           AND REPLACE(LOWER(path), '\\', '/') LIKE '%/microsoft/protect/%'
         ORDER BY path ASC",
    ) else {
        return Vec::new();
    };
    let Ok(rows) = statement.query_map(params![data_source_id], |row| {
        let id: String = row.get(0)?;
        let path: String = row.get(1)?;
        let size: u64 = row.get(2)?;
        let partition_index = row
            .get::<_, Option<i64>>(3)?
            .and_then(|value| usize::try_from(value).ok());
        let encrypted = file_encryption_status_from_row(row, 4)?.blocks_content();
        Ok((id, path, size, partition_index, encrypted))
    }) else {
        return Vec::new();
    };
    rows.flatten()
        .filter(|(_, path, _, _, _)| is_master_key_path(path))
        .map(|row| locator_from_row(data_source_id, row, "BrowserPreload"))
        .collect()
}

fn is_master_key_path(path: &str) -> bool {
    let normalized = normalize_evidence_path(path);
    let Some(name) = normalized.rsplit('/').next() else {
        return false;
    };
    if Uuid::parse_str(name).is_err() || !normalized.contains("/microsoft/protect/") {
        return false;
    }
    let parent_is_sid = normalized
        .split('/')
        .rev()
        .nth(1)
        .is_some_and(|sid| sid.starts_with("s-1-"));
    let system_protect = normalized.contains("/microsoft/protect/s-1-5-18/user/")
        || normalized.contains("/microsoft/protect/s-1-5-18/machine/");
    parent_is_sid || system_protect
}
