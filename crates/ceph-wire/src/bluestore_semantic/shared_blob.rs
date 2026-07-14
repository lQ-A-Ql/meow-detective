use crate::{
    bluestore_semantic::{
        denc::{
            ensure_empty, ensure_limit, read_denc_payload, read_varint_lowz_u64, read_varint_u32,
        },
        types::{BlueStoreSemanticLimits, BlueStoreSharedBlobExtentRef, BlueStoreSharedBlobRecord},
    },
    cursor::CephCursor,
    error::{CephWireError, Result},
};

pub(crate) fn decode_shared_blob(
    logical_key: &[u8],
    value: &[u8],
    limits: BlueStoreSemanticLimits,
) -> Result<BlueStoreSharedBlobRecord> {
    let sbid = decode_shared_blob_key(logical_key)?;
    let mut cursor = CephCursor::new(value);
    let denc = read_denc_payload(&mut cursor, &[1], "BlueStore shared blob")?;
    let mut payload = denc.cursor;
    let extent_refs = decode_ref_map(&mut payload, limits)?;
    ensure_empty(&payload, "BlueStore shared blob DENC payload")?;
    ensure_empty(&cursor, "BlueStore shared blob value")?;
    Ok(BlueStoreSharedBlobRecord {
        sbid,
        denc_version: denc.version,
        extent_refs,
    })
}

fn decode_shared_blob_key(logical_key: &[u8]) -> Result<u64> {
    let bytes: [u8; 8] =
        logical_key
            .try_into()
            .map_err(|_| CephWireError::InvalidBlueStoreSemanticKey {
                key_space: "shared blob",
                reason: "expected exactly one sortable big-endian u64",
            })?;
    Ok(u64::from_be_bytes(bytes))
}

fn decode_ref_map(
    cursor: &mut CephCursor<'_>,
    limits: BlueStoreSemanticLimits,
) -> Result<Vec<BlueStoreSharedBlobExtentRef>> {
    let count = read_varint_u32(cursor, "BlueStore shared blob ref count")? as usize;
    ensure_limit(
        count,
        limits.max_shared_blob_refs,
        "BlueStore shared blob refs",
    )?;
    let mut refs = Vec::with_capacity(count);
    let mut position = 0i64;
    for index in 0..count {
        position = decode_position(cursor, index, position)?;
        let length = u32::try_from(read_varint_lowz_u64(
            cursor,
            "BlueStore shared blob ref length",
        )?)
        .map_err(|_| CephWireError::IntegerOverflow {
            context: "BlueStore shared blob ref length",
        })?;
        let refs_count = read_varint_u32(cursor, "BlueStore shared blob ref count")?;
        let offset = position as u64;
        let end = offset
            .checked_add(u64::from(length))
            .ok_or(CephWireError::IntegerOverflow {
                context: "BlueStore shared blob ref end",
            })?;
        validate_ref_order(&refs, offset, end, refs_count)?;
        refs.push(BlueStoreSharedBlobExtentRef {
            offset,
            length,
            refs: refs_count,
        });
    }
    Ok(refs)
}

fn decode_position(cursor: &mut CephCursor<'_>, index: usize, previous: i64) -> Result<i64> {
    let encoded = read_varint_lowz_u64(cursor, "BlueStore shared blob ref offset")?;
    let signed = i64::try_from(encoded).map_err(|_| CephWireError::IntegerOverflow {
        context: "BlueStore shared blob ref offset",
    })?;
    if index == 0 {
        return Ok(signed);
    }
    if signed == 0 {
        return Err(invalid_ref_map("offset delta must be positive"));
    }
    previous
        .checked_add(signed)
        .ok_or(CephWireError::IntegerOverflow {
            context: "BlueStore shared blob ref offset delta",
        })
}

fn validate_ref_order(
    refs: &[BlueStoreSharedBlobExtentRef],
    offset: u64,
    _end: u64,
    refs_count: u32,
) -> Result<()> {
    if let Some(previous) = refs.last() {
        let previous_end = previous
            .offset
            .checked_add(u64::from(previous.length))
            .ok_or(CephWireError::IntegerOverflow {
                context: "BlueStore shared blob previous ref end",
            })?;
        if offset < previous_end {
            return Err(invalid_ref_map("extent refs overlap"));
        }
        if offset == previous_end && refs_count == previous.refs {
            return Err(invalid_ref_map(
                "adjacent equal-reference extents are not canonical",
            ));
        }
    }
    Ok(())
}

fn invalid_ref_map(reason: &'static str) -> CephWireError {
    CephWireError::InvalidBlueStoreSemanticValue {
        context: "BlueStore shared blob ref map",
        reason,
    }
}
