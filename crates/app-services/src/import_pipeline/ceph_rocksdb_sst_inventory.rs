use std::sync::atomic::AtomicBool;

use persistence_sqlite::repositories::ceph_rocksdb_sst_repo::CephRocksdbSstRecord;
use transport::CommandError;

use super::ceph_bluefs_file_reader::BluefsExtentReader;
use super::ceph_rocksdb_sharding::RocksdbShardingDefinition;
use super::ceph_rocksdb_sst_locator::LocatedRocksdbSst;
use super::ceph_rocksdb_sst_reader::BluefsSstRangeReader;

pub(super) fn inventory_live_rocksdb_ssts(
    reader: &mut BluefsExtentReader<'_>,
    sharding: &RocksdbShardingDefinition,
    located: &[LocatedRocksdbSst<'_>],
    cancel_token: &AtomicBool,
) -> Result<Vec<CephRocksdbSstRecord>, CommandError> {
    located
        .iter()
        .map(|sst| inventory_live_sst(reader, sharding, sst, cancel_token))
        .collect()
}

fn inventory_live_sst(
    reader: &mut BluefsExtentReader<'_>,
    sharding: &RocksdbShardingDefinition,
    located: &LocatedRocksdbSst<'_>,
    cancel_token: &AtomicBool,
) -> Result<CephRocksdbSstRecord, CommandError> {
    let census_context = sharding.census_context(&located.column_family.name)?;
    let mut range_reader = BluefsSstRangeReader::new(reader, &located.file.fnode, cancel_token)?;
    let inspection = rocksdb_wire::inspect_sst(
        &mut range_reader,
        located.file.fnode.size,
        rocksdb_wire::SstReadOptions::default(),
        &census_context,
    )
    .map_err(map_sst_error)?;
    super::ceph_rocksdb_sst_records::build_sst_record(located, inspection)
}

fn map_sst_error(error: rocksdb_wire::RocksDbWireError) -> CommandError {
    if matches!(
        &error,
        rocksdb_wire::RocksDbWireError::SstStoredBlockLimit { .. }
            | rocksdb_wire::RocksDbWireError::SstDecompressedBlockLimit { .. }
            | rocksdb_wire::RocksDbWireError::SstAuxiliaryMetadataLimit { .. }
            | rocksdb_wire::RocksDbWireError::SstEntryLimit { .. }
            | rocksdb_wire::RocksDbWireError::SstCensusEntryLimit { .. }
            | rocksdb_wire::RocksDbWireError::SstCensusDecompressedLimit { .. }
            | rocksdb_wire::RocksDbWireError::SstKeyLengthLimit { .. }
            | rocksdb_wire::RocksDbWireError::SstValueLengthLimit { .. }
    ) {
        return CommandError::unsupported(format!(
            "RocksDB SST exceeds bounded inspection capability: {error}"
        ));
    }
    let message = format!("RocksDB SST decode failed: {error}");
    match error {
        rocksdb_wire::RocksDbWireError::UnsupportedSstMagic { .. }
        | rocksdb_wire::RocksDbWireError::UnsupportedSstFormatVersion { .. }
        | rocksdb_wire::RocksDbWireError::UnsupportedSstChecksum { .. }
        | rocksdb_wire::RocksDbWireError::UnsupportedSstCompression { .. }
        | rocksdb_wire::RocksDbWireError::UnsupportedSstFeature { .. }
        | rocksdb_wire::RocksDbWireError::UnsupportedSstEntryType { .. } => {
            CommandError::unsupported(message)
        }
        rocksdb_wire::RocksDbWireError::SstSourceRead { .. } => CommandError::io(message),
        rocksdb_wire::RocksDbWireError::SstInspectionCancelled => {
            CommandError::conflict("Import cancelled during RocksDB SST inventory")
        }
        _ => CommandError::parser(message),
    }
}

#[cfg(test)]
#[path = "../../tests/unit/import_pipeline/ceph_rocksdb_sst_inventory.rs"]
mod tests;
