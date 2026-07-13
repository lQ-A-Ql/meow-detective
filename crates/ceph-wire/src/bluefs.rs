use uuid::Uuid;

use crate::{
    codec::{
        decode_lba_u64, decode_varint_lowz_u64, decode_varint_u64, CephDecode, CephStructEnvelope,
        CephUtime,
    },
    crc32c::ceph_crc32c,
    cursor::CephCursor,
    error::{CephWireError, Result},
};

pub const BLUEFS_SUPER_OFFSET: u64 = 4096;
pub const BLUEFS_SUPER_BLOCK_SIZE: usize = 4096;
pub const BLUEFS_MAX_EXTENTS: usize = 65_536;

const BLUEFS_SUPER_VERSION: u8 = 3;
const BLUEFS_FNODE_VERSION: u8 = 2;
const BLUEFS_EXTENT_VERSION: u8 = 1;
const BLUEFS_LAYOUT_VERSION: u8 = 1;
const BLUEFS_FNODE_ENCODING_MAX: u64 = 3;
const BLUEFS_MIN_EXTENT_ENCODED_LENGTH: usize = CephStructEnvelope::ENCODED_LENGTH + 6;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BluefsSuper {
    pub uuid: Uuid,
    pub osd_uuid: Uuid,
    pub seq: u64,
    pub block_size: u32,
    pub log_fnode: BluefsFnode,
    pub memorized_layout: Option<BluefsLayout>,
    pub crc32c: u32,
    pub struct_version: u8,
    pub struct_compat_version: u8,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BluefsFnode {
    pub ino: u64,
    pub size: u64,
    pub mtime: CephUtime,
    pub extents: Vec<BluefsExtent>,
    pub encoding: u8,
    pub content_size: u64,
    pub struct_version: u8,
    pub struct_compat_version: u8,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BluefsExtent {
    pub offset: u64,
    pub length: u32,
    pub bdev: u8,
    pub struct_version: u8,
    pub struct_compat_version: u8,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BluefsLayout {
    pub shared_bdev: u32,
    pub dedicated_db: bool,
    pub dedicated_wal: bool,
    pub struct_version: u8,
    pub struct_compat_version: u8,
}

pub fn decode_bluefs_super_block(block: &[u8]) -> Result<BluefsSuper> {
    if block.len() != BLUEFS_SUPER_BLOCK_SIZE {
        return Err(CephWireError::InvalidBluefsSuperblockSize {
            expected: BLUEFS_SUPER_BLOCK_SIZE,
            actual: block.len(),
        });
    }

    let mut cursor = CephCursor::new(block);
    let (envelope, mut payload) =
        CephStructEnvelope::decode_payload(&mut cursor, BLUEFS_SUPER_VERSION)?;
    let uuid = Uuid::decode(&mut payload)?;
    let osd_uuid = Uuid::decode(&mut payload)?;
    let seq = u64::decode(&mut payload)?;
    let block_size = u32::decode(&mut payload)?;
    if !block_size.is_power_of_two() {
        return Err(CephWireError::InvalidBluefsBlockSize { block_size });
    }
    let log_fnode = decode_fnode(&mut payload)?;
    let memorized_layout = if envelope.version >= 2 {
        decode_optional_layout(&mut payload)?
    } else {
        None
    };
    finish_payload(&mut payload, envelope.payload_length as usize)?;

    let crc_offset = cursor.position();
    let expected_crc = u32::decode(&mut cursor)?;
    let actual_crc = ceph_crc32c(&block[..crc_offset]);
    if expected_crc != actual_crc {
        return Err(CephWireError::BluefsCrcMismatch {
            expected: expected_crc,
            actual: actual_crc,
        });
    }

    Ok(BluefsSuper {
        uuid,
        osd_uuid,
        seq,
        block_size,
        log_fnode,
        memorized_layout,
        crc32c: expected_crc,
        struct_version: envelope.version,
        struct_compat_version: envelope.compat_version,
    })
}

pub(crate) fn decode_fnode(cursor: &mut CephCursor<'_>) -> Result<BluefsFnode> {
    let (envelope, mut payload) = CephStructEnvelope::decode_payload(cursor, BLUEFS_FNODE_VERSION)?;
    let ino = decode_varint_u64(&mut payload, "BlueFS fnode ino")?;
    let size = decode_varint_u64(&mut payload, "BlueFS fnode size")?;
    let mtime = CephUtime::decode(&mut payload)?;
    u8::decode(&mut payload)?;

    let extent_count = u32::decode(&mut payload)? as usize;
    let payload_limit = payload.remaining() / BLUEFS_MIN_EXTENT_ENCODED_LENGTH;
    let extent_limit = BLUEFS_MAX_EXTENTS.min(payload_limit);
    if extent_count > extent_limit {
        return Err(CephWireError::LengthLimit {
            context: "BlueFS fnode extents",
            length: extent_count,
            limit: extent_limit,
        });
    }
    let mut extents = Vec::with_capacity(extent_count);
    for _ in 0..extent_count {
        extents.push(decode_extent(&mut payload)?);
    }

    let (encoding, content_size) = if envelope.version >= 2 {
        (
            decode_varint_u64(&mut payload, "BlueFS fnode encoding")?,
            decode_varint_u64(&mut payload, "BlueFS fnode content size")?,
        )
    } else {
        (0, 0)
    };
    if encoding >= BLUEFS_FNODE_ENCODING_MAX {
        return Err(CephWireError::InvalidBluefsFnodeEncoding { encoding });
    }
    finish_payload(&mut payload, envelope.payload_length as usize)?;

    Ok(BluefsFnode {
        ino,
        size,
        mtime,
        extents,
        encoding: encoding as u8,
        content_size,
        struct_version: envelope.version,
        struct_compat_version: envelope.compat_version,
    })
}

pub(crate) fn decode_extent(cursor: &mut CephCursor<'_>) -> Result<BluefsExtent> {
    let (envelope, mut payload) =
        CephStructEnvelope::decode_payload(cursor, BLUEFS_EXTENT_VERSION)?;
    let offset = decode_lba_u64(&mut payload, "BlueFS extent offset")?;
    let encoded_length = decode_varint_lowz_u64(&mut payload, "BlueFS extent length")?;
    let length =
        u32::try_from(encoded_length).map_err(|_| CephWireError::InvalidBluefsExtentLength {
            length: encoded_length,
        })?;
    if length == 0 {
        return Err(CephWireError::InvalidBluefsExtentLength {
            length: encoded_length,
        });
    }
    let bdev = u8::decode(&mut payload)?;
    finish_payload(&mut payload, envelope.payload_length as usize)?;

    Ok(BluefsExtent {
        offset,
        length,
        bdev,
        struct_version: envelope.version,
        struct_compat_version: envelope.compat_version,
    })
}

fn decode_optional_layout(cursor: &mut CephCursor<'_>) -> Result<Option<BluefsLayout>> {
    match u8::decode(cursor)? {
        0 => return Ok(None),
        1 => {}
        value => {
            return Err(CephWireError::InvalidBluefsBoolean {
                context: "optional layout presence",
                value,
            });
        }
    }

    let (envelope, mut payload) =
        CephStructEnvelope::decode_payload(cursor, BLUEFS_LAYOUT_VERSION)?;
    let shared_bdev = u32::decode(&mut payload)?;
    let dedicated_db = decode_bool(&mut payload, "dedicated DB")?;
    let dedicated_wal = decode_bool(&mut payload, "dedicated WAL")?;
    finish_payload(&mut payload, envelope.payload_length as usize)?;
    Ok(Some(BluefsLayout {
        shared_bdev,
        dedicated_db,
        dedicated_wal,
        struct_version: envelope.version,
        struct_compat_version: envelope.compat_version,
    }))
}

fn decode_bool(cursor: &mut CephCursor<'_>, context: &'static str) -> Result<bool> {
    match u8::decode(cursor)? {
        0 => Ok(false),
        1 => Ok(true),
        value => Err(CephWireError::InvalidBluefsBoolean { context, value }),
    }
}

fn finish_payload(cursor: &mut CephCursor<'_>, payload_length: usize) -> Result<()> {
    if cursor.position() > payload_length {
        return Err(CephWireError::StructBoundaryExceeded {
            struct_end: payload_length,
            offset: cursor.position(),
        });
    }
    if !cursor.is_empty() {
        cursor.skip(cursor.remaining())?;
    }
    Ok(())
}
