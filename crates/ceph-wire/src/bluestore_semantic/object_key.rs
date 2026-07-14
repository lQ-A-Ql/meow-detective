use crate::{
    bluestore_semantic::types::{BlueStoreObjectId, BlueStoreSemanticLimits},
    cursor::CephCursor,
    error::{CephWireError, Result},
};

pub(crate) enum ObjectLogicalKey {
    Onode(BlueStoreObjectId),
    ExtentShard {
        object: BlueStoreObjectId,
        offset: u32,
    },
}

pub(crate) fn decode_object_key(
    logical_key: &[u8],
    limits: BlueStoreSemanticLimits,
) -> Result<ObjectLogicalKey> {
    match logical_key.last().copied() {
        Some(b'o') => decode_onode_key(logical_key, limits).map(ObjectLogicalKey::Onode),
        Some(b'x') => decode_extent_shard_key(logical_key, limits),
        _ => Err(invalid_key("expected onode or extent-shard suffix")),
    }
}

fn decode_extent_shard_key(
    logical_key: &[u8],
    limits: BlueStoreSemanticLimits,
) -> Result<ObjectLogicalKey> {
    if logical_key.len() < 6 {
        return Err(invalid_key("extent-shard key is truncated"));
    }
    let onode_end = logical_key.len() - 5;
    let object = decode_onode_key(&logical_key[..onode_end], limits)?;
    let offset_bytes: [u8; 4] = logical_key[onode_end..onode_end + 4]
        .try_into()
        .map_err(|_| invalid_key("extent-shard offset is truncated"))?;
    Ok(ObjectLogicalKey::ExtentShard {
        object,
        offset: u32::from_be_bytes(offset_bytes),
    })
}

fn decode_onode_key(
    logical_key: &[u8],
    limits: BlueStoreSemanticLimits,
) -> Result<BlueStoreObjectId> {
    let mut cursor = CephCursor::new(logical_key);
    let shard = read_u8(&mut cursor)?.wrapping_sub(0x80) as i8;
    if shard < -1 {
        return Err(invalid_key("shard id is outside the canonical range"));
    }
    let encoded_pool = read_be_u64(&mut cursor)?;
    let bitwise_hash = read_be_u32(&mut cursor)?;
    let namespace = read_escaped(&mut cursor, limits.max_string_bytes)?;
    let key_or_name = read_escaped(&mut cursor, limits.max_string_bytes)?;
    let discriminator = read_u8(&mut cursor)?;
    let (object_key, object_name) = decode_name_pair(
        &mut cursor,
        key_or_name,
        discriminator,
        limits.max_string_bytes,
    )?;
    let snap = read_be_u64(&mut cursor)?;
    let generation = read_be_u64(&mut cursor)?;
    if read_u8(&mut cursor)? != b'o' {
        return Err(invalid_key("onode suffix is invalid"));
    }
    if !cursor.is_empty() {
        return Err(invalid_key("bytes follow the onode suffix"));
    }
    Ok(BlueStoreObjectId {
        shard,
        pool: encoded_pool.wrapping_sub(1u64 << 63) as i64,
        hash: bitwise_hash.reverse_bits(),
        bitwise_hash,
        namespace,
        object_key,
        object_name,
        snap,
        generation,
    })
}

fn decode_name_pair(
    cursor: &mut CephCursor<'_>,
    key_or_name: Vec<u8>,
    discriminator: u8,
    max_string_bytes: usize,
) -> Result<(Option<Vec<u8>>, Vec<u8>)> {
    match discriminator {
        b'=' => Ok((None, key_or_name)),
        b'<' | b'>' => {
            let object_name = read_escaped(cursor, max_string_bytes)?;
            let ordering_matches = if discriminator == b'<' {
                key_or_name < object_name
            } else {
                key_or_name > object_name
            };
            if !ordering_matches {
                return Err(invalid_key(
                    "object-key ordering discriminator is non-canonical",
                ));
            }
            Ok((Some(key_or_name), object_name))
        }
        _ => Err(invalid_key("object-key discriminator is invalid")),
    }
}

fn read_escaped(cursor: &mut CephCursor<'_>, max_length: usize) -> Result<Vec<u8>> {
    let mut decoded = Vec::new();
    loop {
        let byte = read_u8(cursor)?;
        match byte {
            b'!' => return Ok(decoded),
            0 => return Err(invalid_key("raw NUL appears in an escaped string")),
            marker @ (b'#' | b'~') => {
                let high = decode_hex(read_u8(cursor)?)?;
                let low = decode_hex(read_u8(cursor)?)?;
                let value = (high << 4) | low;
                validate_escape_marker(marker, value)?;
                push_decoded(&mut decoded, value, max_length)?;
            }
            value if value <= b'#' || value >= b'~' => {
                return Err(invalid_key(
                    "escaped string contains a non-canonical raw byte",
                ));
            }
            value => push_decoded(&mut decoded, value, max_length)?,
        }
    }
}

fn validate_escape_marker(marker: u8, value: u8) -> Result<()> {
    // BlueStore preserves a historical signed-char comparison bug: bytes
    // 0x80..=0xff use '#', while only 0x7e..=0x7f use '~'.
    let expected = if value <= b'#' || value >= 0x80 {
        Some(b'#')
    } else if value >= b'~' {
        Some(b'~')
    } else {
        None
    };
    if expected == Some(marker) {
        Ok(())
    } else {
        Err(invalid_key("escaped string uses a non-canonical marker"))
    }
}

fn push_decoded(decoded: &mut Vec<u8>, value: u8, max_length: usize) -> Result<()> {
    if decoded.len() >= max_length {
        return Err(CephWireError::LengthLimit {
            context: "BlueStore escaped string",
            length: decoded.len() + 1,
            limit: max_length,
        });
    }
    decoded.push(value);
    Ok(())
}

fn decode_hex(value: u8) -> Result<u8> {
    match value {
        b'0'..=b'9' => Ok(value - b'0'),
        b'a'..=b'f' => Ok(value - b'a' + 10),
        _ => Err(invalid_key("escaped string contains invalid hex")),
    }
}

fn read_u8(cursor: &mut CephCursor<'_>) -> Result<u8> {
    Ok(cursor.read_exact(1)?[0])
}

fn read_be_u32(cursor: &mut CephCursor<'_>) -> Result<u32> {
    let bytes: [u8; 4] = cursor
        .read_exact(4)?
        .try_into()
        .map_err(|_| invalid_key("sortable u32 is truncated"))?;
    Ok(u32::from_be_bytes(bytes))
}

fn read_be_u64(cursor: &mut CephCursor<'_>) -> Result<u64> {
    let bytes: [u8; 8] = cursor
        .read_exact(8)?
        .try_into()
        .map_err(|_| invalid_key("sortable u64 is truncated"))?;
    Ok(u64::from_be_bytes(bytes))
}

fn invalid_key(reason: &'static str) -> CephWireError {
    CephWireError::InvalidBlueStoreSemanticKey {
        key_space: "object",
        reason,
    }
}
