use crate::{
    bluestore_semantic::{
        denc::{
            ensure_empty, ensure_limit, read_count, read_denc_payload, read_length_prefixed,
            read_varint_u32, read_varint_u64,
        },
        object_key::{decode_object_key, ObjectLogicalKey},
        types::{
            BlueStoreAllocationHints, BlueStoreAttributeSummary, BlueStoreDeferred,
            BlueStoreDeferredReason, BlueStoreExtentPayload, BlueStoreExtentShardDescriptor,
            BlueStoreExtentStorage, BlueStoreObjectRecord, BlueStoreOnodeFlags,
            BlueStoreOnodeHeader, BlueStoreOnodeTail, BlueStorePayloadStatus,
            BlueStoreSemanticLimits, BlueStoreZoneOffsetRef,
        },
    },
    codec::CephDecode,
    cursor::CephCursor,
    error::{CephWireError, Result},
};

pub(crate) fn decode_object(
    logical_key: &[u8],
    value: &[u8],
    limits: BlueStoreSemanticLimits,
) -> Result<BlueStoreObjectRecord> {
    match decode_object_key(logical_key, limits)? {
        ObjectLogicalKey::Onode(object) => {
            let (onode, tail) = decode_onode_value(value, limits)?;
            Ok(BlueStoreObjectRecord::Onode {
                object,
                onode,
                tail,
            })
        }
        ObjectLogicalKey::ExtentShard { object, offset } => {
            let payload = decode_extent_payload(value, limits)?;
            Ok(BlueStoreObjectRecord::ExtentShard {
                object,
                shard_offset: offset,
                payload,
            })
        }
    }
}

fn decode_onode_value(
    value: &[u8],
    limits: BlueStoreSemanticLimits,
) -> Result<(BlueStoreOnodeHeader, BlueStoreOnodeTail)> {
    let mut cursor = CephCursor::new(value);
    let onode = decode_onode_header(&mut cursor, limits)?;
    let tail = decode_onode_tail(&mut cursor, !onode.extent_shards.is_empty(), limits)?;
    Ok((onode, tail))
}

fn decode_onode_header(
    cursor: &mut CephCursor<'_>,
    limits: BlueStoreSemanticLimits,
) -> Result<BlueStoreOnodeHeader> {
    let denc = read_denc_payload(cursor, &[1, 2], "BlueStore onode")?;
    let mut payload = denc.cursor;
    let nid = read_varint_u64(&mut payload, "BlueStore onode nid")?;
    let size = read_varint_u64(&mut payload, "BlueStore onode size")?;
    let attributes = decode_attributes(&mut payload, limits)?;
    let flags = decode_flags(u8::decode(&mut payload)?);
    let extent_shards = decode_shard_descriptors(&mut payload, limits)?;
    let allocation_hints = BlueStoreAllocationHints {
        expected_object_size: read_varint_u32(&mut payload, "BlueStore expected object size")?,
        expected_write_size: read_varint_u32(&mut payload, "BlueStore expected write size")?,
        flags: read_varint_u32(&mut payload, "BlueStore allocation hint flags")?,
    };
    let zone_offset_refs = if denc.version >= 2 {
        decode_zone_refs(&mut payload, limits)?
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
) -> Result<Vec<BlueStoreAttributeSummary>> {
    let count = read_count(cursor, limits.max_attributes, "BlueStore onode attributes")?;
    let mut attributes = Vec::with_capacity(count);
    let mut total_value_bytes = 0usize;
    for _ in 0..count {
        let name =
            read_length_prefixed(cursor, limits.max_string_bytes, "BlueStore attribute name")?
                .to_vec();
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
        cursor.skip(value_length as usize)?;
        attributes.push(BlueStoreAttributeSummary { name, value_length });
    }
    Ok(attributes)
}

fn decode_shard_descriptors(
    cursor: &mut CephCursor<'_>,
    limits: BlueStoreSemanticLimits,
) -> Result<Vec<BlueStoreExtentShardDescriptor>> {
    let count = read_count(
        cursor,
        limits.max_extent_shards,
        "BlueStore extent shard descriptors",
    )?;
    let mut shards = Vec::with_capacity(count);
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
) -> Result<Vec<BlueStoreZoneOffsetRef>> {
    let count = read_count(cursor, limits.max_zone_refs, "BlueStore zone offset refs")?;
    let mut refs = Vec::with_capacity(count);
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
    is_sharded: bool,
    limits: BlueStoreSemanticLimits,
) -> Result<BlueStoreOnodeTail> {
    let encoded_length = cursor.remaining();
    let version = u8::decode(cursor)?;
    validate_extent_version(version, "BlueStore spanning blob map")?;
    let count = read_varint_u32(cursor, "BlueStore spanning blob count")?;
    ensure_limit(
        count as usize,
        limits.max_spanning_blobs,
        "BlueStore spanning blobs",
    )?;
    if count != 0 {
        return Ok(BlueStoreOnodeTail::Deferred {
            spanning_blob_version: version,
            declared_spanning_blob_count: count,
            payload: BlueStoreDeferred {
                reason: BlueStoreDeferredReason::SpanningBlobRecords,
                encoded_length,
            },
        });
    }
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
        BlueStoreExtentStorage::Inline(decode_extent_payload(bytes, limits)?)
    };
    Ok(BlueStoreOnodeTail::Decoded {
        spanning_blob_version: version,
        extents,
    })
}

pub(crate) fn decode_extent_payload(
    value: &[u8],
    limits: BlueStoreSemanticLimits,
) -> Result<BlueStoreExtentPayload> {
    ensure_limit(
        value.len(),
        limits.max_extent_payload_bytes,
        "BlueStore extent payload",
    )?;
    let mut cursor = CephCursor::new(value);
    let version = u8::decode(&mut cursor)?;
    validate_extent_version(version, "BlueStore extent map")?;
    let count = read_varint_u32(&mut cursor, "BlueStore extent count")?;
    ensure_limit(
        count as usize,
        limits.max_extent_records,
        "BlueStore extent records",
    )?;
    let status = if count == 0 {
        ensure_empty(&cursor, "BlueStore empty extent map")?;
        BlueStorePayloadStatus::Parsed
    } else {
        BlueStorePayloadStatus::Deferred(BlueStoreDeferred {
            reason: BlueStoreDeferredReason::ExtentRecords,
            encoded_length: value.len(),
        })
    };
    Ok(BlueStoreExtentPayload {
        version,
        declared_extent_count: count,
        encoded_length: value.len(),
        status,
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

fn validate_extent_version(version: u8, context: &'static str) -> Result<()> {
    if matches!(version, 1 | 2) {
        Ok(())
    } else {
        Err(CephWireError::UnsupportedBlueStoreDencVersion {
            context,
            encoded_version: version,
            supported_versions: "1 or 2",
        })
    }
}

fn invalid_value(context: &'static str, reason: &'static str) -> CephWireError {
    CephWireError::InvalidBlueStoreSemanticValue { context, reason }
}
