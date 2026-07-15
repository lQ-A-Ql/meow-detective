use crate::{
    bluestore_semantic::{
        budget::SemanticBudget,
        checksum::decode_checksum,
        denc::{read_varint_lowz_u64, read_varint_u32},
        types::{
            BlueStoreBlob, BlueStoreBlobFlags, BlueStoreBlobIdentity, BlueStoreBlobUseRef,
            BlueStoreBlobUseTracker, BlueStorePhysicalExtent,
        },
    },
    codec::{decode_lba_u64, CephDecode},
    cursor::CephCursor,
    error::{CephWireError, Result},
};

const BLOB_FLAG_LEGACY_MUTABLE: u32 = 1;
const BLOB_FLAG_COMPRESSED: u32 = 2;
const BLOB_FLAG_CHECKSUM: u32 = 4;
const BLOB_FLAG_UNUSED: u32 = 8;
const BLOB_FLAG_SHARED: u32 = 16;
const KNOWN_BLOB_FLAGS: u32 = 0x1f;
const INVALID_PHYSICAL_OFFSET: u64 = u64::MAX;

pub(crate) fn decode_blob(
    cursor: &mut CephCursor<'_>,
    version: u8,
    identity: BlueStoreBlobIdentity,
    include_use_tracker: bool,
    budget: &mut SemanticBudget,
) -> Result<BlueStoreBlob> {
    let (physical_extents, on_disk_length) = decode_physical_extents(cursor, budget)?;
    let flags = decode_blob_flags(read_varint_u32(cursor, "BlueStore blob flags")?)?;
    let (logical_length, compressed_length) = decode_blob_lengths(cursor, flags, on_disk_length)?;
    let (checksum, checksum_words) = if flags.checksum {
        let (summary, words) = decode_checksum(cursor, on_disk_length, budget)?;
        (Some(summary), words)
    } else {
        (None, Vec::new())
    };
    let unused_bitmap = if flags.has_unused {
        Some(u16::decode(cursor)?)
    } else {
        None
    };
    let shared_blob_id = if flags.shared {
        Some(u64::decode(cursor)?)
    } else {
        None
    };
    validate_blob_metadata(
        &physical_extents,
        flags,
        on_disk_length,
        logical_length,
        compressed_length,
        unused_bitmap,
        shared_blob_id,
    )?;
    let use_tracker = if include_use_tracker {
        Some(decode_use_tracker(cursor, version, logical_length, budget)?)
    } else {
        None
    };
    Ok(BlueStoreBlob {
        identity,
        owner: None,
        physical_extents,
        on_disk_length,
        logical_length,
        compressed_length,
        flags,
        checksum,
        checksum_words,
        unused_bitmap,
        shared_blob_id,
        use_tracker,
    })
}

fn decode_physical_extents(
    cursor: &mut CephCursor<'_>,
    budget: &mut SemanticBudget,
) -> Result<(Vec<BlueStorePhysicalExtent>, u32)> {
    let count = read_varint_u32(cursor, "BlueStore physical extent count")? as usize;
    budget.claim_physical_extents(count)?;
    let mut extents = Vec::new();
    let mut total_length = 0u64;
    for index in 0..count {
        let raw_offset = decode_lba_u64(cursor, "BlueStore physical extent offset")?;
        let length = lowz_u32(cursor, "BlueStore physical extent length")?;
        validate_physical_extent(index, raw_offset, length)?;
        total_length =
            total_length
                .checked_add(u64::from(length))
                .ok_or(CephWireError::IntegerOverflow {
                    context: "BlueStore blob on-disk length",
                })?;
        extents.push(BlueStorePhysicalExtent {
            offset: (raw_offset != INVALID_PHYSICAL_OFFSET).then_some(raw_offset),
            length,
        });
    }
    let total_length = u32::try_from(total_length).map_err(|_| CephWireError::IntegerOverflow {
        context: "BlueStore blob on-disk length",
    })?;
    Ok((extents, total_length))
}

fn validate_physical_extent(index: usize, offset: u64, length: u32) -> Result<()> {
    if length == 0 {
        return Err(CephWireError::InvalidBlueStorePhysicalExtent {
            index,
            offset,
            length,
            reason: "length must be non-zero",
        });
    }
    if offset != INVALID_PHYSICAL_OFFSET && offset.checked_add(u64::from(length)).is_none() {
        return Err(CephWireError::InvalidBlueStorePhysicalExtent {
            index,
            offset,
            length,
            reason: "allocated range overflows u64",
        });
    }
    Ok(())
}

fn decode_blob_flags(raw: u32) -> Result<BlueStoreBlobFlags> {
    let unknown_bits = raw & !KNOWN_BLOB_FLAGS;
    if unknown_bits != 0 {
        return Err(CephWireError::UnknownBlueStoreBlobFlags {
            flags: raw,
            unknown_bits,
        });
    }
    Ok(BlueStoreBlobFlags {
        raw,
        legacy_mutable: raw & BLOB_FLAG_LEGACY_MUTABLE != 0,
        compressed: raw & BLOB_FLAG_COMPRESSED != 0,
        checksum: raw & BLOB_FLAG_CHECKSUM != 0,
        has_unused: raw & BLOB_FLAG_UNUSED != 0,
        shared: raw & BLOB_FLAG_SHARED != 0,
        unknown_bits,
    })
}

fn decode_blob_lengths(
    cursor: &mut CephCursor<'_>,
    flags: BlueStoreBlobFlags,
    on_disk_length: u32,
) -> Result<(u32, Option<u32>)> {
    if !flags.compressed {
        return Ok((on_disk_length, None));
    }
    let logical_length = lowz_u32(cursor, "BlueStore blob logical length")?;
    let compressed_length = lowz_u32(cursor, "BlueStore blob compressed length")?;
    Ok((logical_length, Some(compressed_length)))
}

fn validate_blob_metadata(
    physical_extents: &[BlueStorePhysicalExtent],
    flags: BlueStoreBlobFlags,
    on_disk_length: u32,
    logical_length: u32,
    compressed_length: Option<u32>,
    unused_bitmap: Option<u16>,
    shared_blob_id: Option<u64>,
) -> Result<()> {
    if flags.shared && flags.has_unused {
        return Err(invalid_blob("shared blobs cannot carry an unused bitmap"));
    }
    if flags.compressed && flags.has_unused {
        return Err(invalid_blob(
            "compressed blobs cannot carry an unused bitmap",
        ));
    }
    if let Some(compressed_length) = compressed_length {
        if logical_length == 0 || compressed_length == 0 {
            return Err(invalid_blob("compressed lengths must be non-zero"));
        }
        if compressed_length > on_disk_length {
            return Err(invalid_blob(
                "compressed payload exceeds the on-disk extent length",
            ));
        }
    }
    if flags.shared && shared_blob_id == Some(0) {
        return Err(invalid_blob("shared blob id must be non-zero"));
    }
    if let Some(unused) = unused_bitmap {
        if unused == 0 || logical_length == 0 || !logical_length.is_multiple_of(u16::BITS) {
            return Err(invalid_blob(
                "unused bitmap requires non-zero bits and 16 equal logical chunks",
            ));
        }
    }
    validate_physical_layout(physical_extents, flags.compressed)?;
    Ok(())
}

fn validate_physical_layout(
    physical_extents: &[BlueStorePhysicalExtent],
    compressed: bool,
) -> Result<()> {
    if compressed {
        let allocated = physical_extents
            .iter()
            .filter(|extent| extent.offset.is_some())
            .count();
        if allocated != 0 && allocated != physical_extents.len() {
            return Err(invalid_blob(
                "compressed blob physical extents must be uniformly allocated",
            ));
        }
    }
    let mut allocated = physical_extents
        .iter()
        .filter_map(|extent| {
            extent
                .offset
                .map(|offset| (offset, offset.saturating_add(u64::from(extent.length))))
        })
        .collect::<Vec<_>>();
    allocated.sort_unstable();
    if allocated.windows(2).any(|ranges| ranges[0].1 > ranges[1].0) {
        return Err(invalid_blob("allocated physical extents overlap"));
    }
    Ok(())
}

fn decode_use_tracker(
    cursor: &mut CephCursor<'_>,
    version: u8,
    logical_length: u32,
    budget: &mut SemanticBudget,
) -> Result<BlueStoreBlobUseTracker> {
    if version == 1 {
        decode_legacy_ref_map(cursor, logical_length, budget)
    } else {
        decode_v2_use_tracker(cursor, logical_length, budget)
    }
}

fn decode_v2_use_tracker(
    cursor: &mut CephCursor<'_>,
    logical_length: u32,
    budget: &mut SemanticBudget,
) -> Result<BlueStoreBlobUseTracker> {
    let allocation_unit_size = read_varint_u32(cursor, "BlueStore use tracker AU size")?;
    if allocation_unit_size == 0 {
        return Ok(BlueStoreBlobUseTracker::V2 {
            allocation_unit_size,
            declared_allocation_units: 0,
            referenced_bytes: Vec::new(),
        });
    }
    let declared = read_varint_u32(cursor, "BlueStore use tracker AU count")?;
    let entry_count = if declared == 0 { 1 } else { declared as usize };
    budget.claim_use_tracker_entries(entry_count)?;
    let mut referenced_bytes = Vec::new();
    for _ in 0..entry_count {
        let bytes = read_varint_u32(cursor, "BlueStore use tracker referenced bytes")?;
        referenced_bytes.push(bytes);
    }
    validate_tracker_coverage(allocation_unit_size, declared, logical_length)?;
    Ok(BlueStoreBlobUseTracker::V2 {
        allocation_unit_size,
        declared_allocation_units: declared,
        referenced_bytes,
    })
}

fn validate_tracker_coverage(au_size: u32, declared: u32, logical_length: u32) -> Result<()> {
    if au_size == 0 || logical_length == 0 || declared == 1 {
        return Err(invalid_tracker(
            "allocation-unit metadata is not in Ceph's canonical form",
        ));
    }
    let expected = u64::from(logical_length).div_ceil(u64::from(au_size));
    if u64::from(declared.max(1)) != expected {
        return Err(invalid_tracker(
            "allocation-unit count does not exactly cover the blob logical length",
        ));
    }
    Ok(())
}

fn decode_legacy_ref_map(
    cursor: &mut CephCursor<'_>,
    logical_length: u32,
    budget: &mut SemanticBudget,
) -> Result<BlueStoreBlobUseTracker> {
    let count = read_varint_u32(cursor, "BlueStore legacy ref-map count")? as usize;
    budget.claim_use_tracker_entries(count)?;
    let mut entries = Vec::new();
    let mut position = 0u64;
    for index in 0..count {
        let delta = read_varint_lowz_u64(cursor, "BlueStore legacy ref-map offset")?;
        if delta > i64::MAX as u64 {
            return Err(CephWireError::IntegerOverflow {
                context: "BlueStore legacy ref-map offset",
            });
        }
        position = if index == 0 {
            delta
        } else {
            position
                .checked_add(delta)
                .ok_or(CephWireError::IntegerOverflow {
                    context: "BlueStore legacy ref-map offset",
                })?
        };
        if position > i64::MAX as u64 {
            return Err(CephWireError::IntegerOverflow {
                context: "BlueStore legacy ref-map offset",
            });
        }
        let entry = BlueStoreBlobUseRef {
            offset: position,
            length: lowz_u32(cursor, "BlueStore legacy ref-map length")?,
            refs: read_varint_u32(cursor, "BlueStore legacy ref-map refs")?,
        };
        validate_legacy_ref(&entries, entry, logical_length)?;
        entries.push(entry);
    }
    Ok(BlueStoreBlobUseTracker::V1LegacyRefMap { entries })
}

fn validate_legacy_ref(
    entries: &[BlueStoreBlobUseRef],
    entry: BlueStoreBlobUseRef,
    logical_length: u32,
) -> Result<()> {
    if entry.length == 0 || entry.refs == 0 {
        return Err(invalid_tracker("legacy ref-map entries must be non-zero"));
    }
    let end = entry.offset.checked_add(u64::from(entry.length)).ok_or(
        CephWireError::IntegerOverflow {
            context: "BlueStore legacy ref-map end",
        },
    )?;
    if end > u64::from(logical_length) {
        return Err(invalid_tracker(
            "legacy ref-map entry exceeds the blob logical length",
        ));
    }
    if let Some(previous) = entries.last() {
        let previous_end = previous.offset + u64::from(previous.length);
        if entry.offset < previous_end {
            return Err(invalid_tracker("legacy ref-map entries overlap"));
        }
        if entry.offset == previous_end && entry.refs == previous.refs {
            return Err(invalid_tracker(
                "adjacent equal-ref legacy entries are not canonical",
            ));
        }
    }
    Ok(())
}

fn lowz_u32(cursor: &mut CephCursor<'_>, context: &'static str) -> Result<u32> {
    let value = read_varint_lowz_u64(cursor, context)?;
    u32::try_from(value).map_err(|_| CephWireError::IntegerOverflow { context })
}

fn invalid_blob(reason: &'static str) -> CephWireError {
    CephWireError::InvalidBlueStoreSemanticValue {
        context: "BlueStore blob",
        reason,
    }
}

fn invalid_tracker(reason: &'static str) -> CephWireError {
    CephWireError::InvalidBlueStoreSemanticValue {
        context: "BlueStore blob use tracker",
        reason,
    }
}
