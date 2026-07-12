use crate::{
    datasource_service::{self, ImageFilesystemKind},
    file_service::FileServiceError,
};
use domain::{DataSourceId, EntryType, FileEntry, FileEntryId};
use persistence_sqlite::{
    repositories::{file_repo::FileRepo, partition_repo::PartitionRepo},
    DbResult,
};
use rusqlite::Connection;
use uuid::Uuid;

pub(crate) const PARTITION_PLACEHOLDER_PREFIX: &str = "__partition_placeholder__/";

pub fn insert_partition_placeholder_root(
    conn: &Connection,
    data_source_id: &DataSourceId,
    partition_index: usize,
    root_name: &str,
    status: &str,
) -> DbResult<FileEntryId> {
    let repo = FileRepo::new(conn);
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
        created_at: None,
        modified_at: None,
        accessed_at: None,
        changed_at: None,
        hash_sha256: None,
    };

    repo.insert_batch(&[root_entry])?;
    Ok(root_id)
}

pub fn remove_partition_placeholder_root(
    conn: &Connection,
    data_source_id: &DataSourceId,
    partition_index: usize,
) -> DbResult<usize> {
    let pattern = format!("{PARTITION_PLACEHOLDER_PREFIX}{partition_index}/*");
    let deleted = conn.execute(
        "DELETE FROM file_entries
         WHERE data_source_id = ?1
           AND parent_id IS NULL
           AND path GLOB ?2",
        rusqlite::params![data_source_id.0, pattern],
    )?;
    Ok(deleted)
}

pub fn replace_placeholder_root_with_real(
    conn: &Connection,
    placeholder_id: &FileEntryId,
    fs: &dyn evidence_core::FileSystemReader,
    root_name_override: Option<&str>,
    progress_fn: Option<&dyn Fn(u32)>,
) -> DbResult<super::EnumerationStats> {
    let repo = FileRepo::new(conn);
    let Some(mut root_entry) = repo.find_by_id(placeholder_id)? else {
        return Err(persistence_sqlite::DbError::System(
            "Partition placeholder root not found".to_string(),
        ));
    };

    let root = fs.root().map_err(|e| {
        persistence_sqlite::DbError::System(format!("Failed to read filesystem root: {}", e))
    })?;

    root_entry.path = String::new();
    root_entry.name = root_name_override.unwrap_or(&root.name).to_string();
    root_entry.created_at = root.created_at;
    root_entry.modified_at = root.modified_at;
    root_entry.accessed_at = root.accessed_at;
    root_entry.hidden = root.hidden;
    root_entry.system = root.system;

    let root_path = root_entry.path.clone();
    let root_name = root_entry.name.clone();
    let root_hidden = root_entry.hidden as i32;
    let root_system = root_entry.system as i32;
    let root_id_value = root_entry.id.0.clone();
    let data_source_id = root_entry.data_source_id.clone();
    let root_id = root_entry.id.clone();
    conn.execute(
        "UPDATE file_entries
         SET path = ?1, name = ?2, created_at = ?3, modified_at = ?4, accessed_at = ?5,
             hidden = ?6, system = ?7
         WHERE id = ?8",
        rusqlite::params![
            root_path,
            root_name,
            root_entry.created_at.map(|dt| dt.to_rfc3339()),
            root_entry.modified_at.map(|dt| dt.to_rfc3339()),
            root_entry.accessed_at.map(|dt| dt.to_rfc3339()),
            root_hidden,
            root_system,
            root_id_value,
        ],
    )?;

    super::enumeration::walk_and_insert_children(
        &repo,
        fs,
        &data_source_id,
        root_id,
        progress_fn,
        None,
    )
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
    partitions: &[persistence_sqlite::repositories::partition_repo::DataSourcePartitionRecord],
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

pub fn store_data_source_partitions(
    conn: &Connection,
    data_source_id: &DataSourceId,
    partitions: &[crate::datasource_service::PartitionRecord],
) -> Result<(), FileServiceError> {
    let repo = PartitionRepo::new(conn);
    let records = partitions
        .iter()
        .map(|partition| {
            persistence_sqlite::repositories::partition_repo::DataSourcePartitionRecord {
                id: Uuid::new_v4().to_string(),
                data_source_id: data_source_id.0.clone(),
                partition_index: partition.index as u32,
                name: partition.name.clone(),
                kind_label: partition.kind_label.clone(),
                status: partition_status_label(partition.status).to_string(),
                type_guid: partition.type_guid.clone(),
                offset: partition.offset,
                length: partition.length,
                filesystem: partition.filesystem.map(image_filesystem_kind_label),
                unlock_hint: partition_unlock_hint(partition),
                lvm_vg_uuid: partition
                    .lvm_identity
                    .as_ref()
                    .map(|identity| identity.vg_uuid.clone()),
                lvm_vg_name: partition
                    .lvm_identity
                    .as_ref()
                    .map(|identity| identity.vg_name.clone()),
                lvm_lv_uuid: partition
                    .lvm_identity
                    .as_ref()
                    .map(|identity| identity.lv_uuid.clone()),
                lvm_lv_name: partition
                    .lvm_identity
                    .as_ref()
                    .map(|identity| identity.lv_name.clone()),
                lvm_pv_offsets_json: partition.lvm_identity.as_ref().and_then(|identity| {
                    serde_json::to_string(&identity.pv_offsets)
                        .map_err(|error| {
                            tracing::warn!(
                                partition_index = partition.index,
                                %error,
                                "Failed to serialize LVM PV offsets for partition metadata"
                            );
                        })
                        .ok()
                }),
                lvm_pv_sources_json: partition.lvm_identity.as_ref().and_then(|identity| {
                    if identity.pv_sources.is_empty() {
                        return None;
                    }
                    serde_json::to_string(&identity.pv_sources)
                        .map_err(|error| {
                            tracing::warn!(
                                partition_index = partition.index,
                                %error,
                                "Failed to serialize LVM PV sources for partition metadata"
                            );
                        })
                        .ok()
                }),
            }
        })
        .collect::<Vec<_>>();

    repo.replace_for_data_source(&data_source_id.0, &records)?;
    Ok(())
}

fn partition_status_label(status: crate::datasource_service::PartitionStatus) -> &'static str {
    match status {
        crate::datasource_service::PartitionStatus::Supported => "supported",
        crate::datasource_service::PartitionStatus::Expanded => "redirected",
        crate::datasource_service::PartitionStatus::EncryptedBitLocker => "locked",
        crate::datasource_service::PartitionStatus::Unsupported => "unsupported",
    }
}

fn image_filesystem_kind_label(kind: crate::datasource_service::ImageFilesystemKind) -> String {
    match kind {
        ImageFilesystemKind::Ntfs => "NTFS".to_string(),
        ImageFilesystemKind::Fat => "FAT".to_string(),
        ImageFilesystemKind::BitLocker => "BitLocker".to_string(),
        ImageFilesystemKind::Ext4 => "Ext4".to_string(),
        ImageFilesystemKind::Xfs => "XFS".to_string(),
        ImageFilesystemKind::Btrfs => "Btrfs".to_string(),
        ImageFilesystemKind::LvmPool => "LVM".to_string(),
    }
}

fn partition_unlock_hint(partition: &crate::datasource_service::PartitionRecord) -> Option<String> {
    if partition.status == crate::datasource_service::PartitionStatus::EncryptedBitLocker {
        Some("BitLocker 分区需要先解锁后才能浏览文件内容。".to_string())
    } else {
        None
    }
}

#[cfg(test)]
#[path = "../../tests/unit/file_service/partition_roots.rs"]
mod tests;
