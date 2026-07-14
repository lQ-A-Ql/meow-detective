use persistence_sqlite::repositories::ceph_rocksdb_sst_repo::{
    CephRocksdbSstRecord, ROCKSDB_BLOCK_BASED_TABLE_MAGIC_HEX,
};
use transport::CommandError;

use super::ceph_rocksdb_sst_locator::LocatedRocksdbSst;

pub(super) fn build_sst_record(
    located: &LocatedRocksdbSst<'_>,
    inspection: rocksdb_wire::SstInspection,
) -> Result<CephRocksdbSstRecord, CommandError> {
    validate_identity(located, &inspection)?;
    if !inspection.census.complete {
        return Err(record_error(format!(
            "live SST {} exceeded the complete key-space census budget",
            located.live.file_number
        )));
    }
    let key_space_summary_json =
        serialize_key_space_summary(&inspection.census).map_err(record_error)?;
    Ok(CephRocksdbSstRecord {
        inventory_id: located.live.inventory_id.clone(),
        file_number: located.live.file_number,
        column_family_id: located.live.column_family_id,
        level: located.live.level,
        bluefs_path: located.path.clone(),
        file_size: inspection.file_size,
        table_magic_hex: ROCKSDB_BLOCK_BASED_TABLE_MAGIC_HEX.to_string(),
        format_version: inspection.footer.format_version,
        checksum_type: checksum_name(inspection.footer.checksum_type).to_string(),
        metaindex_offset: inspection.footer.metaindex_handle.offset,
        metaindex_size: inspection.footer.metaindex_handle.size,
        index_offset: inspection.footer.index_handle.offset,
        index_size: inspection.footer.index_handle.size,
        data_block_count: inspection.properties.num_data_blocks,
        entry_count: inspection.properties.num_entries,
        deletion_count: inspection.properties.deleted_keys,
        merge_operand_count: inspection.properties.merge_operands,
        range_deletion_count: inspection.properties.num_range_deletions,
        raw_key_size: inspection.properties.raw_key_size,
        raw_value_size: inspection.properties.raw_value_size,
        data_size: inspection.properties.data_size,
        properties_index_size: inspection.properties.index_size,
        filter_size: inspection.properties.filter_size,
        compression_name: inspection.properties.compression_name,
        comparator_name: inspection.properties.comparator_name,
        column_family_name: inspection.properties.column_family_name,
        original_file_number: inspection.properties.original_file_number,
        db_identity: inspection.properties.db_identity,
        db_session_identity: inspection.properties.db_session_identity,
        key_space_summary_version: inspection.census.version,
        key_space_summary_json,
        scan_complete: true,
    })
}

fn validate_identity(
    located: &LocatedRocksdbSst<'_>,
    inspection: &rocksdb_wire::SstInspection,
) -> Result<(), CommandError> {
    let properties = &inspection.properties;
    let sequence_matches = located
        .live
        .smallest_sequence
        .is_some_and(|value| value == inspection.smallest_sequence)
        && located
            .live
            .largest_sequence
            .is_some_and(|value| value == inspection.largest_sequence);
    if inspection.file_size != located.live.file_size
        || inspection.footer.table_magic != rocksdb_wire::BLOCK_BASED_TABLE_MAGIC
        || properties.column_family_id != located.live.column_family_id
        || properties.column_family_name != located.column_family.name
        || properties.comparator_name != located.column_family.comparator_name
        || properties.original_file_number != located.live.file_number
        || located
            .manifest
            .identity_uuid
            .as_deref()
            .is_some_and(|identity| properties.db_identity.as_deref() != Some(identity))
        || !sequence_matches
    {
        return Err(record_error(format!(
            "live SST {} properties do not match MANIFEST identity",
            located.live.file_number
        )));
    }
    Ok(())
}

fn checksum_name(checksum: rocksdb_wire::ChecksumType) -> &'static str {
    match checksum {
        rocksdb_wire::ChecksumType::Xxh3 => "xxh3",
    }
}

fn serialize_key_space_summary(
    census: &rocksdb_wire::KeySpaceCensus,
) -> Result<String, serde_json::Error> {
    let buckets = census
        .buckets
        .iter()
        .map(|bucket| {
            serde_json::json!({
                "name": bucket.name,
                "count": bucket.entries,
                "minKeyLength": bucket.min_user_key_length,
                "maxKeyLength": bucket.max_user_key_length,
            })
        })
        .collect::<Vec<_>>();
    serde_json::to_string(&serde_json::json!({
        "version": census.version,
        "complete": census.complete,
        "scannedEntries": census.scanned_entries,
        "scannedDecompressedBytes": census.scanned_decompressed_bytes,
        "buckets": buckets,
    }))
}

fn record_error(error: impl std::fmt::Display) -> CommandError {
    CommandError::parser(format!("RocksDB live-SST inventory failed: {error}"))
}
