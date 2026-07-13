use uuid::Uuid;

use crate::{
    bluefs::{decode_extent, decode_fnode, BluefsExtent, BluefsFnode, BLUEFS_MAX_EXTENTS},
    codec::{decode_string, decode_varint_u64, CephDecode, CephStructEnvelope, CephUtime},
    crc32c::ceph_crc32c,
    cursor::CephCursor,
    error::{CephWireError, Result},
};

pub const BLUEFS_MAX_OPERATION_BYTES: usize = 16 * 1024 * 1024;
pub const BLUEFS_MAX_OPERATIONS: usize = 262_144;
const BLUEFS_TRANSACTION_VERSION: u8 = 1;
const BLUEFS_FNODE_DELTA_VERSION: u8 = 2;
const BLUEFS_MAX_NAME_BYTES: usize = 4096;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BluefsTransactionPrefix {
    pub uuid: Uuid,
    pub sequence: u64,
    pub encoded_length: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BluefsTransaction {
    pub uuid: Uuid,
    pub sequence: u64,
    pub operations: Vec<BluefsOperation>,
    pub operation_crc32c: u32,
    pub encoded_length: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BluefsOperation {
    Init,
    AllocAdd {
        device_id: u8,
        offset: u64,
        length: u64,
    },
    AllocRemove {
        device_id: u8,
        offset: u64,
        length: u64,
    },
    DirectoryLink {
        directory: String,
        file_name: String,
        inode: u64,
    },
    DirectoryUnlink {
        directory: String,
        file_name: String,
    },
    DirectoryCreate {
        directory: String,
    },
    DirectoryRemove {
        directory: String,
    },
    FileUpdate {
        fnode: BluefsFnode,
    },
    FileRemove {
        inode: u64,
    },
    Jump {
        next_sequence: u64,
        offset: u64,
    },
    JumpSequence {
        next_sequence: u64,
    },
    FileUpdateIncremental {
        delta: BluefsFnodeDelta,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BluefsFnodeDelta {
    pub inode: u64,
    pub size: u64,
    pub mtime: CephUtime,
    pub offset: u64,
    pub extents: Vec<BluefsExtent>,
    pub encoding: u8,
    pub content_size: u64,
    pub struct_version: u8,
    pub struct_compat_version: u8,
}

pub fn inspect_bluefs_transaction(bytes: &[u8]) -> Result<BluefsTransactionPrefix> {
    let mut cursor = CephCursor::new(bytes);
    let envelope = CephStructEnvelope::decode(&mut cursor)?;
    if BLUEFS_TRANSACTION_VERSION < envelope.compat_version {
        return Err(CephWireError::IncompatibleStructVersion {
            decoder_version: BLUEFS_TRANSACTION_VERSION,
            encoded_version: envelope.version,
            compat_version: envelope.compat_version,
        });
    }
    let encoded_length = CephStructEnvelope::ENCODED_LENGTH
        .checked_add(envelope.payload_length as usize)
        .ok_or(CephWireError::LengthOverflow {
            context: "BlueFS transaction",
        })?;
    let uuid = Uuid::decode(&mut cursor)?;
    let sequence = u64::decode(&mut cursor)?;
    let operation_length = u32::decode(&mut cursor)? as usize;
    if operation_length > BLUEFS_MAX_OPERATION_BYTES {
        return Err(CephWireError::BluefsTransactionLengthLimit {
            length: operation_length,
            limit: BLUEFS_MAX_OPERATION_BYTES,
        });
    }
    let payload_prefix_length = cursor
        .position()
        .checked_sub(CephStructEnvelope::ENCODED_LENGTH)
        .ok_or(CephWireError::LengthOverflow {
            context: "BlueFS transaction payload prefix",
        })?;
    let minimum_payload_length = payload_prefix_length
        .checked_add(operation_length)
        .and_then(|length| length.checked_add(std::mem::size_of::<u32>()))
        .ok_or(CephWireError::LengthOverflow {
            context: "BlueFS transaction payload",
        })?;
    if minimum_payload_length > envelope.payload_length as usize {
        return Err(CephWireError::BluefsTransactionPayloadLengthMismatch {
            payload_length: envelope.payload_length as usize,
            minimum_length: minimum_payload_length,
        });
    }
    Ok(BluefsTransactionPrefix {
        uuid,
        sequence,
        encoded_length,
    })
}

pub fn decode_bluefs_transaction(bytes: &[u8]) -> Result<BluefsTransaction> {
    let mut cursor = CephCursor::new(bytes);
    let (envelope, mut payload) =
        CephStructEnvelope::decode_payload(&mut cursor, BLUEFS_TRANSACTION_VERSION)?;
    let uuid = Uuid::decode(&mut payload)?;
    let sequence = u64::decode(&mut payload)?;
    let operation_length = u32::decode(&mut payload)? as usize;
    if operation_length > BLUEFS_MAX_OPERATION_BYTES {
        return Err(CephWireError::BluefsTransactionLengthLimit {
            length: operation_length,
            limit: BLUEFS_MAX_OPERATION_BYTES,
        });
    }
    let operation_bytes = payload.read_exact(operation_length)?;
    let expected_crc = u32::decode(&mut payload)?;
    let actual_crc = ceph_crc32c(operation_bytes);
    if expected_crc != actual_crc {
        return Err(CephWireError::BluefsTransactionCrcMismatch {
            expected: expected_crc,
            actual: actual_crc,
        });
    }
    if !payload.is_empty() {
        payload.skip(payload.remaining())?;
    }
    let operations = decode_operations(operation_bytes)?;
    Ok(BluefsTransaction {
        uuid,
        sequence,
        operations,
        operation_crc32c: expected_crc,
        encoded_length: CephStructEnvelope::ENCODED_LENGTH + envelope.payload_length as usize,
    })
}

fn decode_operations(bytes: &[u8]) -> Result<Vec<BluefsOperation>> {
    let mut cursor = CephCursor::new(bytes);
    let mut operations = Vec::new();
    while !cursor.is_empty() {
        if operations.len() >= BLUEFS_MAX_OPERATIONS {
            return Err(CephWireError::LengthLimit {
                context: "BlueFS transaction operations",
                length: operations.len() + 1,
                limit: BLUEFS_MAX_OPERATIONS,
            });
        }
        operations.push(decode_operation(&mut cursor)?);
    }
    Ok(operations)
}

fn decode_operation(cursor: &mut CephCursor<'_>) -> Result<BluefsOperation> {
    match u8::decode(cursor)? {
        1 => Ok(BluefsOperation::Init),
        2 => Ok(BluefsOperation::AllocAdd {
            device_id: u8::decode(cursor)?,
            offset: u64::decode(cursor)?,
            length: u64::decode(cursor)?,
        }),
        3 => Ok(BluefsOperation::AllocRemove {
            device_id: u8::decode(cursor)?,
            offset: u64::decode(cursor)?,
            length: u64::decode(cursor)?,
        }),
        4 => Ok(BluefsOperation::DirectoryLink {
            directory: decode_name(cursor, "BlueFS directory name")?,
            file_name: decode_name(cursor, "BlueFS file name")?,
            inode: u64::decode(cursor)?,
        }),
        5 => Ok(BluefsOperation::DirectoryUnlink {
            directory: decode_name(cursor, "BlueFS directory name")?,
            file_name: decode_name(cursor, "BlueFS file name")?,
        }),
        6 => Ok(BluefsOperation::DirectoryCreate {
            directory: decode_name(cursor, "BlueFS directory name")?,
        }),
        7 => Ok(BluefsOperation::DirectoryRemove {
            directory: decode_name(cursor, "BlueFS directory name")?,
        }),
        8 => Ok(BluefsOperation::FileUpdate {
            fnode: decode_fnode(cursor)?,
        }),
        9 => Ok(BluefsOperation::FileRemove {
            inode: u64::decode(cursor)?,
        }),
        10 => Ok(BluefsOperation::Jump {
            next_sequence: u64::decode(cursor)?,
            offset: u64::decode(cursor)?,
        }),
        11 => Ok(BluefsOperation::JumpSequence {
            next_sequence: u64::decode(cursor)?,
        }),
        12 => Ok(BluefsOperation::FileUpdateIncremental {
            delta: decode_fnode_delta(cursor)?,
        }),
        opcode => Err(CephWireError::UnknownBluefsOperation { opcode }),
    }
}

fn decode_fnode_delta(cursor: &mut CephCursor<'_>) -> Result<BluefsFnodeDelta> {
    let (envelope, mut payload) =
        CephStructEnvelope::decode_payload(cursor, BLUEFS_FNODE_DELTA_VERSION)?;
    let inode = decode_varint_u64(&mut payload, "BlueFS fnode delta inode")?;
    let size = decode_varint_u64(&mut payload, "BlueFS fnode delta size")?;
    let mtime = CephUtime::decode(&mut payload)?;
    let offset = u64::decode(&mut payload)?;
    let count = u32::decode(&mut payload)? as usize;
    if count > BLUEFS_MAX_EXTENTS {
        return Err(CephWireError::LengthLimit {
            context: "BlueFS fnode delta extents",
            length: count,
            limit: BLUEFS_MAX_EXTENTS,
        });
    }
    let mut extents = Vec::with_capacity(count);
    for _ in 0..count {
        extents.push(decode_extent(&mut payload)?);
    }
    let (encoding, content_size) = if envelope.version >= 2 {
        (
            decode_varint_u64(&mut payload, "BlueFS fnode delta encoding")?,
            decode_varint_u64(&mut payload, "BlueFS fnode delta content size")?,
        )
    } else {
        (0, 0)
    };
    if encoding >= 3 {
        return Err(CephWireError::InvalidBluefsFnodeEncoding { encoding });
    }
    if !payload.is_empty() {
        payload.skip(payload.remaining())?;
    }
    Ok(BluefsFnodeDelta {
        inode,
        size,
        mtime,
        offset,
        extents,
        encoding: encoding as u8,
        content_size,
        struct_version: envelope.version,
        struct_compat_version: envelope.compat_version,
    })
}

fn decode_name(cursor: &mut CephCursor<'_>, context: &'static str) -> Result<String> {
    decode_string(cursor, BLUEFS_MAX_NAME_BYTES, context)
}
