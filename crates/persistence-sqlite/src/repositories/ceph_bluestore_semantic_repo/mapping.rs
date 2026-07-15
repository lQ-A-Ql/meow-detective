use super::{
    CephBluestoreBlobRecord, CephBluestoreCollectionRecord, CephBluestoreLogicalExtentRecord,
    CephBluestoreObjectRecord, CephBluestoreOnodeShardRecord, CephBluestorePhysicalExtentRecord,
    CephBluestoreSemanticScanRecord, CephBluestoreSharedBlobRecord,
    CephBluestoreSharedBlobRefRecord, CephBluestoreSuperRecord,
};

pub(super) fn map_scan(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<CephBluestoreSemanticScanRecord> {
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

pub(super) fn map_super(row: &rusqlite::Row<'_>) -> rusqlite::Result<CephBluestoreSuperRecord> {
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

pub(super) fn map_collection(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<CephBluestoreCollectionRecord> {
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

pub(super) fn map_object(row: &rusqlite::Row<'_>) -> rusqlite::Result<CephBluestoreObjectRecord> {
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

pub(super) fn map_onode_shard(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<CephBluestoreOnodeShardRecord> {
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

pub(super) fn map_blob(row: &rusqlite::Row<'_>) -> rusqlite::Result<CephBluestoreBlobRecord> {
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

pub(super) fn map_logical_extent(
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

pub(super) fn map_physical_extent(
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

pub(super) fn map_shared_blob(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<CephBluestoreSharedBlobRecord> {
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

pub(super) fn map_shared_blob_ref(
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
