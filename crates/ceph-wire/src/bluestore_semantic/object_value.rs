use std::{mem::size_of, sync::Arc};

use crate::{
    bluestore_semantic::{
        budget::SemanticBudget,
        denc::{
            ensure_empty, ensure_limit, read_count, read_denc_payload, read_length_prefixed,
            read_varint_u32, read_varint_u64,
        },
        extent::{
            decode_extent_payload_with_budget,
            decode_extent_payload_without_spanning_context_with_budget, decode_spanning_blobs,
        },
        object_key::decode_bluestore_object_key,
        types::{
            BlueStoreAllocationHints, BlueStoreAttributeSummary, BlueStoreBlob, BlueStoreDeferred,
            BlueStoreDeferredReason, BlueStoreExtentShardDescriptor, BlueStoreExtentStorage,
            BlueStoreObjectId, BlueStoreObjectKey, BlueStoreObjectRecord, BlueStoreOnodeFlags,
            BlueStoreOnodeHeader, BlueStoreOnodeTail, BlueStoreSemanticLimits,
            BlueStoreZoneOffsetRef,
        },
    },
    codec::CephDecode,
    cursor::CephCursor,
    error::{CephWireError, Result},
};
use sha2::{Digest, Sha256};

pub(crate) fn decode_object(
    logical_key: &[u8],
    value: &[u8],
    spanning_blobs: Option<&[BlueStoreBlob]>,
    limits: BlueStoreSemanticLimits,
) -> Result<BlueStoreObjectRecord> {
    let mut budget = SemanticBudget::new(limits);
    budget.claim_input(logical_key.len().checked_add(value.len()).ok_or(
        CephWireError::LengthOverflow {
            context: "BlueStore object input",
        },
    )?)?;
    budget.claim_retained_bytes(logical_key.len())?;
    match decode_bluestore_object_key(logical_key, limits)? {
        BlueStoreObjectKey::Onode(object) => {
            let (onode, tail) = decode_onode_value(value, &object, limits, &mut budget)?;
            Ok(BlueStoreObjectRecord::Onode {
                object,
                onode,
                tail,
            })
        }
        BlueStoreObjectKey::ExtentShard {
            object,
            shard_offset,
        } => decode_extent_shard(
            object,
            shard_offset,
            value,
            spanning_blobs,
            limits,
            &mut budget,
        ),
    }
}

fn decode_onode_value(
    value: &[u8],
    object: &BlueStoreObjectId,
    limits: BlueStoreSemanticLimits,
    budget: &mut SemanticBudget,
) -> Result<(BlueStoreOnodeHeader, BlueStoreOnodeTail)> {
    let mut cursor = CephCursor::new(value);
    let onode = decode_onode_header(&mut cursor, limits, budget)?;
    let tail = decode_onode_tail(
        &mut cursor,
        object,
        !onode.extent_shards.is_empty(),
        limits,
        budget,
    )?;
    Ok((onode, tail))
}

fn decode_onode_header(
    cursor: &mut CephCursor<'_>,
    limits: BlueStoreSemanticLimits,
    budget: &mut SemanticBudget,
) -> Result<BlueStoreOnodeHeader> {
    let denc = read_denc_payload(cursor, &[1, 2], "BlueStore onode")?;
    let mut payload = denc.cursor;
    let nid = read_varint_u64(&mut payload, "BlueStore onode nid")?;
    let size = read_varint_u64(&mut payload, "BlueStore onode size")?;
    let attributes = decode_attributes(&mut payload, limits, budget)?;
    let flags = decode_flags(u8::decode(&mut payload)?);
    let extent_shards = decode_shard_descriptors(&mut payload, limits, budget)?;
    let allocation_hints = BlueStoreAllocationHints {
        expected_object_size: read_varint_u32(&mut payload, "BlueStore expected object size")?,
        expected_write_size: read_varint_u32(&mut payload, "BlueStore expected write size")?,
        flags: read_varint_u32(&mut payload, "BlueStore allocation hint flags")?,
    };
    let zone_offset_refs = if denc.version >= 2 {
        decode_zone_refs(&mut payload, limits, budget)?
    } else {
        Vec::new()
    };
    ensure_empty(&payload, "BlueStore onode DENC payload")?;
    Ok(BlueStoreOnodeHeader {
        denc_version: denc.version,
        nid,
        size,
        attributes,
        flags,
        extent_shards,
        allocation_hints,
        zone_offset_refs,
    })
}

fn decode_attributes(
    cursor: &mut CephCursor<'_>,
    limits: BlueStoreSemanticLimits,
    budget: &mut SemanticBudget,
) -> Result<Vec<BlueStoreAttributeSummary>> {
    let count = read_count(cursor, limits.max_attributes, "BlueStore onode attributes")?;
    budget.claim_attributes(count)?;
    let mut attributes = Vec::new();
    let mut total_value_bytes = 0usize;
    for _ in 0..count {
        let name_bytes =
            read_length_prefixed(cursor, limits.max_string_bytes, "BlueStore attribute name")?;
        budget.claim_retained_bytes(name_bytes.len())?;
        let name = name_bytes.to_vec();
        if attributes
            .last()
            .is_some_and(|previous: &BlueStoreAttributeSummary| previous.name >= name)
        {
            return Err(invalid_value(
                "BlueStore onode attributes",
                "attribute names are not strictly ordered",
            ));
        }
        let value_length = u32::decode(cursor)?;
        ensure_limit(
            value_length as usize,
            limits.max_attribute_value_bytes,
            "BlueStore attribute value",
        )?;
        total_value_bytes = total_value_bytes.checked_add(value_length as usize).ok_or(
            CephWireError::LengthOverflow {
                context: "BlueStore attribute values",
            },
        )?;
        ensure_limit(
            total_value_bytes,
            limits.max_total_attribute_value_bytes,
            "BlueStore total attribute values",
        )?;
        budget.claim_input(value_length as usize)?;
        let value = cursor.read_exact(value_length as usize)?;
        attributes.push(BlueStoreAttributeSummary {
            name,
            value_length,
            value_sha256: Sha256::digest(value).into(),
        });
    }
    Ok(attributes)
}

fn decode_shard_descriptors(
    cursor: &mut CephCursor<'_>,
    limits: BlueStoreSemanticLimits,
    budget: &mut SemanticBudget,
) -> Result<Vec<BlueStoreExtentShardDescriptor>> {
    let count = read_count(
        cursor,
        limits.max_extent_shards,
        "BlueStore extent shard descriptors",
    )?;
    budget.claim_extent_shards(count)?;
    let mut shards = Vec::new();
    for _ in 0..count {
        let descriptor = BlueStoreExtentShardDescriptor {
            offset: read_varint_u32(cursor, "BlueStore extent shard offset")?,
            bytes: read_varint_u32(cursor, "BlueStore extent shard bytes")?,
        };
        if shards
            .last()
            .is_some_and(|previous: &BlueStoreExtentShardDescriptor| {
                previous.offset >= descriptor.offset
            })
        {
            return Err(invalid_value(
                "BlueStore extent shard descriptors",
                "shard offsets are not strictly ordered",
            ));
        }
        shards.push(descriptor);
    }
    Ok(shards)
}

fn decode_zone_refs(
    cursor: &mut CephCursor<'_>,
    limits: BlueStoreSemanticLimits,
    budget: &mut SemanticBudget,
) -> Result<Vec<BlueStoreZoneOffsetRef>> {
    let count = read_count(cursor, limits.max_zone_refs, "BlueStore zone offset refs")?;
    budget.claim_zone_refs(count)?;
    let mut refs = Vec::new();
    for _ in 0..count {
        let entry = BlueStoreZoneOffsetRef {
            zone: u32::decode(cursor)?,
            offset: u64::decode(cursor)?,
        };
        if refs
            .last()
            .is_some_and(|previous: &BlueStoreZoneOffsetRef| previous.zone >= entry.zone)
        {
            return Err(invalid_value(
                "BlueStore zone offset refs",
                "zone ids are not strictly ordered",
            ));
        }
        refs.push(entry);
    }
    Ok(refs)
}

fn decode_onode_tail(
    cursor: &mut CephCursor<'_>,
    object: &BlueStoreObjectId,
    is_sharded: bool,
    limits: BlueStoreSemanticLimits,
    budget: &mut SemanticBudget,
) -> Result<BlueStoreOnodeTail> {
    let (spanning_blob_version, mut spanning_blobs) =
        decode_spanning_blobs(cursor, limits, budget)?;
    bind_spanning_blob_owners(&mut spanning_blobs, object, budget)?;
    let extents = if is_sharded {
        ensure_empty(cursor, "BlueStore sharded onode value")?;
        BlueStoreExtentStorage::Sharded
    } else {
        let bytes = read_length_prefixed(
            cursor,
            limits.max_extent_payload_bytes,
            "BlueStore inline extent payload",
        )?;
        ensure_empty(cursor, "BlueStore inline onode value")?;
        BlueStoreExtentStorage::Inline(decode_extent_payload_with_budget(
            bytes,
            &spanning_blobs,
            limits,
            budget,
        )?)
    };
    Ok(BlueStoreOnodeTail::Decoded {
        spanning_blob_version,
        spanning_blobs,
        extents,
    })
}

fn decode_extent_shard(
    object: BlueStoreObjectId,
    shard_offset: u32,
    value: &[u8],
    spanning_blobs: Option<&[BlueStoreBlob]>,
    limits: BlueStoreSemanticLimits,
    budget: &mut SemanticBudget,
) -> Result<BlueStoreObjectRecord> {
    if let Some(context) = spanning_blobs {
        validate_spanning_blob_owners(&object, context)?;
        let payload = decode_extent_payload_with_budget(value, context, limits, budget)?;
        return Ok(BlueStoreObjectRecord::ExtentShard {
            object,
            shard_offset,
            payload,
        });
    }
    let (payload, missing_spanning) =
        decode_extent_payload_without_spanning_context_with_budget(value, limits, budget)?;
    if missing_spanning {
        Ok(BlueStoreObjectRecord::DeferredExtentShard {
            object,
            shard_offset,
            payload: BlueStoreDeferred {
                reason: BlueStoreDeferredReason::SpanningBlobContextRequired,
                encoded_length: value.len(),
            },
        })
    } else {
        Ok(BlueStoreObjectRecord::ExtentShard {
            object,
            shard_offset,
            payload,
        })
    }
}

fn bind_spanning_blob_owners(
    blobs: &mut [BlueStoreBlob],
    object: &BlueStoreObjectId,
    budget: &mut SemanticBudget,
) -> Result<()> {
    if blobs.is_empty() {
        return Ok(());
    }
    budget.claim_retained_bytes(object_heap_bytes(object)?)?;
    let owner = Arc::new(object.clone());
    for blob in blobs {
        blob.owner = Some(Arc::clone(&owner));
    }
    Ok(())
}

fn validate_spanning_blob_owners(
    object: &BlueStoreObjectId,
    blobs: &[BlueStoreBlob],
) -> Result<()> {
    if blobs
        .iter()
        .any(|blob| blob.owner.as_deref() != Some(object))
    {
        return Err(CephWireError::BlueStoreSpanningBlobOwnerMismatch);
    }
    Ok(())
}

fn object_heap_bytes(object: &BlueStoreObjectId) -> Result<usize> {
    [
        size_of::<BlueStoreObjectId>(),
        object.namespace.len(),
        object.object_key.as_ref().map_or(0, Vec::len),
        object.object_name.len(),
    ]
    .into_iter()
    .try_fold(0usize, |total, value| {
        total
            .checked_add(value)
            .ok_or(CephWireError::LengthOverflow {
                context: "BlueStore spanning blob owner",
            })
    })
}

fn decode_flags(raw: u8) -> BlueStoreOnodeFlags {
    BlueStoreOnodeFlags {
        raw,
        omap: raw & 1 != 0,
        pgmeta_omap: raw & 2 != 0,
        per_pool_omap: raw & 4 != 0,
        per_pg_omap: raw & 8 != 0,
        unknown_bits: raw & !0x0f,
    }
}

fn invalid_value(context: &'static str, reason: &'static str) -> CephWireError {
    CephWireError::InvalidBlueStoreSemanticValue { context, reason }
}
