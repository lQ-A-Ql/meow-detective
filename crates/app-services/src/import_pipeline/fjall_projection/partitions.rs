use crate::storage_runtime::ReadWriteSourceStorageHandle;
use persistence_fjall::{PartitionRecord, PersistMode};
use persistence_sqlite::repositories::partition_repo::PartitionRepo;
use rusqlite::Connection;
use transport::CommandError;

pub(crate) fn mirror_source_partitions(
    connection: &Connection,
    source: &ReadWriteSourceStorageHandle,
    source_id: &str,
) -> Result<(), CommandError> {
    let records = PartitionRepo::new(connection)
        .find_by_data_source(source_id)
        .map_err(|error| CommandError::internal(error.to_string()))?
        .into_iter()
        .map(|record| PartitionRecord {
            id: record.id,
            data_source_id: record.data_source_id,
            partition_index: record.partition_index,
            name: record.name,
            kind_label: record.kind_label,
            status: record.status,
            type_guid: record.type_guid,
            offset: record.offset,
            length: record.length,
            filesystem: record.filesystem,
            unlock_hint: record.unlock_hint,
            lvm_vg_uuid: record.lvm_vg_uuid,
            lvm_vg_name: record.lvm_vg_name,
            lvm_lv_uuid: record.lvm_lv_uuid,
            lvm_lv_name: record.lvm_lv_name,
            lvm_pv_offsets_json: record.lvm_pv_offsets_json,
            lvm_pv_sources_json: record.lvm_pv_sources_json,
        })
        .collect::<Vec<_>>();
    source
        .partition_replace_by_source(source_id, &records, PersistMode::Buffer)
        .map_err(|error| CommandError::internal(error.to_string()))
}
