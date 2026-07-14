use std::sync::atomic::AtomicBool;

use persistence_sqlite::repositories::ceph_rocksdb_repo::CephRocksdbAggregate;
use persistence_sqlite::repositories::ceph_rocksdb_sst_repo::CephRocksdbSstRecord;
use transport::CommandError;

use super::ceph_bluefs_file_reader::BluefsExtentReader;
use super::ceph_bluefs_replay::BluefsReplaySnapshot;

pub(super) fn inventory_rocksdb_manifest(
    reader: &mut BluefsExtentReader<'_>,
    replay: &BluefsReplaySnapshot,
    data_source_id: &str,
    inventory_id: &str,
    cancel_token: &AtomicBool,
) -> Result<RocksdbInventoryAggregate, CommandError> {
    let control = super::ceph_rocksdb_control_files::read_rocksdb_control_files(reader, replay)?;
    let snapshot = rocksdb_wire::decode_manifest(
        &control.manifest_bytes,
        rocksdb_wire::ManifestDecodeLimits::default(),
    )
    .map_err(map_manifest_error)?;
    let aggregate = super::ceph_rocksdb_records::build_rocksdb_aggregate(
        data_source_id,
        inventory_id,
        control,
        snapshot,
    )?;
    let sharding =
        super::ceph_rocksdb_sharding::read_rocksdb_sharding_definition(reader, replay, &aggregate)?;
    let located = super::ceph_rocksdb_sst_locator::locate_live_rocksdb_ssts(replay, &aggregate)?;
    let ssts = super::ceph_rocksdb_sst_inventory::inventory_live_rocksdb_ssts(
        reader,
        &sharding,
        &located,
        cancel_token,
    )?;
    Ok(RocksdbInventoryAggregate {
        manifest: aggregate,
        ssts,
    })
}

pub(super) struct RocksdbInventoryAggregate {
    pub(super) manifest: CephRocksdbAggregate,
    pub(super) ssts: Vec<CephRocksdbSstRecord>,
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
