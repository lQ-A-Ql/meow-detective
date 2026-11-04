use crate::datasource_service;
use domain::{DataSourceId, EntryType, FileEntry, FileEntryId};
use persistence_sqlite::{repositories::file_repo::FileRepo, DbResult};
use rusqlite::Connection;
use std::sync::atomic::AtomicBool;
use uuid::Uuid;

pub(crate) const PARTITION_PLACEHOLDER_PREFIX: &str = "__partition_placeholder__/";

pub fn insert_partition_placeholder_root(
    conn: &Connection,
    data_source_id: &DataSourceId,
    partition_index: usize,
    root_name: &str,
    status: &str,
) -> DbResult<FileEntryId> {
    let root_id = FileEntryId(Uuid::new_v4().to_string());
    let root_entry = FileEntry {
        id: root_id.clone(),
        parent_id: None,
        data_source_id: data_source_id.clone(),
        path: format!("{PARTITION_PLACEHOLDER_PREFIX}{partition_index}/{status}"),
        name: root_name.to_string(),
        entry_type: EntryType::Directory,
        size: None,
        ext: None,
        deleted: false,
        hidden: false,
        system: false,
        encrypted: false,
        read_only: false,
        archive: false,
        unix_mode: None,
        created_at: None,
        modified_at: None,
        accessed_at: None,
        changed_at: None,
        hash_sha256: None,
    };

    let tx = conn.unchecked_transaction()?;
    {
        let repo = FileRepo::new(&tx);
        repo.insert_batch_unchecked(&[root_entry])?;
        repo.set_partition_index_by_id(&root_id, partition_index)?;
    }
    tx.commit()?;
    Ok(root_id)
}

pub fn remove_partition_placeholder_root(
    conn: &Connection,
    data_source_id: &DataSourceId,
    partition_index: usize,
) -> DbResult<usize> {
    let pattern = format!("{PARTITION_PLACEHOLDER_PREFIX}{partition_index}/*");
    let deleted =
        FileRepo::new(conn).remove_partition_placeholder_root(&data_source_id.0, &pattern)?;
    Ok(deleted)
}

pub fn replace_placeholder_root_with_real(
    conn: &Connection,
    placeholder_id: &FileEntryId,
    fs: &dyn evidence_core::FileSystemReader,
    root_name_override: Option<&str>,
    progress_fn: Option<&dyn Fn(u32)>,
) -> DbResult<super::EnumerationStats> {
    replace_placeholder_root_with_real_and_cancel(
        conn,
        placeholder_id,
        fs,
        root_name_override,
        progress_fn,
        None,
    )
}

pub fn replace_placeholder_root_with_real_and_cancel(
    conn: &Connection,
    placeholder_id: &FileEntryId,
    fs: &dyn evidence_core::FileSystemReader,
    root_name_override: Option<&str>,
    progress_fn: Option<&dyn Fn(u32)>,
    cancel_token: Option<&AtomicBool>,
) -> DbResult<super::EnumerationStats> {
    let root = fs.root().map_err(|e| {
        persistence_sqlite::DbError::System(format!("Failed to read filesystem root: {}", e))
    })?;

    let tx = conn.unchecked_transaction()?;
    let result = {
        let repo = FileRepo::new(&tx);
        let Some(mut root_entry) = repo.find_by_id(placeholder_id)? else {
            return Err(persistence_sqlite::DbError::System(
                "Partition placeholder root not found".to_string(),
            ));
        };

        root_entry.path = String::new();
        root_entry.name = root_name_override.unwrap_or(&root.name).to_string();
        root_entry.created_at = root.created_at;
        root_entry.modified_at = root.modified_at;
        root_entry.accessed_at = root.accessed_at;
        root_entry.hidden = root.hidden;
        root_entry.system = root.system;
        let partition_index = repo
            .find_partition_index_by_id(placeholder_id)?
            .ok_or_else(|| {
                persistence_sqlite::DbError::System(format!(
                    "Partition placeholder '{}' is missing partition_index",
                    placeholder_id.0
                ))
            })?;

        FileRepo::new(&tx).update_root_metadata(&root_entry)?;

        let stats = super::enumeration::walk_and_insert_children(
            &repo,
            fs,
            &root_entry.data_source_id,
            root_entry.id,
            progress_fn,
            cancel_token,
        )?;
        repo.assign_partition_index_to_subtree(placeholder_id, partition_index)?;
        Ok(stats)
    };

    match result {
        Ok(stats) => {
            tx.commit()?;
            Ok(stats)
        }
        Err(error) => {
            tx.rollback().ok();
            Err(error)
        }
    }
}

pub(crate) fn directory_depth(entry: &FileEntry) -> u32 {
    if entry.path.is_empty() {
        return 0;
    }

    std::path::Path::new(&entry.path)
        .components()
        .filter(|component| matches!(component, std::path::Component::Normal(_)))
        .count() as u32
}

pub(crate) fn partition_placeholder_status(entry: &FileEntry) -> Option<&str> {
    let rest = entry.path.strip_prefix(PARTITION_PLACEHOLDER_PREFIX)?;
    if rest.is_empty() {
        return None;
    }
    match rest.split_once('/') {
        Some((index, status))
            if index.chars().all(|c| c.is_ascii_digit()) && !status.is_empty() =>
        {
            Some(status)
        }
        _ => Some(rest),
    }
}

pub(crate) fn looks_like_partition_root_name(name: &str) -> bool {
    name.starts_with("Partition ") || name.starts_with("Volume")
}

pub(crate) fn looks_like_raw_fs_root_name(name: &str) -> bool {
    matches!(name.trim(), "\\" | "/" | ".")
}

pub(crate) fn mft_entry_partition_index(entry_id: &str) -> Option<usize> {
    let mut parts = entry_id.split(':');
    match (parts.next(), parts.next(), parts.next(), parts.next()) {
        (Some("mft"), Some(partition), Some(_record), None) => partition.parse().ok(),
        _ => None,
    }
}

pub(crate) fn normalized_bare_root_name_from_partitions(
    entry: &FileEntry,
    partitions: &[crate::partition_query_service::PartitionRecord],
) -> String {
    let resolved = mft_entry_partition_index(&entry.id.0)
        .and_then(|idx| {
            partitions
                .iter()
                .find(|p| p.partition_index as usize == idx)
        })
        .or_else(|| {
            if partitions.len() == 1 {
                partitions.first()
            } else {
                None
            }
        });

    match resolved {
        Some(p) => datasource_service::partition_display_name(
            p.partition_index as usize,
            &p.kind_label,
            None,
            None,
        ),
        None => "Partition ? (UNKNOWN)".to_string(),
    }
}

#[cfg(test)]
#[path = "../../tests/unit/file_service/partition_roots.rs"]
mod tests;
