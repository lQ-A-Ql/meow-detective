use std::collections::{HashMap, HashSet};

use domain::{DataSourceId, EntryType, FileEntry, FileEntryId};
use fs_ntfs::mft_scanner::MftRecord;
use persistence_sqlite::DbResult;
use rusqlite::Connection;

use crate::file_service::visibility;

pub fn records_to_file_entries(
    records: &[MftRecord],
    data_source_id: &DataSourceId,
) -> Vec<FileEntry> {
    records
        .iter()
        .filter(|record| record.is_valid && (!record.name.is_empty() || record.record_number == 5))
        .map(|record| record_to_file_entry(record, data_source_id))
        .collect()
}

fn record_to_file_entry(record: &MftRecord, data_source_id: &DataSourceId) -> FileEntry {
    let is_root = record.record_number == 5;
    let name = if is_root && (record.name.is_empty() || record.name == ".") {
        "/".to_string()
    } else {
        record.name.clone()
    };
    let entry_type = if record.is_dir {
        EntryType::Directory
    } else {
        EntryType::File
    };
    FileEntry {
        id: FileEntryId(format!("mft:{}", record.record_number)),
        parent_id: (!is_root).then(|| FileEntryId(format!("mft:{}", record.parent_ref))),
        data_source_id: data_source_id.clone(),
        path: String::new(),
        name,
        entry_type,
        size: (!record.is_dir).then_some(record.size),
        ext: file_extension(record),
        deleted: record.deleted,
        hidden: record.hidden
            || visibility::inferred_hidden_name(&record.name)
            || visibility::inferred_system_name(&record.name),
        system: record.system || visibility::inferred_system_name(&record.name),
        encrypted: false,
        created_at: record.created_at,
        modified_at: record.modified_at,
        accessed_at: record.accessed_at,
        changed_at: record.changed_at,
        hash_sha256: None,
    }
}

fn file_extension(record: &MftRecord) -> Option<String> {
    if record.is_dir {
        return None;
    }
    record
        .name
        .rsplit('.')
        .next()
        .filter(|extension| *extension != record.name)
        .map(str::to_string)
}

pub fn add_entry_to_path_map(
    path_map: &mut HashMap<String, (Option<String>, String, bool)>,
    deleted_records: &mut HashSet<String>,
    entry: &FileEntry,
) {
    let record_number = entry.id.0.strip_prefix("mft:").unwrap_or(&entry.id.0);
    let parent_number = entry
        .parent_id
        .as_ref()
        .and_then(|parent| parent.0.strip_prefix("mft:").map(str::to_string));
    path_map.insert(
        record_number.to_string(),
        (
            parent_number,
            entry.name.clone(),
            entry.entry_type == EntryType::Directory,
        ),
    );
    if entry.deleted {
        deleted_records.insert(record_number.to_string());
    }
}

pub fn update_entry_paths(
    conn: &Connection,
    data_source_id: &DataSourceId,
    path_map: &HashMap<String, (Option<String>, String, bool)>,
    deleted_records: &HashSet<String>,
    partition_index: usize,
) -> DbResult<()> {
    let resolved = resolve_all_paths(path_map, deleted_records);
    persist_resolved_paths(conn, data_source_id, &resolved, partition_index)
}

fn resolve_all_paths(
    path_map: &HashMap<String, (Option<String>, String, bool)>,
    deleted_records: &HashSet<String>,
) -> HashMap<String, String> {
    let mut resolved = HashMap::with_capacity(path_map.len());
    let mut visiting = HashSet::new();
    for record in path_map.keys() {
        resolve_path(
            record,
            path_map,
            deleted_records,
            &mut resolved,
            &mut visiting,
        );
    }
    resolved
}

fn resolve_path(
    record: &str,
    path_map: &HashMap<String, (Option<String>, String, bool)>,
    deleted_records: &HashSet<String>,
    resolved: &mut HashMap<String, String>,
    visiting: &mut HashSet<String>,
) -> String {
    if let Some(path) = resolved.get(record) {
        return path.clone();
    }
    if !visiting.insert(record.to_string()) {
        tracing::warn!("Cycle detected in path chain at record {}", record);
        return String::new();
    }
    let Some((parent, name, _)) = path_map.get(record) else {
        visiting.remove(record);
        return String::new();
    };
    let path = match parent {
        Some(parent) if path_map.contains_key(parent) => {
            let parent_path = resolve_path(parent, path_map, deleted_records, resolved, visiting);
            if parent_path.is_empty() {
                name.clone()
            } else {
                format!("{parent_path}/{name}")
            }
        }
        _ if record != "5" && deleted_records.contains(record) => {
            format!("/$DeletedOrphans/{record}-{name}")
        }
        _ if record == "5" || parent.is_none() => name.clone(),
        _ => format!("/Unresolved/{name}"),
    };
    resolved.insert(record.to_string(), path.clone());
    visiting.remove(record);
    path
}

fn persist_resolved_paths(
    conn: &Connection,
    data_source_id: &DataSourceId,
    resolved: &HashMap<String, String>,
    partition_index: usize,
) -> DbResult<()> {
    let prefix = format!("[P{partition_index}]");
    let tx = conn.unchecked_transaction()?;
    {
        let mut statement =
            tx.prepare("UPDATE file_entries SET path = ?1 WHERE id = ?2 AND data_source_id = ?3")?;
        for (record_number, path) in resolved {
            let prefixed_path = prefixed_path(&prefix, path);
            statement.execute(rusqlite::params![
                prefixed_path,
                format!("mft:{record_number}"),
                data_source_id.0
            ])?;
        }
    }
    tx.commit()?;
    Ok(())
}

fn prefixed_path(prefix: &str, path: &str) -> String {
    let trimmed = path.trim_start_matches('/');
    if trimmed.is_empty() {
        prefix.to_string()
    } else {
        format!("{prefix}/{trimmed}")
    }
}

pub fn update_entry_parent_ids(
    conn: &Connection,
    data_source_id: &DataSourceId,
    path_map: &HashMap<String, (Option<String>, String, bool)>,
) -> DbResult<()> {
    let tx = conn.unchecked_transaction()?;
    {
        let mut statement = tx.prepare(
            "UPDATE file_entries SET parent_id = ?1 WHERE id = ?2 AND data_source_id = ?3",
        )?;
        for (record_number, (parent, _, _)) in path_map {
            statement.execute(rusqlite::params![
                mft_parent_entry_id(record_number, parent.as_deref(), path_map),
                format!("mft:{record_number}"),
                data_source_id.0
            ])?;
        }
    }
    tx.commit()?;
    Ok(())
}

pub fn mft_parent_entry_id(
    record_number: &str,
    parent_number: Option<&str>,
    path_map: &HashMap<String, (Option<String>, String, bool)>,
) -> Option<String> {
    if record_number == "5" {
        return None;
    }
    match parent_number {
        Some(parent) if parent != record_number && path_map.contains_key(parent) => {
            Some(format!("mft:{parent}"))
        }
        _ if path_map.contains_key("5") => Some("mft:5".to_string()),
        _ => None,
    }
}
