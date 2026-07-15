use std::collections::HashMap;

use rusqlite::{params, types::Type, Connection, OptionalExtension};

use crate::connection::{DbError, DbResult};

pub(super) use super::mapping::{
    map_blob, map_collection, map_logical_extent, map_object, map_onode_shard, map_physical_extent,
    map_scan, map_shared_blob, map_shared_blob_ref, map_super,
};
use super::{
    CephBluestoreBlobRecord, CephBluestoreChecksumChunkRecord, CephBluestoreCollectionRecord,
    CephBluestoreLogicalExtentRecord, CephBluestoreObjectRecord, CephBluestoreOnodeShardRecord,
    CephBluestorePhysicalExtentRecord, CephBluestoreSemanticAggregate,
    CephBluestoreSemanticScanRecord, CephBluestoreSharedBlobRecord,
    CephBluestoreSharedBlobRefRecord, CephBluestoreSuperRecord,
};

pub(super) fn find_aggregate(
    conn: &Connection,
    inventory_id: &str,
) -> DbResult<Option<CephBluestoreSemanticAggregate>> {
    let Some(scan) = find_scan(conn, inventory_id)? else {
        return Ok(None);
    };
    let objects = find_objects(conn, inventory_id)?;
    let checksum_chunks = find_checksum_chunks(conn, inventory_id, &objects)?;
    Ok(Some(CephBluestoreSemanticAggregate {
        super_record: find_super(conn, inventory_id)?,
        collections: find_collections(conn, inventory_id)?,
        objects,
        onode_shards: find_onode_shards(conn, inventory_id)?,
        blobs: find_blobs(conn, inventory_id)?,
        logical_extents: find_logical_extents(conn, inventory_id)?,
        physical_extents: find_physical_extents(conn, inventory_id)?,
        checksum_chunks,
        shared_blobs: find_shared_blobs(conn, inventory_id)?,
        shared_blob_refs: find_shared_blob_refs(conn, inventory_id)?,
        scan,
    }))
}

pub(super) fn find_scan(
    conn: &Connection,
    inventory_id: &str,
) -> DbResult<Option<CephBluestoreSemanticScanRecord>> {
    conn.query_row(
        "SELECT inventory_id, schema_version, decode_profile, sharding_sha256,
                latest_state_sha256, semantic_sha256,
                s_latest_count, s_decoded_count, s_deferred_count,
                c_latest_count, c_decoded_count, c_deferred_count,
                o_latest_count, o_decoded_count, o_deferred_count,
                x_latest_count, x_decoded_count, x_deferred_count,
                collection_count, object_count, blob_count, onode_shard_count,
                logical_extent_count, physical_extent_count, checksum_chunk_count,
                shared_blob_count, shared_ref_extent_count, profile_complete
         FROM ceph_bluestore_semantic_scans
         WHERE inventory_id = ?1",
        params![inventory_id],
        map_scan,
    )
    .optional()
    .map_err(Into::into)
}

pub(super) fn find_super(
    conn: &Connection,
    inventory_id: &str,
) -> DbResult<CephBluestoreSuperRecord> {
    conn.query_row(
        "SELECT inventory_id, nid_max, blobid_max, min_alloc_size, ondisk_format,
                min_compat_ondisk_format, per_pool_omap, freelist_type,
                observed_count, deferred_count
         FROM ceph_bluestore_super
         WHERE inventory_id = ?1",
        params![inventory_id],
        map_super,
    )
    .map_err(Into::into)
}

fn find_collections(
    conn: &Connection,
    inventory_id: &str,
) -> DbResult<Vec<CephBluestoreCollectionRecord>> {
    let mut statement = conn.prepare(
        "SELECT inventory_id, collection_identity, kind, pool, seed, shard, bits,
                denc_version, decode_status, deferred_reason
         FROM ceph_bluestore_collections
         WHERE inventory_id = ?1
         ORDER BY collection_identity",
    )?;
    let rows = statement.query_map(params![inventory_id], map_collection)?;
    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

fn find_objects(conn: &Connection, inventory_id: &str) -> DbResult<Vec<CephBluestoreObjectRecord>> {
    let mut statement = conn.prepare(
        "SELECT inventory_id, object_identity_sha256, decoded_shard, decoded_pool,
                decoded_hash, decoded_bitwise_hash, object_namespace, object_key,
                object_name, snap_hex, generation_hex, onode_denc_version, nid, size,
                flags_raw, flag_omap, flag_pgmeta_omap, flag_per_pool_omap,
                flag_per_pg_omap, flags_unknown_bits, attribute_count,
                attribute_value_bytes, attributes_sha256, expected_object_size,
                expected_write_size, allocation_hint_flags, zone_ref_count,
                extent_storage, spanning_blob_version, declared_spanning_blob_count,
                decode_status, deferred_reason, onode_shard_count, blob_count,
                logical_extent_count, physical_extent_count
         FROM ceph_bluestore_objects
         WHERE inventory_id = ?1
         ORDER BY object_identity_sha256",
    )?;
    let rows = statement.query_map(params![inventory_id], map_object)?;
    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

fn find_onode_shards(
    conn: &Connection,
    inventory_id: &str,
) -> DbResult<Vec<CephBluestoreOnodeShardRecord>> {
    let mut statement = conn.prepare(
        "SELECT inventory_id, object_identity_sha256, shard_ordinal, shard_offset,
                descriptor_bytes, payload_version, declared_extent_count,
                payload_encoded_length, decode_status, deferred_reason,
                logical_extent_count
         FROM ceph_bluestore_onode_shards
         WHERE inventory_id = ?1
         ORDER BY object_identity_sha256, shard_ordinal",
    )?;
    let rows = statement.query_map(params![inventory_id], map_onode_shard)?;
    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

fn find_blobs(conn: &Connection, inventory_id: &str) -> DbResult<Vec<CephBluestoreBlobRecord>> {
    let mut statement = conn.prepare(
        "SELECT inventory_id, object_identity_sha256, blob_ordinal, blob_kind,
                blob_id_hex, shared_blob_id_hex, logical_length, on_disk_length,
                compressed_length, flags_raw, flag_legacy_mutable, flag_compressed,
                flag_checksum, flag_has_unused, flag_shared, flags_unknown_bits,
                unused_bitmap, checksum_type, checksum_order, checksum_chunk_size,
                checksum_encoded_length, checksum_value_count,
                checksum_data_crc32c, checksum_digest_sha256, use_tracker_kind,
                use_tracker_allocation_unit_size,
                use_tracker_declared_allocation_units, use_tracker_entry_count,
                use_tracker_sha256, logical_extent_count, physical_extent_count
         FROM ceph_bluestore_blobs
         WHERE inventory_id = ?1
         ORDER BY object_identity_sha256, blob_ordinal",
    )?;
    let rows = statement.query_map(params![inventory_id], map_blob)?;
    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

fn find_logical_extents(
    conn: &Connection,
    inventory_id: &str,
) -> DbResult<Vec<CephBluestoreLogicalExtentRecord>> {
    let mut statement = conn.prepare(
        "SELECT inventory_id, object_identity_sha256, extent_ordinal,
                logical_offset, length, blob_ordinal, blob_offset, shard_ordinal,
                defines_blob, flags_raw, flag_contiguous, flag_zero_blob_offset,
                flag_same_length, flag_spanning
         FROM ceph_bluestore_logical_extents
         WHERE inventory_id = ?1
         ORDER BY object_identity_sha256, extent_ordinal",
    )?;
    let rows = statement.query_map(params![inventory_id], map_logical_extent)?;
    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

fn find_physical_extents(
    conn: &Connection,
    inventory_id: &str,
) -> DbResult<Vec<CephBluestorePhysicalExtentRecord>> {
    let mut statement = conn.prepare(
        "SELECT inventory_id, object_identity_sha256, blob_ordinal, extent_ordinal,
                blob_offset, device_id, physical_offset_hex, length
         FROM ceph_bluestore_physical_extents
         WHERE inventory_id = ?1
         ORDER BY object_identity_sha256, blob_ordinal, extent_ordinal",
    )?;
    let rows = statement.query_map(params![inventory_id], map_physical_extent)?;
    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

fn find_checksum_chunks(
    conn: &Connection,
    inventory_id: &str,
    objects: &[CephBluestoreObjectRecord],
) -> DbResult<Vec<CephBluestoreChecksumChunkRecord>> {
    let mut statement = conn.prepare(
        "SELECT object_identity_sha256, blob_ordinal,
                checksum_ordinal, chunk_offset, chunk_length, checksum_value_hex
         FROM ceph_bluestore_checksum_chunks
         WHERE inventory_id = ?1
         ORDER BY object_identity_sha256, blob_ordinal, checksum_ordinal",
    )?;
    let object_ordinals = objects
        .iter()
        .enumerate()
        .map(|(ordinal, record)| {
            u32::try_from(ordinal)
                .map(|ordinal| (record.object_identity_sha256.as_str(), ordinal))
                .map_err(|_| DbError::System("BlueStore object ordinal exceeds u32".to_string()))
        })
        .collect::<DbResult<HashMap<_, _>>>()?;
    let rows = statement.query_map(params![inventory_id], |row| {
        let object_id = row.get_ref(0)?.as_str()?;
        let (checksum_value, checksum_value_bytes) =
            parse_checksum_value(row.get_ref(5)?.as_str()?)?;
        let object_ordinal = object_ordinals
            .get(object_id)
            .copied()
            .ok_or_else(|| invalid_checksum_object_id(object_id))?;
        Ok(CephBluestoreChecksumChunkRecord {
            object_ordinal,
            blob_ordinal: row.get(1)?,
            checksum_ordinal: row.get(2)?,
            chunk_offset: row.get(3)?,
            chunk_length: row.get(4)?,
            checksum_value,
            checksum_value_bytes,
        })
    })?;
    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

fn invalid_checksum_object_id(object_id: &str) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(
        0,
        Type::Text,
        Box::new(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("BlueStore checksum references unknown object {object_id}"),
        )),
    )
}

pub(super) fn parse_checksum_value(value: &str) -> rusqlite::Result<(u64, u8)> {
    if value.is_empty()
        || value.len() > 16
        || !value.len().is_multiple_of(2)
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(invalid_checksum_value());
    }
    let parsed = u64::from_str_radix(value, 16).map_err(|_| invalid_checksum_value())?;
    Ok((parsed, (value.len() / 2) as u8))
}

fn invalid_checksum_value() -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(
        5,
        Type::Text,
        Box::new(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "BlueStore checksum value is not canonical fixed-width lowercase hex",
        )),
    )
}

fn find_shared_blobs(
    conn: &Connection,
    inventory_id: &str,
) -> DbResult<Vec<CephBluestoreSharedBlobRecord>> {
    let mut statement = conn.prepare(
        "SELECT inventory_id, shared_blob_id_hex, denc_version, decode_status,
                deferred_reason, ref_extent_count, total_ref_bytes, total_refs
         FROM ceph_bluestore_shared_blobs
         WHERE inventory_id = ?1
         ORDER BY shared_blob_id_hex",
    )?;
    let rows = statement.query_map(params![inventory_id], map_shared_blob)?;
    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

fn find_shared_blob_refs(
    conn: &Connection,
    inventory_id: &str,
) -> DbResult<Vec<CephBluestoreSharedBlobRefRecord>> {
    let mut statement = conn.prepare(
        "SELECT inventory_id, shared_blob_id_hex, ref_ordinal, ref_offset_hex,
                length, refs
         FROM ceph_bluestore_shared_blob_refs
         WHERE inventory_id = ?1
         ORDER BY shared_blob_id_hex, ref_ordinal",
    )?;
    let rows = statement.query_map(params![inventory_id], map_shared_blob_ref)?;
    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}
