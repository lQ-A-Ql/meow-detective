use domain::DataSourceId;
use persistence_source::fjall::{PartitionRecord, PersistMode};
use thiserror::Error;

pub(crate) use persistence_source::runtime::partition_marker::partition_projection_marker;

#[derive(Debug, Error)]
pub enum PartitionStorageError {
    #[error("partition metadata serialization failed: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("Fjall partition storage failed: {0}")]
    Storage(#[from] crate::storage_runtime::StorageRuntimeError),
}

pub fn replace_from_probe(
    source: &crate::storage_runtime::ReadWriteSourceStorageHandle,
    data_source_id: &DataSourceId,
    partitions: &[crate::datasource_service::PartitionRecord],
    gpt_disk_guid: Option<&str>,
) -> Result<(), PartitionStorageError> {
    let records = partitions
        .iter()
        .map(|partition| {
            Ok(PartitionRecord {
                id: format!("partition:{}", partition.index),
                data_source_id: data_source_id.0.clone(),
                partition_index: partition.index as u32,
                name: partition.name.clone(),
                kind_label: partition.kind_label.clone(),
                status: partition_status(partition.status).to_owned(),
                type_guid: partition.type_guid.clone(),
                offset: partition.offset,
                length: partition.length,
                filesystem: partition.filesystem.map(filesystem_name),
                unlock_hint: (partition.status
                    == crate::datasource_service::PartitionStatus::EncryptedBitLocker)
                    .then(|| "BitLocker 分区需要先解锁后才能浏览文件内容。".to_owned()),
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
                lvm_pv_offsets_json: partition
                    .lvm_identity
                    .as_ref()
                    .map(|identity| serde_json::to_string(&identity.pv_offsets))
                    .transpose()?,
                lvm_pv_sources_json: partition
                    .lvm_identity
                    .as_ref()
                    .filter(|identity| !identity.pv_sources.is_empty())
                    .map(|identity| serde_json::to_string(&identity.pv_sources))
                    .transpose()?,
                gpt_disk_guid: gpt_disk_guid.map(str::to_owned),
                gpt_partition_uuid: partition.gpt_partition_uuid.clone(),
                filesystem_uuid: partition.filesystem_uuid.clone(),
            })
        })
        .collect::<Result<Vec<_>, serde_json::Error>>()?;
    source.partition_replace_by_source(&data_source_id.0, &records, PersistMode::Buffer)?;
    source.partition_projection_publish(&data_source_id.0)?;
    Ok(())
}

fn partition_status(status: crate::datasource_service::PartitionStatus) -> &'static str {
    match status {
        crate::datasource_service::PartitionStatus::Supported => "supported",
        crate::datasource_service::PartitionStatus::Expanded => "redirected",
        crate::datasource_service::PartitionStatus::EncryptedBitLocker => "locked",
        crate::datasource_service::PartitionStatus::Unsupported => "unsupported",
    }
}

fn filesystem_name(kind: crate::datasource_service::ImageFilesystemKind) -> String {
    match kind {
        crate::datasource_service::ImageFilesystemKind::Ntfs => "NTFS",
        crate::datasource_service::ImageFilesystemKind::Fat => "FAT",
        crate::datasource_service::ImageFilesystemKind::Iso9660 => "ISO9660",
        crate::datasource_service::ImageFilesystemKind::BitLocker => "BitLocker",
        crate::datasource_service::ImageFilesystemKind::Ext4 => "Ext4",
        crate::datasource_service::ImageFilesystemKind::F2fs => "F2FS",
        crate::datasource_service::ImageFilesystemKind::Erofs => "EROFS",
        crate::datasource_service::ImageFilesystemKind::Xfs => "XFS",
        crate::datasource_service::ImageFilesystemKind::Btrfs => "Btrfs",
        crate::datasource_service::ImageFilesystemKind::LvmPool => "LVM",
    }
    .to_owned()
}
