use std::collections::HashMap;

use crate::connection::{DbError, DbResult};

use super::{
    super::{
        super::{
            CephBluestoreBlobRecord, CephBluestoreOnodeShardRecord, CephBluestoreSemanticAggregate,
            CephBluestoreSharedBlobRecord,
        },
        primitives::semantic_error,
    },
    BlobKey, LogicalCounts, ShardKey,
};

pub(super) fn validate_counts(
    aggregate: &CephBluestoreSemanticAggregate,
    shards: &HashMap<&str, u64>,
    logical: &LogicalCounts<'_>,
    physical: &HashMap<BlobKey<'_>, u64>,
    checksums: &HashMap<BlobKey<'_>, u64>,
    shared_refs: &HashMap<&str, (u64, u64, u64)>,
) -> DbResult<()> {
    validate_object_counts(aggregate, shards, logical, physical)?;
    validate_blob_counts(&aggregate.blobs, &logical.blobs, physical, checksums)?;
    validate_shard_counts(&aggregate.onode_shards, &logical.shards)?;
    validate_shared_counts(&aggregate.shared_blobs, shared_refs)
}

fn validate_object_counts(
    aggregate: &CephBluestoreSemanticAggregate,
    shards: &HashMap<&str, u64>,
    logical: &LogicalCounts<'_>,
    physical: &HashMap<BlobKey<'_>, u64>,
) -> DbResult<()> {
    let mut child_counts = HashMap::<&str, (u64, u64, u64)>::new();
    for blob in &aggregate.blobs {
        let counts = child_counts
            .entry(blob.object_identity_sha256.as_str())
            .or_default();
        counts.0 = counts
            .0
            .checked_add(1)
            .ok_or_else(|| DbError::System("BlueStore object blob count overflow".to_string()))?;
        if blob.blob_kind == "spanning" {
            counts.1 = counts.1.checked_add(1).ok_or_else(|| {
                DbError::System("BlueStore object spanning blob count overflow".to_string())
            })?;
        }
    }
    for ((object_id, _), count) in physical {
        let counts = child_counts.entry(object_id).or_default();
        counts.2 = counts.2.checked_add(*count).ok_or_else(|| {
            DbError::System("BlueStore object physical count overflow".to_string())
        })?;
    }
    for object in &aggregate.objects {
        let id = object.object_identity_sha256.as_str();
        let (blob_count, spanning_blob_count, physical_count) =
            child_counts.get(id).copied().unwrap_or_default();
        if object.onode_shard_count != shards.get(id).copied().unwrap_or(0)
            || object.blob_count != blob_count
            || object.declared_spanning_blob_count != spanning_blob_count
            || object.logical_extent_count != logical.objects.get(id).copied().unwrap_or(0)
            || object.physical_extent_count != physical_count
        {
            return semantic_error("BlueStore object child counts do not close");
        }
    }
    Ok(())
}

fn validate_blob_counts(
    blobs: &[CephBluestoreBlobRecord],
    logical: &HashMap<BlobKey<'_>, u64>,
    physical: &HashMap<BlobKey<'_>, u64>,
    checksums: &HashMap<BlobKey<'_>, u64>,
) -> DbResult<()> {
    for blob in blobs {
        let key = (blob.object_identity_sha256.as_str(), blob.blob_ordinal);
        if blob.logical_extent_count != logical.get(&key).copied().unwrap_or(0)
            || blob.physical_extent_count != physical.get(&key).copied().unwrap_or(0)
            || blob.checksum_value_count != checksums.get(&key).copied().unwrap_or(0)
        {
            return semantic_error("BlueStore blob child counts do not close");
        }
    }
    Ok(())
}

fn validate_shard_counts(
    shards: &[CephBluestoreOnodeShardRecord],
    logical: &HashMap<ShardKey<'_>, u64>,
) -> DbResult<()> {
    for shard in shards {
        let key = (shard.object_identity_sha256.as_str(), shard.shard_ordinal);
        let actual = logical.get(&key).copied().unwrap_or(0);
        if shard.logical_extent_count != actual
            || shard
                .declared_extent_count
                .is_some_and(|declared| declared != actual)
        {
            return semantic_error("BlueStore onode shard extent counts do not close");
        }
    }
    Ok(())
}

fn validate_shared_counts(
    shared: &[CephBluestoreSharedBlobRecord],
    totals: &HashMap<&str, (u64, u64, u64)>,
) -> DbResult<()> {
    for blob in shared {
        let actual = totals
            .get(blob.shared_blob_id_hex.as_str())
            .copied()
            .unwrap_or((0, 0, 0));
        if (blob.ref_extent_count, blob.total_ref_bytes, blob.total_refs) != actual {
            return semantic_error("BlueStore shared blob ref counts do not close");
        }
    }
    Ok(())
}
