use rusqlite::{params_from_iter, types::ToSql, Connection};

use crate::connection::{DbError, DbResult};

use super::batch::insert_rows;
use crate::repositories::ceph_bluestore_semantic_repo::{
    CephBluestoreBlobRecord, CephBluestoreChecksumChunkRecord, CephBluestoreCollectionRecord,
    CephBluestoreLogicalExtentRecord, CephBluestoreObjectRecord, CephBluestoreOnodeShardRecord,
    CephBluestorePhysicalExtentRecord, CephBluestoreSharedBlobRecord,
    CephBluestoreSharedBlobRefRecord,
};

pub(super) fn insert_collections(
    conn: &Connection,
    records: &[CephBluestoreCollectionRecord],
) -> DbResult<()> {
    insert_rows(
        conn,
        "INSERT INTO ceph_bluestore_collections (
            inventory_id, collection_identity, kind, pool, seed, shard, bits,
            denc_version, decode_status, deferred_reason
         ) VALUES ",
        10,
        records,
        |statement, records| {
            statement.execute(params_from_iter(records.iter().flat_map(|record| {
                [
                    &record.inventory_id as &dyn ToSql,
                    &record.collection_identity,
                    &record.kind,
                    &record.pool,
                    &record.seed,
                    &record.shard,
                    &record.bits,
                    &record.denc_version,
                    &record.decode_status,
                    &record.deferred_reason,
                ]
            })))
        },
    )
}

pub(super) fn insert_objects(
    conn: &Connection,
    records: &[CephBluestoreObjectRecord],
) -> DbResult<()> {
    insert_rows(
        conn,
        "INSERT INTO ceph_bluestore_objects (
            inventory_id, object_identity_sha256, decoded_shard, decoded_pool,
            decoded_hash, decoded_bitwise_hash, object_namespace, object_key,
            object_name, snap_hex, generation_hex, onode_denc_version, nid, size,
            flags_raw, flag_omap, flag_pgmeta_omap, flag_per_pool_omap,
            flag_per_pg_omap, flags_unknown_bits, attribute_count,
            attribute_value_bytes, attributes_sha256, expected_object_size,
            expected_write_size, allocation_hint_flags, zone_ref_count,
            extent_storage, spanning_blob_version, declared_spanning_blob_count,
            decode_status, deferred_reason, onode_shard_count, blob_count,
            logical_extent_count, physical_extent_count
         ) VALUES ",
        36,
        records,
        |statement, records| {
            statement.execute(params_from_iter(records.iter().flat_map(|record| {
                [
                    &record.inventory_id as &dyn ToSql,
                    &record.object_identity_sha256,
                    &record.decoded_shard,
                    &record.decoded_pool,
                    &record.decoded_hash,
                    &record.decoded_bitwise_hash,
                    &record.object_namespace,
                    &record.object_key,
                    &record.object_name,
                    &record.snap_hex,
                    &record.generation_hex,
                    &record.onode_denc_version,
                    &record.nid,
                    &record.size,
                    &record.flags_raw,
                    &record.flag_omap,
                    &record.flag_pgmeta_omap,
                    &record.flag_per_pool_omap,
                    &record.flag_per_pg_omap,
                    &record.flags_unknown_bits,
                    &record.attribute_count,
                    &record.attribute_value_bytes,
                    &record.attributes_sha256,
                    &record.expected_object_size,
                    &record.expected_write_size,
                    &record.allocation_hint_flags,
                    &record.zone_ref_count,
                    &record.extent_storage,
                    &record.spanning_blob_version,
                    &record.declared_spanning_blob_count,
                    &record.decode_status,
                    &record.deferred_reason,
                    &record.onode_shard_count,
                    &record.blob_count,
                    &record.logical_extent_count,
                    &record.physical_extent_count,
                ]
            })))
        },
    )
}

pub(super) fn insert_onode_shards(
    conn: &Connection,
    records: &[CephBluestoreOnodeShardRecord],
) -> DbResult<()> {
    insert_rows(
        conn,
        "INSERT INTO ceph_bluestore_onode_shards (
            inventory_id, object_identity_sha256, shard_ordinal, shard_offset,
            descriptor_bytes, payload_version, declared_extent_count,
            payload_encoded_length, decode_status, deferred_reason,
            logical_extent_count
         ) VALUES ",
        11,
        records,
        |statement, records| {
            statement.execute(params_from_iter(records.iter().flat_map(|record| {
                [
                    &record.inventory_id as &dyn ToSql,
                    &record.object_identity_sha256,
                    &record.shard_ordinal,
                    &record.shard_offset,
                    &record.descriptor_bytes,
                    &record.payload_version,
                    &record.declared_extent_count,
                    &record.payload_encoded_length,
                    &record.decode_status,
                    &record.deferred_reason,
                    &record.logical_extent_count,
                ]
            })))
        },
    )
}

pub(super) fn insert_blobs(conn: &Connection, records: &[CephBluestoreBlobRecord]) -> DbResult<()> {
    insert_rows(
        conn,
        "INSERT INTO ceph_bluestore_blobs (
            inventory_id, object_identity_sha256, blob_ordinal, blob_kind,
            blob_id_hex, shared_blob_id_hex, logical_length, on_disk_length,
            compressed_length, flags_raw, flag_legacy_mutable, flag_compressed,
            flag_checksum, flag_has_unused, flag_shared, flags_unknown_bits,
            unused_bitmap, checksum_type, checksum_order, checksum_chunk_size,
            checksum_encoded_length, checksum_value_count,
            checksum_data_crc32c, checksum_digest_sha256, use_tracker_kind,
            use_tracker_allocation_unit_size,
            use_tracker_declared_allocation_units, use_tracker_entry_count,
            use_tracker_sha256, logical_extent_count, physical_extent_count
         ) VALUES ",
        31,
        records,
        |statement, records| {
            statement.execute(params_from_iter(records.iter().flat_map(|record| {
                [
                    &record.inventory_id as &dyn ToSql,
                    &record.object_identity_sha256,
                    &record.blob_ordinal,
                    &record.blob_kind,
                    &record.blob_id_hex,
                    &record.shared_blob_id_hex,
                    &record.logical_length,
                    &record.on_disk_length,
                    &record.compressed_length,
                    &record.flags_raw,
                    &record.flag_legacy_mutable,
                    &record.flag_compressed,
                    &record.flag_checksum,
                    &record.flag_has_unused,
                    &record.flag_shared,
                    &record.flags_unknown_bits,
                    &record.unused_bitmap,
                    &record.checksum_type,
                    &record.checksum_order,
                    &record.checksum_chunk_size,
                    &record.checksum_encoded_length,
                    &record.checksum_value_count,
                    &record.checksum_data_crc32c,
                    &record.checksum_digest_sha256,
                    &record.use_tracker_kind,
                    &record.use_tracker_allocation_unit_size,
                    &record.use_tracker_declared_allocation_units,
                    &record.use_tracker_entry_count,
                    &record.use_tracker_sha256,
                    &record.logical_extent_count,
                    &record.physical_extent_count,
                ]
            })))
        },
    )
}

pub(super) fn insert_checksum_chunks(
    conn: &Connection,
    inventory_id: &str,
    objects: &[CephBluestoreObjectRecord],
    records: &[CephBluestoreChecksumChunkRecord],
) -> DbResult<()> {
    if records
        .iter()
        .any(|record| record.object_ordinal as usize >= objects.len())
    {
        return Err(DbError::System(
            "BlueStore checksum object ordinal exceeds object rows".to_string(),
        ));
    }
    insert_rows(
        conn,
        "INSERT INTO ceph_bluestore_checksum_chunks (
            inventory_id, object_identity_sha256, blob_ordinal,
            checksum_ordinal, chunk_offset, chunk_length, checksum_value_hex
         ) VALUES ",
        7,
        records,
        |statement, records| {
            let checksum_values = records
                .iter()
                .map(|record| {
                    format!(
                        "{:0width$x}",
                        record.checksum_value,
                        width = usize::from(record.checksum_value_bytes) * 2
                    )
                })
                .collect::<Vec<_>>();
            statement.execute(params_from_iter(records.iter().enumerate().flat_map(
                |(index, record)| {
                    [
                        &inventory_id as &dyn ToSql,
                        &objects[record.object_ordinal as usize].object_identity_sha256,
                        &record.blob_ordinal,
                        &record.checksum_ordinal,
                        &record.chunk_offset,
                        &record.chunk_length,
                        &checksum_values[index],
                    ]
                },
            )))
        },
    )
}

pub(super) fn insert_logical_extents(
    conn: &Connection,
    records: &[CephBluestoreLogicalExtentRecord],
) -> DbResult<()> {
    insert_rows(
        conn,
        "INSERT INTO ceph_bluestore_logical_extents (
            inventory_id, object_identity_sha256, extent_ordinal, logical_offset,
            length, blob_ordinal, blob_offset, shard_ordinal, defines_blob,
            flags_raw, flag_contiguous, flag_zero_blob_offset, flag_same_length,
            flag_spanning
         ) VALUES ",
        14,
        records,
        |statement, records| {
            statement.execute(params_from_iter(records.iter().flat_map(|record| {
                [
                    &record.inventory_id as &dyn ToSql,
                    &record.object_identity_sha256,
                    &record.extent_ordinal,
                    &record.logical_offset,
                    &record.length,
                    &record.blob_ordinal,
                    &record.blob_offset,
                    &record.shard_ordinal,
                    &record.defines_blob,
                    &record.flags_raw,
                    &record.flag_contiguous,
                    &record.flag_zero_blob_offset,
                    &record.flag_same_length,
                    &record.flag_spanning,
                ]
            })))
        },
    )
}

pub(super) fn insert_physical_extents(
    conn: &Connection,
    records: &[CephBluestorePhysicalExtentRecord],
) -> DbResult<()> {
    insert_rows(
        conn,
        "INSERT INTO ceph_bluestore_physical_extents (
            inventory_id, object_identity_sha256, blob_ordinal, extent_ordinal,
            blob_offset, device_id, physical_offset_hex, length
         ) VALUES ",
        8,
        records,
        |statement, records| {
            statement.execute(params_from_iter(records.iter().flat_map(|record| {
                [
                    &record.inventory_id as &dyn ToSql,
                    &record.object_identity_sha256,
                    &record.blob_ordinal,
                    &record.extent_ordinal,
                    &record.blob_offset,
                    &record.device_id,
                    &record.physical_offset_hex,
                    &record.length,
                ]
            })))
        },
    )
}

pub(super) fn insert_shared_blobs(
    conn: &Connection,
    records: &[CephBluestoreSharedBlobRecord],
) -> DbResult<()> {
    insert_rows(
        conn,
        "INSERT INTO ceph_bluestore_shared_blobs (
            inventory_id, shared_blob_id_hex, denc_version, decode_status,
            deferred_reason, ref_extent_count, total_ref_bytes, total_refs
         ) VALUES ",
        8,
        records,
        |statement, records| {
            statement.execute(params_from_iter(records.iter().flat_map(|record| {
                [
                    &record.inventory_id as &dyn ToSql,
                    &record.shared_blob_id_hex,
                    &record.denc_version,
                    &record.decode_status,
                    &record.deferred_reason,
                    &record.ref_extent_count,
                    &record.total_ref_bytes,
                    &record.total_refs,
                ]
            })))
        },
    )
}

pub(super) fn insert_shared_blob_refs(
    conn: &Connection,
    records: &[CephBluestoreSharedBlobRefRecord],
) -> DbResult<()> {
    insert_rows(
        conn,
        "INSERT INTO ceph_bluestore_shared_blob_refs (
            inventory_id, shared_blob_id_hex, ref_ordinal, ref_offset_hex,
            length, refs
         ) VALUES ",
        6,
        records,
        |statement, records| {
            statement.execute(params_from_iter(records.iter().flat_map(|record| {
                [
                    &record.inventory_id as &dyn ToSql,
                    &record.shared_blob_id_hex,
                    &record.ref_ordinal,
                    &record.ref_offset_hex,
                    &record.length,
                    &record.refs,
                ]
            })))
        },
    )
}
