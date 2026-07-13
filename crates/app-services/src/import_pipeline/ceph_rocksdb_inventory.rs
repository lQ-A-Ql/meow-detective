use persistence_sqlite::repositories::ceph_rocksdb_repo::CephRocksdbAggregate;
use transport::CommandError;

use super::ceph_bluefs_file_reader::BluefsExtentReader;
use super::ceph_bluefs_replay::BluefsReplaySnapshot;

pub(super) fn inventory_rocksdb_manifest(
    reader: &mut BluefsExtentReader<'_>,
    replay: &BluefsReplaySnapshot,
    data_source_id: &str,
    inventory_id: &str,
) -> Result<CephRocksdbAggregate, CommandError> {
    let control = super::ceph_rocksdb_control_files::read_rocksdb_control_files(reader, replay)?;
    let snapshot = rocksdb_wire::decode_manifest(
        &control.manifest_bytes,
        rocksdb_wire::ManifestDecodeLimits::default(),
    )
    .map_err(map_manifest_error)?;
    super::ceph_rocksdb_records::build_rocksdb_aggregate(
        data_source_id,
        inventory_id,
        control,
        snapshot,
    )
}

fn map_manifest_error(error: rocksdb_wire::RocksDbWireError) -> CommandError {
    let message = format!("RocksDB MANIFEST decode failed: {error}");
    match error {
        rocksdb_wire::RocksDbWireError::UnsupportedWalCompressionRecord { .. }
        | rocksdb_wire::RocksDbWireError::UnknownMandatoryTag { .. }
        | rocksdb_wire::RocksDbWireError::UnknownMandatoryCustomTag { .. } => {
            CommandError::unsupported(message)
        }
        _ => CommandError::parser(message),
    }
}

#[cfg(test)]
#[path = "../../tests/unit/import_pipeline/ceph_rocksdb_inventory.rs"]
mod tests;
