use rusqlite::{params, Connection, OptionalExtension};

use crate::connection::DbResult;

use super::super::{
    mapping::{map_blob, map_logical_extent, map_object, map_onode_shard, map_physical_extent},
    CephBluestoreBlobRecord, CephBluestoreLogicalExtentRecord, CephBluestoreObjectRecord,
    CephBluestoreOnodeShardRecord, CephBluestorePhysicalExtentRecord,
    CephBluestoreSharedBlobRecord, CephBluestoreSharedBlobRefRecord,
};
use super::OBJECT_COLUMNS;

pub(super) fn find_object(
    conn: &Connection,
    inventory_id: &str,
    object_identity_sha256: &str,
) -> DbResult<Option<CephBluestoreObjectRecord>> {
    let sql = format!(
        "SELECT {OBJECT_COLUMNS}
         FROM ceph_bluestore_objects
         WHERE inventory_id = ?1 AND object_identity_sha256 = ?2"
    );
    let mut statement = conn.prepare_cached(&sql)?;
    statement
        .query_row(params![inventory_id, object_identity_sha256], map_object)
        .optional()
        .map_err(Into::into)
}

pub(super) fn find_onode_shards(
    conn: &Connection,
    inventory_id: &str,
    object_identity_sha256: &str,
) -> DbResult<Vec<CephBluestoreOnodeShardRecord>> {
    let mut statement = conn.prepare_cached(
        "SELECT inventory_id, object_identity_sha256, shard_ordinal, shard_offset,
                descriptor_bytes, payload_version, declared_extent_count,
                payload_encoded_length, decode_status, deferred_reason,
                logical_extent_count
         FROM ceph_bluestore_onode_shards
         WHERE inventory_id = ?1 AND object_identity_sha256 = ?2
         ORDER BY shard_ordinal",
    )?;
    let rows = statement.query_map(
        params![inventory_id, object_identity_sha256],
        map_onode_shard,
    )?;
    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

pub(super) fn find_blobs(
    conn: &Connection,
    inventory_id: &str,
    object_identity_sha256: &str,
) -> DbResult<Vec<CephBluestoreBlobRecord>> {
    let mut statement = conn.prepare_cached(
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
         WHERE inventory_id = ?1 AND object_identity_sha256 = ?2
         ORDER BY blob_ordinal",
    )?;
    let rows = statement.query_map(params![inventory_id, object_identity_sha256], map_blob)?;
    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

pub(super) fn find_logical_extents(
    conn: &Connection,
    inventory_id: &str,
    object_identity_sha256: &str,
) -> DbResult<Vec<CephBluestoreLogicalExtentRecord>> {
    let mut statement = conn.prepare_cached(
        "SELECT inventory_id, object_identity_sha256, extent_ordinal,
                logical_offset, length, blob_ordinal, blob_offset, shard_ordinal,
                defines_blob, flags_raw, flag_contiguous, flag_zero_blob_offset,
                flag_same_length, flag_spanning
         FROM ceph_bluestore_logical_extents
         WHERE inventory_id = ?1 AND object_identity_sha256 = ?2
         ORDER BY extent_ordinal",
    )?;
    let rows = statement.query_map(
        params![inventory_id, object_identity_sha256],
        map_logical_extent,
    )?;
    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

pub(super) fn find_physical_extents(
    conn: &Connection,
    inventory_id: &str,
    object_identity_sha256: &str,
) -> DbResult<Vec<CephBluestorePhysicalExtentRecord>> {
    let mut statement = conn.prepare_cached(
        "SELECT inventory_id, object_identity_sha256, blob_ordinal, extent_ordinal,
                blob_offset, device_id, physical_offset_hex, length
         FROM ceph_bluestore_physical_extents
         WHERE inventory_id = ?1 AND object_identity_sha256 = ?2
         ORDER BY blob_ordinal, extent_ordinal",
    )?;
    let rows = statement.query_map(
        params![inventory_id, object_identity_sha256],
        map_physical_extent,
    )?;
    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

pub(super) fn find_shared_blobs(
    conn: &Connection,
    inventory_id: &str,
    object_identity_sha256: &str,
) -> DbResult<Vec<CephBluestoreSharedBlobRecord>> {
    let mut statement = conn.prepare_cached(
        "SELECT inventory_id, shared_blob_id_hex, denc_version, decode_status,
                deferred_reason, ref_extent_count, total_ref_bytes, total_refs
         FROM ceph_bluestore_shared_blobs
         WHERE inventory_id = ?1
           AND shared_blob_id_hex IN (
               SELECT shared_blob_id_hex
               FROM ceph_bluestore_blobs
               WHERE inventory_id = ?1
                 AND object_identity_sha256 = ?2
                 AND shared_blob_id_hex IS NOT NULL
           )
         ORDER BY shared_blob_id_hex",
    )?;
    let rows = statement.query_map(params![inventory_id, object_identity_sha256], |row| {
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
    })?;
    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

pub(super) fn find_shared_blob_refs(
    conn: &Connection,
    inventory_id: &str,
    object_identity_sha256: &str,
) -> DbResult<Vec<CephBluestoreSharedBlobRefRecord>> {
    let mut statement = conn.prepare_cached(
        "SELECT inventory_id, shared_blob_id_hex, ref_ordinal, ref_offset_hex,
                length, refs
         FROM ceph_bluestore_shared_blob_refs
         WHERE inventory_id = ?1
           AND shared_blob_id_hex IN (
               SELECT shared_blob_id_hex
               FROM ceph_bluestore_blobs
               WHERE inventory_id = ?1
                 AND object_identity_sha256 = ?2
                 AND shared_blob_id_hex IS NOT NULL
           )
         ORDER BY shared_blob_id_hex, ref_ordinal",
    )?;
    let rows = statement.query_map(params![inventory_id, object_identity_sha256], |row| {
        Ok(CephBluestoreSharedBlobRefRecord {
            inventory_id: row.get(0)?,
            shared_blob_id_hex: row.get(1)?,
            ref_ordinal: row.get(2)?,
            ref_offset_hex: row.get(3)?,
            length: row.get(4)?,
            refs: row.get(5)?,
        })
    })?;
    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}
