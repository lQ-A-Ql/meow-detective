mod children;
mod digest;
mod identity;
mod primitives;

use crate::connection::{DbError, DbResult};
use crate::repositories::{
    ceph_rocksdb_latest_state_repo::CephRocksdbLatestStateRecord,
    ceph_rocksdb_repo::CephRocksdbAggregate,
};

use super::{
    CephBluestoreCollectionRecord, CephBluestoreObjectRecord, CephBluestoreSemanticAggregate,
    CephBluestoreSharedBlobRecord, BLUESTORE_SEMANTIC_DECODE_PROFILE,
    BLUESTORE_SEMANTIC_SCHEMA_VERSION,
};
use primitives::{
    checked_len, fits_sqlite, semantic_error, valid_hex_u64, valid_optional_text, valid_sha256,
    valid_status, valid_text,
};

pub use digest::{latest_state_set_sha256, semantic_aggregate_sha256};
pub use identity::{canonical_collection_identity, object_identity_sha256};

pub(super) fn validate_replacement(aggregate: &CephBluestoreSemanticAggregate) -> DbResult<()> {
    validate_scan(aggregate)?;
    validate_super(aggregate)?;
    validate_collections(aggregate)?;
    validate_objects(aggregate)?;
    validate_shared_blobs(aggregate)?;
    children::validate_children(aggregate)?;
    validate_scan_counts(aggregate)?;
    if semantic_aggregate_sha256(aggregate) != aggregate.scan.semantic_sha256 {
        return semantic_error("BlueStore semantic aggregate digest does not match its rows");
    }
    Ok(())
}

pub(crate) fn validate_recovery_binding(
    rocksdb: &CephRocksdbAggregate,
    latest_state: &[CephRocksdbLatestStateRecord],
    aggregate: &CephBluestoreSemanticAggregate,
) -> DbResult<()> {
    let sharding_sha256 = latest_state
        .first()
        .map(|record| record.sharding_sha256.as_str())
        .unwrap_or_default();
    if aggregate.scan.inventory_id != rocksdb.manifest.inventory_id
        || aggregate.scan.sharding_sha256 != sharding_sha256
        || aggregate.scan.latest_state_sha256 != latest_state_set_sha256(latest_state)
    {
        return Err(DbError::System(
            "BlueStore semantic snapshot does not match its RocksDB recovery".to_string(),
        ));
    }
    Ok(())
}

pub(crate) fn validate_device_bounds(
    aggregate: &CephBluestoreSemanticAggregate,
    device_size: u64,
) -> DbResult<()> {
    if aggregate.physical_extents.iter().any(|extent| {
        extent.physical_offset_hex.as_deref().is_some_and(|value| {
            u64::from_str_radix(value, 16)
                .ok()
                .and_then(|offset| offset.checked_add(extent.length))
                .is_none_or(|end| end > device_size)
        })
    }) {
        return Err(DbError::System(
            "BlueStore semantic physical extent exceeds its OSD device".to_string(),
        ));
    }
    Ok(())
}

fn validate_scan(aggregate: &CephBluestoreSemanticAggregate) -> DbResult<()> {
    let scan = &aggregate.scan;
    let counts = [
        scan.s_latest_count,
        scan.s_decoded_count,
        scan.s_deferred_count,
        scan.c_latest_count,
        scan.c_decoded_count,
        scan.c_deferred_count,
        scan.o_latest_count,
        scan.o_decoded_count,
        scan.o_deferred_count,
        scan.x_latest_count,
        scan.x_decoded_count,
        scan.x_deferred_count,
        scan.collection_count,
        scan.object_count,
        scan.blob_count,
        scan.onode_shard_count,
        scan.logical_extent_count,
        scan.physical_extent_count,
        scan.checksum_chunk_count,
        scan.shared_blob_count,
        scan.shared_ref_extent_count,
    ];
    if !valid_text(&scan.inventory_id)
        || scan.schema_version != BLUESTORE_SEMANTIC_SCHEMA_VERSION
        || scan.decode_profile != BLUESTORE_SEMANTIC_DECODE_PROFILE
        || !valid_sha256(&scan.sharding_sha256)
        || !valid_sha256(&scan.latest_state_sha256)
        || !valid_sha256(&scan.semantic_sha256)
        || !scan.profile_complete
        || counts.into_iter().any(|count| !fits_sqlite(count))
    {
        return semantic_error("BlueStore semantic scan is incomplete or invalid");
    }
    Ok(())
}

fn validate_super(aggregate: &CephBluestoreSemanticAggregate) -> DbResult<()> {
    let record = &aggregate.super_record;
    let present = [
        record.nid_max.is_some(),
        record.blobid_max.is_some(),
        record.min_alloc_size.is_some(),
        record.ondisk_format.is_some(),
        record.min_compat_ondisk_format.is_some(),
        record.per_pool_omap.is_some(),
        record.freelist_type.is_some(),
    ]
    .into_iter()
    .filter(|present| *present)
    .count();
    let valid_limits = [record.nid_max, record.blobid_max, record.min_alloc_size]
        .into_iter()
        .flatten()
        .all(fits_sqlite);
    if record.inventory_id != aggregate.scan.inventory_id
        || !valid_limits
        || record.min_alloc_size == Some(0)
        || record.deferred_count > record.observed_count
        || checked_len(present)? + record.deferred_count != record.observed_count
        || !fits_sqlite(record.observed_count)
        || !fits_sqlite(record.deferred_count)
        || !matches!(
            record.per_pool_omap.as_deref(),
            None | Some("bulk" | "perPool" | "perPg")
        )
        || !valid_optional_text(record.freelist_type.as_deref())
        || matches!(
            (record.min_compat_ondisk_format, record.ondisk_format),
            (Some(minimum), Some(current)) if minimum > current
        )
    {
        return semantic_error("BlueStore super summary is inconsistent");
    }
    Ok(())
}

fn validate_collections(aggregate: &CephBluestoreSemanticAggregate) -> DbResult<()> {
    if !aggregate
        .collections
        .windows(2)
        .all(|rows| rows[0].collection_identity < rows[1].collection_identity)
    {
        return semantic_error("BlueStore collections are not in canonical identity order");
    }
    for record in &aggregate.collections {
        validate_collection(&aggregate.scan.inventory_id, record)?;
    }
    Ok(())
}

fn validate_collection(inventory_id: &str, record: &CephBluestoreCollectionRecord) -> DbResult<()> {
    let identity =
        canonical_collection_identity(&record.kind, record.pool, record.seed, record.shard);
    let parsed = record.decode_status == "parsed";
    if record.inventory_id != inventory_id
        || identity.as_deref() != Some(record.collection_identity.as_str())
        || !valid_status(&record.decode_status, record.deferred_reason.as_deref())
        || record.pool.is_some_and(|pool| !fits_sqlite(pool))
        || (parsed && (record.bits.is_none() || record.denc_version != Some(1)))
        || (!parsed && record.bits.is_some())
    {
        return semantic_error("BlueStore collection row is inconsistent");
    }
    Ok(())
}

fn validate_objects(aggregate: &CephBluestoreSemanticAggregate) -> DbResult<()> {
    if !aggregate
        .objects
        .windows(2)
        .all(|rows| rows[0].object_identity_sha256 < rows[1].object_identity_sha256)
    {
        return semantic_error("BlueStore objects are not in object identity order");
    }
    for record in &aggregate.objects {
        validate_object(&aggregate.scan.inventory_id, record)?;
    }
    Ok(())
}

fn validate_object(inventory_id: &str, record: &CephBluestoreObjectRecord) -> DbResult<()> {
    let counts = [
        record.nid,
        record.size,
        record.attribute_count,
        record.attribute_value_bytes,
        record.expected_object_size,
        record.expected_write_size,
        record.zone_ref_count,
        record.declared_spanning_blob_count,
        record.onode_shard_count,
        record.blob_count,
        record.logical_extent_count,
        record.physical_extent_count,
    ];
    if record.inventory_id != inventory_id
        || record.decoded_shard < -1
        || object_identity_sha256(record) != record.object_identity_sha256
        || !valid_hex_u64(&record.snap_hex)
        || !valid_hex_u64(&record.generation_hex)
        || !valid_sha256(&record.attributes_sha256)
        || counts.into_iter().any(|count| !fits_sqlite(count))
        || !matches!(record.onode_denc_version, 1 | 2)
        || !matches!(record.spanning_blob_version, 1 | 2)
        || !valid_status(&record.decode_status, record.deferred_reason.as_deref())
        || !valid_object_flags(record)
        || !valid_object_storage(record)
    {
        return semantic_error("BlueStore object row is inconsistent");
    }
    Ok(())
}

fn valid_object_flags(record: &CephBluestoreObjectRecord) -> bool {
    record.flag_omap == (record.flags_raw & 1 != 0)
        && record.flag_pgmeta_omap == (record.flags_raw & 2 != 0)
        && record.flag_per_pool_omap == (record.flags_raw & 4 != 0)
        && record.flag_per_pg_omap == (record.flags_raw & 8 != 0)
        && record.flags_unknown_bits == record.flags_raw & !0x0f
}

fn valid_object_storage(record: &CephBluestoreObjectRecord) -> bool {
    match record.extent_storage.as_str() {
        "inline" => record.onode_shard_count == 0 && record.decode_status == "parsed",
        "sharded" => record.onode_shard_count > 0 && record.decode_status == "parsed",
        "deferred" => {
            record.onode_shard_count == 0
                && record.blob_count == 0
                && record.logical_extent_count == 0
                && record.physical_extent_count == 0
                && record.decode_status == "deferred"
        }
        _ => false,
    }
}

fn validate_shared_blobs(aggregate: &CephBluestoreSemanticAggregate) -> DbResult<()> {
    if !aggregate
        .shared_blobs
        .windows(2)
        .all(|rows| rows[0].shared_blob_id_hex < rows[1].shared_blob_id_hex)
    {
        return semantic_error("BlueStore shared blobs are not in identifier order");
    }
    for record in &aggregate.shared_blobs {
        validate_shared_blob(&aggregate.scan.inventory_id, record)?;
    }
    Ok(())
}

fn validate_shared_blob(
    inventory_id: &str,
    record: &CephBluestoreSharedBlobRecord,
) -> DbResult<()> {
    let parsed = record.decode_status == "parsed";
    if record.inventory_id != inventory_id
        || !valid_hex_u64(&record.shared_blob_id_hex)
        || !valid_status(&record.decode_status, record.deferred_reason.as_deref())
        || (parsed && record.denc_version != Some(1))
        || (!parsed && record.denc_version.is_some())
        || [
            record.ref_extent_count,
            record.total_ref_bytes,
            record.total_refs,
        ]
        .into_iter()
        .any(|count| !fits_sqlite(count))
    {
        return semantic_error("BlueStore shared blob row is inconsistent");
    }
    Ok(())
}

fn validate_scan_counts(aggregate: &CephBluestoreSemanticAggregate) -> DbResult<()> {
    let scan = &aggregate.scan;
    let c_decoded = status_count_collections(&aggregate.collections, "parsed")?;
    let o_decoded = status_count_objects(aggregate, "parsed")?;
    let x_decoded = status_count_shared(&aggregate.shared_blobs, "parsed")?;
    let counts_match = scan.s_latest_count == aggregate.super_record.observed_count
        && scan.s_decoded_count
            == aggregate.super_record.observed_count - aggregate.super_record.deferred_count
        && scan.s_deferred_count == aggregate.super_record.deferred_count
        && scan.c_latest_count == checked_len(aggregate.collections.len())?
        && scan.c_decoded_count == c_decoded
        && scan.c_deferred_count == scan.c_latest_count - c_decoded
        && scan.o_latest_count
            == checked_len(aggregate.objects.len())? + checked_len(aggregate.onode_shards.len())?
        && scan.o_decoded_count == o_decoded
        && scan.o_deferred_count == scan.o_latest_count - o_decoded
        && scan.x_latest_count == checked_len(aggregate.shared_blobs.len())?
        && scan.x_decoded_count == x_decoded
        && scan.x_deferred_count == scan.x_latest_count - x_decoded;
    if !counts_match || !entity_counts_match(aggregate)? {
        return semantic_error("BlueStore semantic scan counts do not close");
    }
    Ok(())
}

fn entity_counts_match(aggregate: &CephBluestoreSemanticAggregate) -> DbResult<bool> {
    let scan = &aggregate.scan;
    Ok(
        scan.collection_count == checked_len(aggregate.collections.len())?
            && scan.object_count == checked_len(aggregate.objects.len())?
            && scan.blob_count == checked_len(aggregate.blobs.len())?
            && scan.onode_shard_count == checked_len(aggregate.onode_shards.len())?
            && scan.logical_extent_count == checked_len(aggregate.logical_extents.len())?
            && scan.physical_extent_count == checked_len(aggregate.physical_extents.len())?
            && scan.checksum_chunk_count == checked_len(aggregate.checksum_chunks.len())?
            && scan.shared_blob_count == checked_len(aggregate.shared_blobs.len())?
            && scan.shared_ref_extent_count == checked_len(aggregate.shared_blob_refs.len())?,
    )
}

fn status_count_collections(
    records: &[CephBluestoreCollectionRecord],
    status: &str,
) -> DbResult<u64> {
    checked_len(
        records
            .iter()
            .filter(|record| record.decode_status == status)
            .count(),
    )
}

fn status_count_shared(records: &[CephBluestoreSharedBlobRecord], status: &str) -> DbResult<u64> {
    checked_len(
        records
            .iter()
            .filter(|record| record.decode_status == status)
            .count(),
    )
}

fn status_count_objects(aggregate: &CephBluestoreSemanticAggregate, status: &str) -> DbResult<u64> {
    let objects = aggregate
        .objects
        .iter()
        .filter(|record| record.decode_status == status)
        .count();
    let shards = aggregate
        .onode_shards
        .iter()
        .filter(|record| record.decode_status == status)
        .count();
    checked_len(objects + shards)
}
