use domain::{DataSourceId, FileEntry};
use persistence_source::fjall::{CatalogRecordStore, FileEntryRecord, ScanDirection};
use persistence_source::repositories::file_repo::MAX_FILE_QUERY_ROWS;
use persistence_sqlite::{DbError, DbResult};
use rusqlite::Connection;

const PAGE_SIZE: usize = 512;
const GROUPS: [&[&str]; 7] = [
    &["etc/hostname"],
    &["os-release"],
    &["etc/hosts"],
    &["vmlinuz-"],
    &["etc/kubernetes/manifests"],
    &["etc/kubernetes/kubelet.conf"],
    &["var/lib/docker/containers", "config.v2.json"],
];

pub(super) fn collect_entries(
    connection: &Connection,
    source: &DataSourceId,
) -> DbResult<Vec<FileEntry>> {
    read_entries(connection, source, MAX_FILE_QUERY_ROWS)
}

fn read_entries(
    connection: &Connection,
    source: &DataSourceId,
    limit: usize,
) -> DbResult<Vec<FileEntry>> {
    if persistence_source::detail::source_id(connection)?.as_deref() != Some(source.0.as_str()) {
        return Err(DbError::System("source catalog owner mismatch".into()));
    }
    let store = persistence_source::detail::require_source_store(connection)?;
    let catalog = CatalogRecordStore::from_source_store((*store).clone());
    let snapshot = catalog.read_snapshot();
    let mut matches: [Option<Vec<FileEntry>>; GROUPS.len()] =
        std::array::from_fn(|_| Some(Vec::new()));
    let mut cursor = None;
    loop {
        persistence_sqlite::read_operation::ensure_active()?;
        let page = catalog
            .list_all(
                &snapshot,
                &source.0,
                cursor.as_deref(),
                ScanDirection::Forward,
                PAGE_SIZE,
            )
            .map_err(|error| DbError::System(error.to_string()))?;
        for record in page.records {
            collect_record(record, &mut matches, limit);
        }
        cursor = page.next_cursor;
        if cursor.is_none() {
            break;
        }
    }
    let mut entries = matches.into_iter().flatten().flatten().collect::<Vec<_>>();
    entries.sort_by(|left, right| {
        left.path
            .cmp(&right.path)
            .then_with(|| left.id.0.cmp(&right.id.0))
    });
    entries.dedup_by(|left, right| left.id == right.id);
    Ok(entries)
}

fn collect_record(
    record: FileEntryRecord,
    matches: &mut [Option<Vec<FileEntry>>; GROUPS.len()],
    limit: usize,
) {
    if record.deleted {
        return;
    }
    let mut entry = None;
    for (fragments, group) in GROUPS.iter().zip(matches) {
        let Some(rows) = group else {
            continue;
        };
        if !fragments
            .iter()
            .all(|fragment| record.path.contains(fragment))
        {
            continue;
        }
        if rows.len() == limit {
            *group = None;
            continue;
        }
        let entry =
            entry.get_or_insert_with(|| persistence_source::detail::file_entry(record.clone()));
        rows.push(entry.clone());
    }
}

#[cfg(test)]
#[path = "../../tests/unit/linux_evidence_facts/catalog.rs"]
mod tests;
