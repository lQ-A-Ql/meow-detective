use std::collections::BTreeMap;

use uuid::Uuid;

use crate::{
    cursor::CephCursor,
    error::{CephWireError, Result},
};

pub(crate) const DEFAULT_MAX_STRING_LENGTH: usize = 16 * 1024 * 1024;
pub(crate) const DEFAULT_MAX_MAP_ENTRIES: usize = 1_000_000;

pub type CephStringMap = BTreeMap<String, String>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CephUtime {
    pub seconds: u32,
    pub nanoseconds: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CephStructEnvelope {
    pub version: u8,
    pub compat_version: u8,
    pub payload_length: u32,
}

pub trait CephDecode: Sized {
    fn decode(cursor: &mut CephCursor<'_>) -> Result<Self>;
}

pub trait CephEncode {
    fn encode(&self, output: &mut Vec<u8>);
}

impl CephStructEnvelope {
    pub(crate) const ENCODED_LENGTH: usize = 6;

    pub fn decode_payload<'a>(
        cursor: &mut CephCursor<'a>,
        decoder_version: u8,
    ) -> Result<(Self, CephCursor<'a>)> {
        let envelope = Self::decode(cursor)?;
        if decoder_version < envelope.compat_version {
            return Err(CephWireError::IncompatibleStructVersion {
                decoder_version,
                encoded_version: envelope.version,
                compat_version: envelope.compat_version,
            });
        }
        let payload = cursor.take(envelope.payload_length as usize)?;
        Ok((envelope, payload))
    }
}

impl CephDecode for CephStructEnvelope {
    fn decode(cursor: &mut CephCursor<'_>) -> Result<Self> {
        Ok(Self {
            version: u8::decode(cursor)?,
            compat_version: u8::decode(cursor)?,
            payload_length: u32::decode(cursor)?,
        })
    }
}

impl CephEncode for CephStructEnvelope {
    fn encode(&self, output: &mut Vec<u8>) {
        self.version.encode(output);
        self.compat_version.encode(output);
        self.payload_length.encode(output);
    }
}

impl CephDecode for u8 {
    fn decode(cursor: &mut CephCursor<'_>) -> Result<Self> {
        Ok(cursor.read_exact(1)?[0])
    }
}

impl CephEncode for u8 {
    fn encode(&self, output: &mut Vec<u8>) {
        output.push(*self);
    }
}

macro_rules! impl_le_integer {
    ($type:ty, $length:expr) => {
        impl CephDecode for $type {
            fn decode(cursor: &mut CephCursor<'_>) -> Result<Self> {
                let mut bytes = [0u8; $length];
                bytes.copy_from_slice(cursor.read_exact($length)?);
                Ok(<$type>::from_le_bytes(bytes))
            }
        }

        impl CephEncode for $type {
            fn encode(&self, output: &mut Vec<u8>) {
                output.extend_from_slice(&self.to_le_bytes());
            }
        }
    };
}

impl_le_integer!(u16, 2);
impl_le_integer!(u32, 4);
impl_le_integer!(u64, 8);
impl_le_integer!(i32, 4);
impl_le_integer!(i64, 8);

impl CephDecode for Uuid {
    fn decode(cursor: &mut CephCursor<'_>) -> Result<Self> {
        let mut bytes = [0u8; 16];
        bytes.copy_from_slice(cursor.read_exact(16)?);
        Ok(Uuid::from_bytes(bytes))
    }
}

impl CephEncode for Uuid {
    fn encode(&self, output: &mut Vec<u8>) {
        output.extend_from_slice(self.as_bytes());
    }
}

impl CephDecode for CephUtime {
    fn decode(cursor: &mut CephCursor<'_>) -> Result<Self> {
        Ok(Self {
            seconds: u32::decode(cursor)?,
            nanoseconds: u32::decode(cursor)?,
        })
    }
}

impl CephEncode for CephUtime {
    fn encode(&self, output: &mut Vec<u8>) {
        self.seconds.encode(output);
        self.nanoseconds.encode(output);
    }
}

impl CephDecode for String {
    fn decode(cursor: &mut CephCursor<'_>) -> Result<Self> {
        decode_string(cursor, DEFAULT_MAX_STRING_LENGTH, "string")
    }
}

impl CephEncode for String {
    fn encode(&self, output: &mut Vec<u8>) {
        encode_string(self, output);
    }
}

impl CephDecode for CephStringMap {
    fn decode(cursor: &mut CephCursor<'_>) -> Result<Self> {
        decode_string_map(cursor, DEFAULT_MAX_MAP_ENTRIES, DEFAULT_MAX_STRING_LENGTH)
    }
}

impl CephEncode for CephStringMap {
    fn encode(&self, output: &mut Vec<u8>) {
        (self.len() as u32).encode(output);
        for (key, value) in self {
            encode_string(key, output);
            encode_string(value, output);
        }
    }
}

pub fn decode_string(
    cursor: &mut CephCursor<'_>,
    max_length: usize,
    context: &'static str,
) -> Result<String> {
    let length = u32::decode(cursor)? as usize;
    if length > max_length {
        return Err(CephWireError::LengthLimit {
            context,
            length,
            limit: max_length,
        });
    }
    let bytes = cursor.read_exact(length)?;
    String::from_utf8(bytes.to_vec()).map_err(|error| CephWireError::InvalidUtf8 {
        context,
        message: error.to_string(),
    })
}

pub fn decode_string_map(
    cursor: &mut CephCursor<'_>,
    max_entries: usize,
    max_string_length: usize,
) -> Result<CephStringMap> {
    let count = u32::decode(cursor)? as usize;
    if count > max_entries {
        return Err(CephWireError::LengthLimit {
            context: "map entries",
            length: count,
            limit: max_entries,
        });
    }

    let mut map = BTreeMap::new();
    for _ in 0..count {
        let key = decode_string(cursor, max_string_length, "map key")?;
        let value = decode_string(cursor, max_string_length, "map value")?;
        map.insert(key, value);
    }
    Ok(map)
}

pub fn decode_varint_u64(cursor: &mut CephCursor<'_>, context: &'static str) -> Result<u64> {
    let mut value = 0u64;
    for index in 0..10 {
        let byte = u8::decode(cursor)?;
        let payload = u64::from(byte & 0x7f);
        if index == 9 && payload > 1 {
            return Err(CephWireError::IntegerOverflow { context });
        }
        value |= payload << (index * 7);
        if byte & 0x80 == 0 {
            return Ok(value);
        }
    }
    Err(CephWireError::VarintTooLong { context, limit: 10 })
}

pub fn decode_varint_lowz_u64(cursor: &mut CephCursor<'_>, context: &'static str) -> Result<u64> {
    let encoded = decode_varint_u64(cursor, context)?;
    let low_zero_nibbles = (encoded & 3) as u32;
    let value = encoded >> 2;
    let shift = low_zero_nibbles * 4;
    if value > (u64::MAX >> shift) {
        return Err(CephWireError::IntegerOverflow { context });
    }
    Ok(value << shift)
}

pub fn decode_lba_u64(cursor: &mut CephCursor<'_>, context: &'static str) -> Result<u64> {
    let word = u32::decode(cursor)?;
    let selector = word & 7;
    let (mut value, mut shift) = if selector & 1 == 0 {
        (u64::from(word & 0x7fff_fffe) << 11, 42u32)
    } else if selector == 1 || selector == 5 {
        (u64::from(word & 0x7fff_fffc) << 14, 45u32)
    } else if selector == 3 {
        (u64::from(word & 0x7fff_fff8) << 17, 48u32)
    } else {
        (u64::from(word & 0x7fff_fff8) >> 3, 28u32)
    };

    let mut byte = (word >> 24) as u8;
    while byte & 0x80 != 0 {
        byte = u8::decode(cursor)?;
        let payload = u64::from(byte & 0x7f);
        if shift >= u64::BITS || payload > (u64::MAX >> shift) {
            return Err(CephWireError::IntegerOverflow { context });
        }
        value |= payload << shift;
        shift = shift
            .checked_add(7)
            .ok_or(CephWireError::IntegerOverflow { context })?;
    }
    Ok(value)
}

pub fn encode_string(value: &str, output: &mut Vec<u8>) {
    (value.len() as u32).encode(output);
    output.extend_from_slice(value.as_bytes());
}
