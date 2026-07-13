use persistence_sqlite::repositories::ceph_rocksdb_repo::{
    CephRocksdbAggregate, CephRocksdbColumnFamilyRecord, CephRocksdbLiveSstRecord,
    CephRocksdbManifestRecord,
};
use rocksdb_wire::{ColumnFamilyState, LiveFile, ManifestSnapshot, NewFileFormat};
use transport::CommandError;

use super::ceph_rocksdb_control_files::RocksdbControlFiles;

pub(super) fn build_rocksdb_aggregate(
    data_source_id: &str,
    inventory_id: &str,
    control: RocksdbControlFiles,
    snapshot: ManifestSnapshot,
) -> Result<CephRocksdbAggregate, CommandError> {
    let comparator_name = utf8_metadata(
        snapshot
            .comparator
            .clone()
            .ok_or_else(|| record_error("RocksDB default column family has no comparator"))?,
        "default column family comparator",
    )?;
    let logical_edit_count = u32::try_from(snapshot.logical_edit_count)
        .map_err(|_| record_error("RocksDB logical edit count exceeds u32"))?;
    let max_column_family_id = snapshot.max_column_family_id;
    let last_sequence = snapshot.last_sequence;
    let manifest = CephRocksdbManifestRecord {
        inventory_id: inventory_id.to_string(),
        data_source_id: data_source_id.to_string(),
        active_manifest_path: control.manifest_path,
        identity_uuid: control.identity_uuid,
        manifest_file_number: control.manifest_file_number,
        manifest_file_size: control.manifest_file_size,
        logical_edit_count,
        comparator_name,
        last_sequence,
        next_file_number: snapshot.next_file_number,
        log_number: snapshot.log_number,
        prev_log_number: snapshot.previous_log_number,
        max_column_family_id,
        min_log_number_to_keep: (snapshot.min_log_number_to_keep > 0)
            .then_some(snapshot.min_log_number_to_keep),
    };
    let column_families = snapshot
        .column_families
        .into_iter()
        .map(|column_family| map_column_family(inventory_id, column_family))
        .collect::<Result<Vec<_>, _>>()?;
    let live_ssts = snapshot
        .live_files
        .into_iter()
        .map(|file| map_live_file(inventory_id, file))
        .collect();
    Ok(CephRocksdbAggregate {
        manifest,
        column_families,
        live_ssts,
    })
}

fn map_column_family(
    inventory_id: &str,
    column_family: ColumnFamilyState,
) -> Result<CephRocksdbColumnFamilyRecord, CommandError> {
    let column_family_id = column_family.id;
    let name = utf8_metadata(column_family.name, "column family name")?;
    let comparator_name = utf8_metadata(
        column_family.comparator.ok_or_else(|| {
            record_error(format!(
                "RocksDB column family {column_family_id} has no comparator"
            ))
        })?,
        "column family comparator",
    )?;
    Ok(CephRocksdbColumnFamilyRecord {
        inventory_id: inventory_id.to_string(),
        column_family_id,
        name,
        comparator_name,
        dropped: column_family.dropped,
    })
}

fn map_live_file(inventory_id: &str, file: LiveFile) -> CephRocksdbLiveSstRecord {
    let (smallest_sequence, largest_sequence) = match file.format {
        NewFileFormat::NewFile => (None, None),
        NewFileFormat::NewFile2 | NewFileFormat::NewFile3 | NewFileFormat::NewFile4 => (
            Some(file.smallest_sequence_number),
            Some(file.largest_sequence_number),
        ),
    };
    CephRocksdbLiveSstRecord {
        inventory_id: inventory_id.to_string(),
        column_family_id: file.column_family_id,
        level: file.level,
        file_number: file.file_number,
        path_id: file.path_id,
        format: format_name(file.format).to_string(),
        file_size: file.file_size,
        smallest_sequence,
        largest_sequence,
        smallest_internal_key_length: file.smallest.encoded_length,
        largest_internal_key_length: file.largest.encoded_length,
    }
}

fn format_name(format: NewFileFormat) -> &'static str {
    match format {
        NewFileFormat::NewFile => "newFile",
        NewFileFormat::NewFile2 => "newFile2",
        NewFileFormat::NewFile3 => "newFile3",
        NewFileFormat::NewFile4 => "newFile4",
    }
}

fn record_error(error: impl std::fmt::Display) -> CommandError {
    CommandError::parser(format!("RocksDB inventory mapping failed: {error}"))
}

fn utf8_metadata(bytes: Vec<u8>, field: &'static str) -> Result<String, CommandError> {
    String::from_utf8(bytes)
        .map_err(|_| record_error(format!("RocksDB {field} is not valid UTF-8")))
}

#[cfg(test)]
#[path = "../../tests/unit/import_pipeline/ceph_rocksdb_records.rs"]
mod tests;
