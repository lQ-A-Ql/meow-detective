use crate::{
    codec::{CephDecode, CephStructEnvelope},
    cursor::CephCursor,
    error::{CephWireError, Result},
};

pub(crate) struct DencPayload<'a> {
    pub version: u8,
    pub cursor: CephCursor<'a>,
}

pub(crate) fn read_denc_payload<'a>(
    cursor: &mut CephCursor<'a>,
    supported_versions: &'static [u8],
    context: &'static str,
) -> Result<DencPayload<'a>> {
    let envelope = CephStructEnvelope::decode(cursor)?;
    if !supported_versions.contains(&envelope.version) {
        return Err(CephWireError::UnsupportedBlueStoreDencVersion {
            context,
            encoded_version: envelope.version,
            supported_versions: supported_versions_label(supported_versions),
        });
    }
    if envelope.compat_version == 0 || envelope.compat_version > envelope.version {
        return Err(CephWireError::InvalidBlueStoreSemanticValue {
            context,
            reason: "invalid DENC compatibility version",
        });
    }
    let decoder_version = *supported_versions.last().unwrap_or(&0);
    if envelope.compat_version > decoder_version {
        return Err(CephWireError::IncompatibleStructVersion {
            decoder_version,
            encoded_version: envelope.version,
            compat_version: envelope.compat_version,
        });
    }
    Ok(DencPayload {
        version: envelope.version,
        cursor: cursor.take(envelope.payload_length as usize)?,
    })
}

pub(crate) fn read_length_prefixed<'a>(
    cursor: &mut CephCursor<'a>,
    limit: usize,
    context: &'static str,
) -> Result<&'a [u8]> {
    let length = u32::decode(cursor)? as usize;
    ensure_limit(length, limit, context)?;
    cursor.read_exact(length)
}

pub(crate) fn read_count(
    cursor: &mut CephCursor<'_>,
    limit: usize,
    context: &'static str,
) -> Result<usize> {
    let count = u32::decode(cursor)? as usize;
    ensure_limit(count, limit, context)?;
    Ok(count)
}

pub(crate) fn read_varint_u32(cursor: &mut CephCursor<'_>, context: &'static str) -> Result<u32> {
    let value = read_varint_u64(cursor, context)?;
    u32::try_from(value).map_err(|_| CephWireError::IntegerOverflow { context })
}

pub(crate) fn read_varint_u64(cursor: &mut CephCursor<'_>, context: &'static str) -> Result<u64> {
    let start = cursor.position();
    let mut value = 0u64;
    for index in 0..10 {
        let byte = u8::decode(cursor)?;
        let payload = u64::from(byte & 0x7f);
        if index == 9 && payload > 1 {
            return Err(CephWireError::IntegerOverflow { context });
        }
        value |= payload << (index * 7);
        if byte & 0x80 == 0 {
            let encoded_length = cursor.position() - start;
            if encoded_length != minimal_varint_length(value) {
                return Err(CephWireError::InvalidBlueStoreSemanticValue {
                    context,
                    reason: "non-canonical varint",
                });
            }
            return Ok(value);
        }
    }
    Err(CephWireError::VarintTooLong { context, limit: 10 })
}

pub(crate) fn read_varint_lowz_u64(
    cursor: &mut CephCursor<'_>,
    context: &'static str,
) -> Result<u64> {
    let encoded = read_varint_u64(cursor, context)?;
    let encoded_low_zero_nibbles = (encoded & 3) as u32;
    let compact = encoded >> 2;
    let shift = encoded_low_zero_nibbles * 4;
    if compact > (u64::MAX >> shift) {
        return Err(CephWireError::IntegerOverflow { context });
    }
    let value = compact << shift;
    if encoded_low_zero_nibbles != canonical_low_zero_nibbles(value) {
        return Err(CephWireError::InvalidBlueStoreSemanticValue {
            context,
            reason: "non-canonical low-zero varint",
        });
    }
    Ok(value)
}

pub(crate) fn ensure_empty(cursor: &CephCursor<'_>, context: &'static str) -> Result<()> {
    if cursor.is_empty() {
        Ok(())
    } else {
        Err(CephWireError::BlueStoreTrailingBytes {
            context,
            remaining: cursor.remaining(),
        })
    }
}

pub(crate) fn ensure_limit(length: usize, limit: usize, context: &'static str) -> Result<()> {
    if length > limit {
        Err(CephWireError::LengthLimit {
            context,
            length,
            limit,
        })
    } else {
        Ok(())
    }
}

fn minimal_varint_length(mut value: u64) -> usize {
    let mut length = 1;
    while value >= 0x80 {
        value >>= 7;
        length += 1;
    }
    length
}

fn canonical_low_zero_nibbles(value: u64) -> u32 {
    if value == 0 {
        0
    } else {
        (value.trailing_zeros() / 4).min(3)
    }
}

fn supported_versions_label(versions: &[u8]) -> &'static str {
    match versions {
        [1] => "1",
        [1, 2] => "1 or 2",
        _ => "the decoder's explicit version set",
    }
}
