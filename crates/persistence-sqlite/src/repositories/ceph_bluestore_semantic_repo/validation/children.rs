mod blob;
mod checksum;
mod closure;
mod physical;

use std::collections::{HashMap, HashSet};

use crate::connection::{DbError, DbResult};

use super::{
    super::{
        CephBluestoreBlobRecord, CephBluestoreLogicalExtentRecord, CephBluestoreObjectRecord,
        CephBluestoreOnodeShardRecord, CephBluestoreSemanticAggregate,
        CephBluestoreSharedBlobRecord, CephBluestoreSharedBlobRefRecord,
    },
    primitives::{fits_sqlite, parse_hex_u64, semantic_error, valid_hex_u64, valid_status},
};

pub(super) type BlobKey<'a> = (&'a str, u32);
pub(super) type ShardKey<'a> = (&'a str, u32);

pub(super) fn validate_children(aggregate: &CephBluestoreSemanticAggregate) -> DbResult<()> {
    let inventory_id = aggregate.scan.inventory_id.as_str();
    let objects = index_objects(&aggregate.objects);
    let shared = index_shared_blobs(&aggregate.shared_blobs);
    let shard_counts = validate_shards(inventory_id, &objects, &aggregate.onode_shards)?;
    let blobs = blob::validate_blobs(inventory_id, &objects, &shared, &aggregate.blobs)?;
    let checksums = checksum::validate_checksum_chunks(
        &aggregate.objects,
        &aggregate.blobs,
        &aggregate.checksum_chunks,
    )?;
    let logical = validate_logical_extents(
        inventory_id,
        &objects,
        &blobs,
        &aggregate.onode_shards,
        &aggregate.logical_extents,
    )?;
    let shared_refs = validate_shared_refs(inventory_id, &shared, &aggregate.shared_blob_refs)?;
    let physical = physical::validate_physical_extents(
        inventory_id,
        &blobs,
        &aggregate.physical_extents,
        &aggregate.shared_blob_refs,
    )?;
    closure::validate_counts(
        aggregate,
        &shard_counts,
        &logical,
        &physical,
        &checksums,
        &shared_refs,
    )
}

fn index_objects(
    records: &[CephBluestoreObjectRecord],
) -> HashMap<&str, &CephBluestoreObjectRecord> {
    records
        .iter()
        .map(|record| (record.object_identity_sha256.as_str(), record))
        .collect()
}

fn index_shared_blobs(
    records: &[CephBluestoreSharedBlobRecord],
) -> HashMap<&str, &CephBluestoreSharedBlobRecord> {
    records
        .iter()
        .map(|record| (record.shared_blob_id_hex.as_str(), record))
        .collect()
}

fn validate_shards<'a>(
    inventory_id: &str,
    objects: &HashMap<&str, &CephBluestoreObjectRecord>,
    records: &'a [CephBluestoreOnodeShardRecord],
) -> DbResult<HashMap<&'a str, u64>> {
    if !records.windows(2).all(|rows| {
        (
            rows[0].object_identity_sha256.as_str(),
            rows[0].shard_ordinal,
        ) < (
            rows[1].object_identity_sha256.as_str(),
            rows[1].shard_ordinal,
        )
    }) {
        return semantic_error("BlueStore onode shards are not in canonical order");
    }
    let mut next = HashMap::new();
    let mut previous_offset = HashMap::new();
    let mut counts = HashMap::new();
    for record in records {
        let object = objects.get(record.object_identity_sha256.as_str());
        if record.inventory_id != inventory_id
            || object.is_none()
            || !valid_shard(record)
            || !take_ordinal(
                &mut next,
                record.object_identity_sha256.as_str(),
                record.shard_ordinal,
            )
            || previous_offset
                .insert(record.object_identity_sha256.as_str(), record.shard_offset)
                .is_some_and(|offset| offset >= record.shard_offset)
        {
            return semantic_error("BlueStore onode shard row is inconsistent");
        }
        increment(&mut counts, record.object_identity_sha256.as_str())?;
    }
    Ok(counts)
}

fn valid_shard(record: &CephBluestoreOnodeShardRecord) -> bool {
    let parsed = record.decode_status == "parsed";
    record.descriptor_bytes > 0
        && valid_status(&record.decode_status, record.deferred_reason.as_deref())
        && record.declared_extent_count.is_none_or(fits_sqlite)
        && record.payload_encoded_length.is_none_or(fits_sqlite)
        && fits_sqlite(record.logical_extent_count)
        && if parsed {
            matches!(record.payload_version, Some(1 | 2))
                && record.declared_extent_count.is_some()
                && record
                    .payload_encoded_length
                    .is_some_and(|length| length <= u64::from(record.descriptor_bytes))
        } else {
            record.logical_extent_count == 0
        }
}

pub(super) struct LogicalCounts<'a> {
    pub(super) objects: HashMap<&'a str, u64>,
    pub(super) blobs: HashMap<BlobKey<'a>, u64>,
    pub(super) shards: HashMap<ShardKey<'a>, u64>,
}

struct LogicalExtentProgress<'a> {
    next: HashMap<&'a str, u32>,
    logical_end: HashMap<&'a str, u64>,
    defined_local: HashSet<BlobKey<'a>>,
}

fn validate_logical_extents<'a>(
    inventory_id: &str,
    objects: &HashMap<&str, &CephBluestoreObjectRecord>,
    blobs: &HashMap<BlobKey<'a>, &'a CephBluestoreBlobRecord>,
    shards: &'a [CephBluestoreOnodeShardRecord],
    records: &'a [CephBluestoreLogicalExtentRecord],
) -> DbResult<LogicalCounts<'a>> {
    ensure_logical_order(records)?;
    let shard_keys = shards
        .iter()
        .map(|record| (record.object_identity_sha256.as_str(), record.shard_ordinal))
        .collect::<HashSet<_>>();
    let mut counts = LogicalCounts {
        objects: HashMap::new(),
        blobs: HashMap::new(),
        shards: HashMap::new(),
    };
    let mut progress = LogicalExtentProgress {
        next: HashMap::new(),
        logical_end: HashMap::new(),
        defined_local: HashSet::new(),
    };
    for record in records {
        validate_logical_extent(
            inventory_id,
            objects,
            blobs,
            &shard_keys,
            record,
            &mut progress,
        )?;
        increment(&mut counts.objects, record.object_identity_sha256.as_str())?;
        increment(
            &mut counts.blobs,
            (record.object_identity_sha256.as_str(), record.blob_ordinal),
        )?;
        if let Some(shard_ordinal) = record.shard_ordinal {
            increment(
                &mut counts.shards,
                (record.object_identity_sha256.as_str(), shard_ordinal),
            )?;
        }
    }
    Ok(counts)
}

fn ensure_logical_order(records: &[CephBluestoreLogicalExtentRecord]) -> DbResult<()> {
    if records.windows(2).all(|rows| {
        (
            rows[0].object_identity_sha256.as_str(),
            rows[0].extent_ordinal,
        ) < (
            rows[1].object_identity_sha256.as_str(),
            rows[1].extent_ordinal,
        )
    }) {
        Ok(())
    } else {
        semantic_error("BlueStore logical extents are not in canonical order")
    }
}

fn validate_logical_extent<'a>(
    inventory_id: &str,
    objects: &HashMap<&str, &CephBluestoreObjectRecord>,
    blobs: &HashMap<BlobKey<'a>, &'a CephBluestoreBlobRecord>,
    shards: &HashSet<ShardKey<'a>>,
    record: &'a CephBluestoreLogicalExtentRecord,
    progress: &mut LogicalExtentProgress<'a>,
) -> DbResult<()> {
    let object_id = record.object_identity_sha256.as_str();
    let Some(object) = objects.get(object_id) else {
        return semantic_error("BlueStore logical extent references an unknown object");
    };
    let Some(blob) = blobs.get(&(object_id, record.blob_ordinal)) else {
        return semantic_error("BlueStore logical extent references an unknown blob");
    };
    let end = record.logical_offset.checked_add(record.length);
    let blob_end = record.blob_offset.checked_add(record.length);
    let shard_valid = match (object.extent_storage.as_str(), record.shard_ordinal) {
        ("inline", None) => true,
        ("sharded", Some(ordinal)) => shards.contains(&(object_id, ordinal)),
        _ => false,
    };
    let blob_key = (object_id, record.blob_ordinal);
    let definition_valid = match blob.blob_kind.as_str() {
        "spanning" => record.flag_spanning && !record.defines_blob,
        "local" if record.defines_blob => {
            !record.flag_spanning && progress.defined_local.insert(blob_key)
        }
        "local" => !record.flag_spanning && progress.defined_local.contains(&blob_key),
        _ => false,
    };
    if record.inventory_id != inventory_id
        || record.length == 0
        || ![record.logical_offset, record.length, record.blob_offset]
            .into_iter()
            .all(fits_sqlite)
        || end.is_none_or(|end| end > object.size)
        || blob_end.is_none_or(|end| end > blob.logical_length)
        || !shard_valid
        || !definition_valid
        || record.flag_zero_blob_offset != (record.blob_offset == 0)
        || !valid_logical_flags(record)
        || !take_ordinal(&mut progress.next, object_id, record.extent_ordinal)
        || progress
            .logical_end
            .insert(object_id, end.unwrap_or_default())
            .is_some_and(|previous| previous > record.logical_offset)
    {
        return semantic_error("BlueStore logical extent range is inconsistent");
    }
    Ok(())
}

fn valid_logical_flags(record: &CephBluestoreLogicalExtentRecord) -> bool {
    record.flags_raw <= 0x0f
        && record.flag_contiguous == (record.flags_raw & 1 != 0)
        && record.flag_zero_blob_offset == (record.flags_raw & 2 != 0)
        && record.flag_same_length == (record.flags_raw & 4 != 0)
        && record.flag_spanning == (record.flags_raw & 8 != 0)
}

fn validate_shared_refs<'a>(
    inventory_id: &str,
    shared: &HashMap<&str, &CephBluestoreSharedBlobRecord>,
    records: &'a [CephBluestoreSharedBlobRefRecord],
) -> DbResult<HashMap<&'a str, (u64, u64, u64)>> {
    if !records.windows(2).all(|rows| {
        (rows[0].shared_blob_id_hex.as_str(), rows[0].ref_ordinal)
            < (rows[1].shared_blob_id_hex.as_str(), rows[1].ref_ordinal)
    }) {
        return semantic_error("BlueStore shared blob refs are not in canonical order");
    }
    let mut next = HashMap::new();
    let mut previous_end = HashMap::new();
    let mut totals = HashMap::new();
    for record in records {
        let id = record.shared_blob_id_hex.as_str();
        let end = parse_hex_u64(&record.ref_offset_hex)
            .and_then(|offset| offset.checked_add(record.length));
        if record.inventory_id != inventory_id
            || !shared.contains_key(id)
            || !valid_hex_u64(&record.ref_offset_hex)
            || record.length == 0
            || record.refs == 0
            || !fits_sqlite(record.length)
            || !fits_sqlite(record.refs)
            || end.is_none()
            || !take_ordinal(&mut next, id, record.ref_ordinal)
            || previous_end
                .insert(id, end.unwrap_or_default())
                .is_some_and(|previous| {
                    parse_hex_u64(&record.ref_offset_hex).is_none_or(|offset| previous > offset)
                })
        {
            return semantic_error("BlueStore shared blob ref range is inconsistent");
        }
        add_shared_totals(&mut totals, id, record.length, record.refs)?;
    }
    Ok(totals)
}

fn take_ordinal<K>(next: &mut HashMap<K, u32>, key: K, ordinal: u32) -> bool
where
    K: Eq + std::hash::Hash,
{
    let expected = next.entry(key).or_default();
    if *expected != ordinal {
        return false;
    }
    let Some(value) = expected.checked_add(1) else {
        return false;
    };
    *expected = value;
    true
}

fn increment<K>(counts: &mut HashMap<K, u64>, key: K) -> DbResult<()>
where
    K: Eq + std::hash::Hash,
{
    let count = counts.entry(key).or_default();
    *count = count
        .checked_add(1)
        .filter(|value| fits_sqlite(*value))
        .ok_or_else(|| DbError::System("BlueStore semantic count overflow".to_string()))?;
    Ok(())
}

fn add_shared_totals<'a>(
    totals: &mut HashMap<&'a str, (u64, u64, u64)>,
    id: &'a str,
    length: u64,
    refs: u64,
) -> DbResult<()> {
    let total = totals.entry(id).or_default();
    total.0 = total
        .0
        .checked_add(1)
        .filter(|value| fits_sqlite(*value))
        .ok_or_else(|| DbError::System("BlueStore shared ref count overflow".to_string()))?;
    total.1 = total
        .1
        .checked_add(length)
        .filter(|value| fits_sqlite(*value))
        .ok_or_else(|| DbError::System("BlueStore shared byte count overflow".to_string()))?;
    total.2 = total
        .2
        .checked_add(refs)
        .filter(|value| fits_sqlite(*value))
        .ok_or_else(|| DbError::System("BlueStore shared refs count overflow".to_string()))?;
    Ok(())
}
