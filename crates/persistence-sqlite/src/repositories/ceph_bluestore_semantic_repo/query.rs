use std::collections::HashMap;

use rusqlite::{params, types::Type, Connection, OptionalExtension};

use crate::connection::{DbError, DbResult};

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

fn find_scan(
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

fn find_super(conn: &Connection, inventory_id: &str) -> DbResult<CephBluestoreSuperRecord> {
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

fn parse_checksum_value(value: &str) -> rusqlite::Result<(u64, u8)> {
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

fn map_scan(row: &rusqlite::Row<'_>) -> rusqlite::Result<CephBluestoreSemanticScanRecord> {
    Ok(CephBluestoreSemanticScanRecord {
        inventory_id: row.get(0)?,
        schema_version: row.get(1)?,
        decode_profile: row.get(2)?,
        sharding_sha256: row.get(3)?,
        latest_state_sha256: row.get(4)?,
        semantic_sha256: row.get(5)?,
        s_latest_count: row.get(6)?,
        s_decoded_count: row.get(7)?,
        s_deferred_count: row.get(8)?,
        c_latest_count: row.get(9)?,
        c_decoded_count: row.get(10)?,
        c_deferred_count: row.get(11)?,
        o_latest_count: row.get(12)?,
        o_decoded_count: row.get(13)?,
        o_deferred_count: row.get(14)?,
        x_latest_count: row.get(15)?,
        x_decoded_count: row.get(16)?,
        x_deferred_count: row.get(17)?,
        collection_count: row.get(18)?,
        object_count: row.get(19)?,
        blob_count: row.get(20)?,
        onode_shard_count: row.get(21)?,
        logical_extent_count: row.get(22)?,
        physical_extent_count: row.get(23)?,
        checksum_chunk_count: row.get(24)?,
        shared_blob_count: row.get(25)?,
        shared_ref_extent_count: row.get(26)?,
        profile_complete: row.get(27)?,
    })
}

fn map_super(row: &rusqlite::Row<'_>) -> rusqlite::Result<CephBluestoreSuperRecord> {
    Ok(CephBluestoreSuperRecord {
        inventory_id: row.get(0)?,
        nid_max: row.get(1)?,
        blobid_max: row.get(2)?,
        min_alloc_size: row.get(3)?,
        ondisk_format: row.get(4)?,
        min_compat_ondisk_format: row.get(5)?,
        per_pool_omap: row.get(6)?,
        freelist_type: row.get(7)?,
        observed_count: row.get(8)?,
        deferred_count: row.get(9)?,
    })
}

fn map_collection(row: &rusqlite::Row<'_>) -> rusqlite::Result<CephBluestoreCollectionRecord> {
    Ok(CephBluestoreCollectionRecord {
        inventory_id: row.get(0)?,
        collection_identity: row.get(1)?,
        kind: row.get(2)?,
        pool: row.get(3)?,
        seed: row.get(4)?,
        shard: row.get(5)?,
        bits: row.get(6)?,
        denc_version: row.get(7)?,
        decode_status: row.get(8)?,
        deferred_reason: row.get(9)?,
    })
}

fn map_object(row: &rusqlite::Row<'_>) -> rusqlite::Result<CephBluestoreObjectRecord> {
    Ok(CephBluestoreObjectRecord {
        inventory_id: row.get(0)?,
        object_identity_sha256: row.get(1)?,
        decoded_shard: row.get(2)?,
        decoded_pool: row.get(3)?,
        decoded_hash: row.get(4)?,
        decoded_bitwise_hash: row.get(5)?,
        object_namespace: row.get(6)?,
        object_key: row.get(7)?,
        object_name: row.get(8)?,
        snap_hex: row.get(9)?,
        generation_hex: row.get(10)?,
        onode_denc_version: row.get(11)?,
        nid: row.get(12)?,
        size: row.get(13)?,
        flags_raw: row.get(14)?,
        flag_omap: row.get(15)?,
        flag_pgmeta_omap: row.get(16)?,
        flag_per_pool_omap: row.get(17)?,
        flag_per_pg_omap: row.get(18)?,
        flags_unknown_bits: row.get(19)?,
        attribute_count: row.get(20)?,
        attribute_value_bytes: row.get(21)?,
        attributes_sha256: row.get(22)?,
        expected_object_size: row.get(23)?,
        expected_write_size: row.get(24)?,
        allocation_hint_flags: row.get(25)?,
        zone_ref_count: row.get(26)?,
        extent_storage: row.get(27)?,
        spanning_blob_version: row.get(28)?,
        declared_spanning_blob_count: row.get(29)?,
        decode_status: row.get(30)?,
        deferred_reason: row.get(31)?,
        onode_shard_count: row.get(32)?,
        blob_count: row.get(33)?,
        logical_extent_count: row.get(34)?,
        physical_extent_count: row.get(35)?,
    })
}

fn map_onode_shard(row: &rusqlite::Row<'_>) -> rusqlite::Result<CephBluestoreOnodeShardRecord> {
    Ok(CephBluestoreOnodeShardRecord {
        inventory_id: row.get(0)?,
        object_identity_sha256: row.get(1)?,
        shard_ordinal: row.get(2)?,
        shard_offset: row.get(3)?,
        descriptor_bytes: row.get(4)?,
        payload_version: row.get(5)?,
        declared_extent_count: row.get(6)?,
        payload_encoded_length: row.get(7)?,
        decode_status: row.get(8)?,
        deferred_reason: row.get(9)?,
        logical_extent_count: row.get(10)?,
    })
}

fn map_blob(row: &rusqlite::Row<'_>) -> rusqlite::Result<CephBluestoreBlobRecord> {
    Ok(CephBluestoreBlobRecord {
        inventory_id: row.get(0)?,
        object_identity_sha256: row.get(1)?,
        blob_ordinal: row.get(2)?,
        blob_kind: row.get(3)?,
        blob_id_hex: row.get(4)?,
        shared_blob_id_hex: row.get(5)?,
        logical_length: row.get(6)?,
        on_disk_length: row.get(7)?,
        compressed_length: row.get(8)?,
        flags_raw: row.get(9)?,
        flag_legacy_mutable: row.get(10)?,
        flag_compressed: row.get(11)?,
        flag_checksum: row.get(12)?,
        flag_has_unused: row.get(13)?,
        flag_shared: row.get(14)?,
        flags_unknown_bits: row.get(15)?,
        unused_bitmap: row.get(16)?,
        checksum_type: row.get(17)?,
        checksum_order: row.get(18)?,
        checksum_chunk_size: row.get(19)?,
        checksum_encoded_length: row.get(20)?,
        checksum_value_count: row.get(21)?,
        checksum_data_crc32c: row.get(22)?,
        checksum_digest_sha256: row.get(23)?,
        use_tracker_kind: row.get(24)?,
        use_tracker_allocation_unit_size: row.get(25)?,
        use_tracker_declared_allocation_units: row.get(26)?,
        use_tracker_entry_count: row.get(27)?,
        use_tracker_sha256: row.get(28)?,
        logical_extent_count: row.get(29)?,
        physical_extent_count: row.get(30)?,
    })
}

fn map_logical_extent(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<CephBluestoreLogicalExtentRecord> {
    Ok(CephBluestoreLogicalExtentRecord {
        inventory_id: row.get(0)?,
        object_identity_sha256: row.get(1)?,
        extent_ordinal: row.get(2)?,
        logical_offset: row.get(3)?,
        length: row.get(4)?,
        blob_ordinal: row.get(5)?,
        blob_offset: row.get(6)?,
        shard_ordinal: row.get(7)?,
        defines_blob: row.get(8)?,
        flags_raw: row.get(9)?,
        flag_contiguous: row.get(10)?,
        flag_zero_blob_offset: row.get(11)?,
        flag_same_length: row.get(12)?,
        flag_spanning: row.get(13)?,
    })
}

fn map_physical_extent(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<CephBluestorePhysicalExtentRecord> {
    Ok(CephBluestorePhysicalExtentRecord {
        inventory_id: row.get(0)?,
        object_identity_sha256: row.get(1)?,
        blob_ordinal: row.get(2)?,
        extent_ordinal: row.get(3)?,
        blob_offset: row.get(4)?,
        device_id: row.get(5)?,
        physical_offset_hex: row.get(6)?,
        length: row.get(7)?,
    })
}

fn map_shared_blob(row: &rusqlite::Row<'_>) -> rusqlite::Result<CephBluestoreSharedBlobRecord> {
    Ok(CephBluestoreSharedBlobRecord {
        inventory_id: row.get(0)?,
        shared_blob_id_hex: row.get(1)?,
        denc_version: row.get(2)?,
        decode_status: row.get(3)?,
        deferred_reason: row.get(4)?,
        ref_extent_count: row.get(5)?,
        total_ref_bytes: row.get(6)?,
        total_refs: row.get(7)?,
    })
}

fn map_shared_blob_ref(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<CephBluestoreSharedBlobRefRecord> {
    Ok(CephBluestoreSharedBlobRefRecord {
        inventory_id: row.get(0)?,
        shared_blob_id_hex: row.get(1)?,
        ref_ordinal: row.get(2)?,
        ref_offset_hex: row.get(3)?,
        length: row.get(4)?,
        refs: row.get(5)?,
    })
}
