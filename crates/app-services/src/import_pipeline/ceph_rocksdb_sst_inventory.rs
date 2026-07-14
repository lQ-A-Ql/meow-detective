use std::sync::atomic::AtomicBool;

use persistence_sqlite::repositories::ceph_rocksdb_sst_repo::CephRocksdbSstRecord;
use transport::CommandError;

use super::ceph_bluefs_file_reader::BluefsExtentReader;
use super::ceph_rocksdb_sharding::RocksdbShardingDefinition;
use super::ceph_rocksdb_spool::{
    RocksdbRecoverySpool, SpoolPointInput, SpoolProvenance, SpoolRangeInput, SpoolSourceKind,
};
use super::ceph_rocksdb_sst_locator::LocatedRocksdbSst;
use super::ceph_rocksdb_sst_reader::BluefsSstRangeReader;

pub(super) fn inventory_live_rocksdb_ssts(
    reader: &mut BluefsExtentReader<'_>,
    sharding: &RocksdbShardingDefinition,
    located: &[LocatedRocksdbSst<'_>],
    cancel_token: &AtomicBool,
    spool: &mut RocksdbRecoverySpool,
) -> Result<Vec<CephRocksdbSstRecord>, CommandError> {
    located
        .iter()
        .map(|sst| inventory_live_sst(reader, sharding, sst, cancel_token, spool))
        .collect()
}

fn inventory_live_sst(
    reader: &mut BluefsExtentReader<'_>,
    sharding: &RocksdbShardingDefinition,
    located: &LocatedRocksdbSst<'_>,
    cancel_token: &AtomicBool,
    spool: &mut RocksdbRecoverySpool,
) -> Result<CephRocksdbSstRecord, CommandError> {
    let census_context = sharding.census_context(&located.column_family.name)?;
    let mut range_reader = BluefsSstRangeReader::new(reader, &located.file.fnode, cancel_token)?;
    let mut visitor = SpoolVisitor { located, spool };
    let inspected = rocksdb_wire::inspect_sst_with_visitor(
        &mut range_reader,
        located.file.fnode.size,
        rocksdb_wire::SstVisitOptions::default(),
        &census_context,
        &mut visitor,
    )
    .map_err(map_stream_error)?;
    validate_stream_matches_inspection(&inspected.inspection, &inspected.stream)?;
    super::ceph_rocksdb_sst_records::build_sst_record(located, inspected.inspection)
}

struct SpoolVisitor<'a, 'spool> {
    located: &'a LocatedRocksdbSst<'a>,
    spool: &'spool mut RocksdbRecoverySpool,
}

impl rocksdb_wire::SstEntryVisitor for SpoolVisitor<'_, '_> {
    type Error = CommandError;

    fn visit_data(&mut self, entry: rocksdb_wire::SstDataEntry<'_>) -> Result<(), Self::Error> {
        let value_type = match entry.kind {
            rocksdb_wire::SstEntryKind::Deletion => 0,
            rocksdb_wire::SstEntryKind::Value => 1,
            rocksdb_wire::SstEntryKind::Merge => 2,
            rocksdb_wire::SstEntryKind::SingleDeletion => 7,
            rocksdb_wire::SstEntryKind::BlobIndex
            | rocksdb_wire::SstEntryKind::DeletionWithTimestamp
            | rocksdb_wire::SstEntryKind::WideColumnEntity => {
                return Err(CommandError::unsupported(format!(
                    "RocksDB live SST {} contains unsupported entry type {:#04x}",
                    self.located.live.file_number,
                    entry.kind.value_type()
                )));
            }
        };
        self.spool.insert_point(SpoolPointInput {
            column_family_id: entry.column_family_id,
            user_key: entry.user_key,
            sequence: entry.sequence,
            value_type,
            value: entry.value,
            provenance: SpoolProvenance {
                source_kind: SpoolSourceKind::Sst,
                file_number: self.located.live.file_number,
                level: Some(self.located.live.level),
                physical_offset: entry.block_handle.offset,
                primary_ordinal: entry.block_ordinal,
                secondary_ordinal: entry.entry_ordinal,
            },
        })
    }

    fn visit_range_deletion(
        &mut self,
        entry: rocksdb_wire::SstRangeDeletionEntry<'_>,
    ) -> Result<(), Self::Error> {
        self.spool.insert_range(SpoolRangeInput {
            column_family_id: entry.column_family_id,
            start_key: entry.start_user_key,
            end_key: entry.end_user_key,
            sequence: entry.sequence,
            provenance: SpoolProvenance {
                source_kind: SpoolSourceKind::Sst,
                file_number: self.located.live.file_number,
                level: Some(self.located.live.level),
                physical_offset: entry.block_handle.offset,
                primary_ordinal: 0,
                secondary_ordinal: entry.entry_ordinal,
            },
        })
    }
}

fn validate_stream_matches_inspection(
    inspection: &rocksdb_wire::SstInspection,
    stream: &rocksdb_wire::SstEntryStreamSummary,
) -> Result<(), CommandError> {
    if stream.file_size != inspection.file_size
        || stream.properties != inspection.properties
        || stream.data_block_count != inspection.data_blocks.len() as u64
        || stream.counts != inspection.counts
        || stream.raw_key_size != inspection.raw_key_size
        || stream.raw_value_size != inspection.raw_value_size
        || stream.smallest_sequence != inspection.smallest_sequence
        || stream.largest_sequence != inspection.largest_sequence
    {
        return Err(CommandError::parser(
            "RocksDB SST inventory and entry stream produced inconsistent validation summaries",
        ));
    }
    Ok(())
}

fn map_stream_error(error: rocksdb_wire::SstVisitError<CommandError>) -> CommandError {
    match error {
        rocksdb_wire::SstVisitError::Wire(error) => map_sst_error(error),
        rocksdb_wire::SstVisitError::Visitor(error) => error,
    }
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
