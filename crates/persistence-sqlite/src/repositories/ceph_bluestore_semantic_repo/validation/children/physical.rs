use std::{
    cmp::Reverse,
    collections::{BinaryHeap, HashMap, HashSet},
};

use crate::connection::DbResult;

use super::{
    super::{
        super::{
            CephBluestoreBlobRecord, CephBluestorePhysicalExtentRecord,
            CephBluestoreSharedBlobRefRecord,
        },
        primitives::{fits_sqlite, parse_hex_u64, semantic_error},
    },
    increment, take_ordinal, BlobKey,
};

#[derive(Clone, Copy)]
struct AllocatedRange<'a> {
    device_id: u8,
    start: u64,
    end: u64,
    object_id: &'a str,
    blob_ordinal: u32,
    extent_ordinal: u32,
    shared_blob_id: Option<&'a str>,
}

pub(super) fn validate_physical_extents<'a>(
    inventory_id: &str,
    blobs: &HashMap<BlobKey<'a>, &'a CephBluestoreBlobRecord>,
    records: &'a [CephBluestorePhysicalExtentRecord],
    shared_refs: &'a [CephBluestoreSharedBlobRefRecord],
) -> DbResult<HashMap<BlobKey<'a>, u64>> {
    ensure_physical_order(records)?;
    let mut next = HashMap::new();
    let mut blob_end = HashMap::new();
    let mut counts = HashMap::new();
    for record in records {
        validate_physical_extent(inventory_id, blobs, record, &mut next, &mut blob_end)?;
        increment(
            &mut counts,
            (record.object_identity_sha256.as_str(), record.blob_ordinal),
        )?;
    }
    ensure_blob_lengths_close(blobs, &blob_end)?;
    let ranges = allocated_ranges(blobs, records);
    validate_blob_internal_ranges(&ranges)?;
    validate_cross_blob_ranges(&ranges)?;
    validate_shared_ref_coverage(&ranges, shared_refs)?;
    Ok(counts)
}

fn validate_physical_extent<'a>(
    inventory_id: &str,
    blobs: &HashMap<BlobKey<'a>, &'a CephBluestoreBlobRecord>,
    record: &'a CephBluestorePhysicalExtentRecord,
    next: &mut HashMap<BlobKey<'a>, u32>,
    blob_end: &mut HashMap<BlobKey<'a>, u64>,
) -> DbResult<()> {
    let key = (record.object_identity_sha256.as_str(), record.blob_ordinal);
    let Some(blob) = blobs.get(&key) else {
        return semantic_error("BlueStore physical extent references an unknown blob");
    };
    let end = record.blob_offset.checked_add(record.length);
    let valid_physical_offset = record.physical_offset_hex.as_deref().is_none_or(|offset| {
        parse_hex_u64(offset)
            .and_then(|offset| offset.checked_add(record.length))
            .is_some()
    });
    if record.inventory_id != inventory_id
        || record.length == 0
        || ![record.blob_offset, record.length]
            .into_iter()
            .all(fits_sqlite)
        || end.is_none_or(|end| end > blob.on_disk_length)
        || !valid_physical_offset
        || !take_ordinal(next, key, record.extent_ordinal)
        || blob_end.get(&key).copied().unwrap_or(0) != record.blob_offset
    {
        return semantic_error("BlueStore physical extent range is inconsistent");
    }
    blob_end.insert(key, end.unwrap_or_default());
    Ok(())
}

fn ensure_blob_lengths_close(
    blobs: &HashMap<BlobKey<'_>, &CephBluestoreBlobRecord>,
    blob_end: &HashMap<BlobKey<'_>, u64>,
) -> DbResult<()> {
    for (key, blob) in blobs {
        if blob_end.get(key).copied().unwrap_or(0) != blob.on_disk_length {
            return semantic_error("BlueStore physical extents do not close the blob length");
        }
    }
    Ok(())
}

fn allocated_ranges<'a>(
    blobs: &HashMap<BlobKey<'a>, &'a CephBluestoreBlobRecord>,
    records: &'a [CephBluestorePhysicalExtentRecord],
) -> Vec<AllocatedRange<'a>> {
    records
        .iter()
        .filter_map(|record| {
            let start = parse_hex_u64(record.physical_offset_hex.as_deref()?)?;
            let end = start.checked_add(record.length)?;
            let blob = blobs.get(&(record.object_identity_sha256.as_str(), record.blob_ordinal))?;
            Some(AllocatedRange {
                device_id: record.device_id,
                start,
                end,
                object_id: record.object_identity_sha256.as_str(),
                blob_ordinal: record.blob_ordinal,
                extent_ordinal: record.extent_ordinal,
                shared_blob_id: blob.shared_blob_id_hex.as_deref(),
            })
        })
        .collect()
}

fn validate_blob_internal_ranges(ranges: &[AllocatedRange<'_>]) -> DbResult<()> {
    let mut ordered = ranges.to_vec();
    ordered.sort_unstable_by_key(|range| {
        (
            range.object_id,
            range.blob_ordinal,
            range.device_id,
            range.start,
            range.end,
        )
    });
    for pair in ordered.windows(2) {
        let previous = pair[0];
        let current = pair[1];
        if previous.object_id == current.object_id
            && previous.blob_ordinal == current.blob_ordinal
            && previous.device_id == current.device_id
            && current.start < previous.end
        {
            return overlap_error("within one blob", previous, current);
        }
    }
    Ok(())
}

fn validate_cross_blob_ranges(ranges: &[AllocatedRange<'_>]) -> DbResult<()> {
    let mut ordered = ranges.to_vec();
    ordered.sort_unstable_by_key(|range| {
        (
            range.device_id,
            range.start,
            range.end,
            range.object_id,
            range.blob_ordinal,
        )
    });
    let mut device = None;
    let mut active_end = BinaryHeap::<Reverse<(u64, usize)>>::new();
    let mut active_indices = HashSet::new();
    let mut active_shared = HashMap::<Option<&str>, usize>::new();
    for (index, current) in ordered.iter().copied().enumerate() {
        if device != Some(current.device_id) {
            device = Some(current.device_id);
            active_end.clear();
            active_indices.clear();
            active_shared.clear();
        }
        retire_finished_ranges(
            current.start,
            &ordered,
            &mut active_end,
            &mut active_indices,
            &mut active_shared,
        );
        let overlap_is_allowed = current.shared_blob_id.is_some()
            && active_shared.len() == 1
            && active_shared.contains_key(&current.shared_blob_id);
        if !(active_indices.is_empty() || overlap_is_allowed) {
            let Some(conflict) = active_indices
                .iter()
                .copied()
                .find(|active| {
                    current.shared_blob_id.is_none()
                        || ordered[*active].shared_blob_id != current.shared_blob_id
                })
                .or_else(|| active_indices.iter().copied().next())
            else {
                return semantic_error("BlueStore active physical range set is inconsistent");
            };
            return overlap_error("across unrelated blobs", ordered[conflict], current);
        }
        active_end.push(Reverse((current.end, index)));
        active_indices.insert(index);
        *active_shared.entry(current.shared_blob_id).or_default() += 1;
    }
    Ok(())
}

fn retire_finished_ranges<'a>(
    start: u64,
    ranges: &[AllocatedRange<'a>],
    active_end: &mut BinaryHeap<Reverse<(u64, usize)>>,
    active_indices: &mut HashSet<usize>,
    active_shared: &mut HashMap<Option<&'a str>, usize>,
) {
    while let Some(Reverse((end, index))) = active_end.peek().copied() {
        if end > start {
            break;
        }
        active_end.pop();
        if active_indices.remove(&index) {
            let key = ranges[index].shared_blob_id;
            let remove = active_shared.get_mut(&key).is_some_and(|count| {
                if *count > 1 {
                    *count -= 1;
                    false
                } else {
                    true
                }
            });
            if remove {
                active_shared.remove(&key);
            }
        }
    }
}

fn validate_shared_ref_coverage(
    ranges: &[AllocatedRange<'_>],
    records: &[CephBluestoreSharedBlobRefRecord],
) -> DbResult<()> {
    let coverage = shared_ref_ranges(records);
    for range in ranges {
        let Some(shared_blob_id) = range.shared_blob_id else {
            continue;
        };
        if coverage
            .get(shared_blob_id)
            .is_none_or(|refs| !range_is_covered(range.start, range.end, refs))
        {
            return semantic_error(&format!(
                "BlueStore shared physical range is outside its ref map: device={} \
                 range={:#x}..{:#x} object={} blob={} extent={} shared={}",
                range.device_id,
                range.start,
                range.end,
                object_prefix(range.object_id),
                range.blob_ordinal,
                range.extent_ordinal,
                shared_blob_id,
            ));
        }
    }
    Ok(())
}

fn shared_ref_ranges(
    records: &[CephBluestoreSharedBlobRefRecord],
) -> HashMap<&str, Vec<(u64, u64)>> {
    let mut ranges = HashMap::<&str, Vec<(u64, u64)>>::new();
    for record in records {
        let Some(start) = parse_hex_u64(&record.ref_offset_hex) else {
            continue;
        };
        let Some(end) = start.checked_add(record.length) else {
            continue;
        };
        ranges
            .entry(record.shared_blob_id_hex.as_str())
            .or_default()
            .push((start, end));
    }
    ranges
}

fn range_is_covered(start: u64, end: u64, refs: &[(u64, u64)]) -> bool {
    let mut cursor = start;
    let mut index = refs.partition_point(|(_, ref_end)| *ref_end <= cursor);
    while cursor < end {
        let Some((ref_start, ref_end)) = refs.get(index).copied() else {
            return false;
        };
        if ref_start > cursor {
            return false;
        }
        cursor = ref_end;
        index += 1;
    }
    true
}

fn overlap_error<T>(
    context: &str,
    previous: AllocatedRange<'_>,
    current: AllocatedRange<'_>,
) -> DbResult<T> {
    semantic_error(&format!(
        "BlueStore physical ranges overlap {context}: device={} previous={:#x}..{:#x} \
         object={} blob={} extent={} shared={:?}; current={:#x}..{:#x} \
         object={} blob={} extent={} shared={:?}",
        previous.device_id,
        previous.start,
        previous.end,
        object_prefix(previous.object_id),
        previous.blob_ordinal,
        previous.extent_ordinal,
        previous.shared_blob_id,
        current.start,
        current.end,
        object_prefix(current.object_id),
        current.blob_ordinal,
        current.extent_ordinal,
        current.shared_blob_id,
    ))
}

fn ensure_physical_order(records: &[CephBluestorePhysicalExtentRecord]) -> DbResult<()> {
    if records.windows(2).all(|rows| {
        (
            rows[0].object_identity_sha256.as_str(),
            rows[0].blob_ordinal,
            rows[0].extent_ordinal,
        ) < (
            rows[1].object_identity_sha256.as_str(),
            rows[1].blob_ordinal,
            rows[1].extent_ordinal,
        )
    }) {
        Ok(())
    } else {
        semantic_error("BlueStore physical extents are not in canonical order")
    }
}

fn object_prefix(object_id: &str) -> &str {
    &object_id[..object_id.len().min(12)]
}
