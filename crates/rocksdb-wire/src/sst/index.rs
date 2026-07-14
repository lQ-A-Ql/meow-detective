use crate::cursor::WireCursor;
use crate::{Result, RocksDbWireError};

use super::{BlockHandle, IndexKeyKind, IndexKeyMetadata, SstReadOptions};

const HASH_INDEX_FLAG: u32 = 1 << 31;

pub(crate) struct IndexEntry {
    pub handle: BlockHandle,
    pub key: IndexKeyMetadata,
}

pub(crate) fn parse_index(
    block: &[u8],
    data_boundary: u64,
    key_kind: IndexKeyKind,
    options: SstReadOptions,
) -> Result<Vec<IndexEntry>> {
    let layout = IndexLayout::decode(block, options.max_entries_per_block)?;
    if layout.hash_index {
        return Err(RocksDbWireError::UnsupportedSstFeature {
            feature: "index block hash index",
            value: 1,
        });
    }
    let mut entries = Vec::new();
    let mut previous_key = Vec::new();
    let mut previous_handle = None;
    let mut position = 0usize;
    let mut restart_index = 0usize;
    let mut first_handle = true;
    while position < layout.entries_end {
        if entries.len() >= options.max_entries_per_block {
            return Err(RocksDbWireError::SstEntryLimit {
                limit: options.max_entries_per_block,
            });
        }
        let at_restart =
            restart_index < layout.restarts.len() && layout.restarts[restart_index] == position;
        let decoded = decode_index_entry(
            &block[position..layout.entries_end],
            &previous_key,
            previous_handle,
            at_restart,
            key_kind,
            options,
        )?;
        decoded.handle.validate_before(data_boundary)?;
        if first_handle && decoded.handle.offset != 0 {
            return Err(RocksDbWireError::InvalidSstIndex {
                reason: "first data block does not begin at file offset zero",
            });
        }
        if let Some(previous) = previous_handle {
            let expected_offset = previous.serialized_end()?;
            if decoded.handle.offset != expected_offset {
                return Err(RocksDbWireError::InvalidSstIndex {
                    reason: "delta index handle is not physically consecutive",
                });
            }
        }
        previous_key = decoded.key;
        previous_handle = Some(decoded.handle);
        position =
            position
                .checked_add(decoded.consumed)
                .ok_or(RocksDbWireError::LengthOverflow {
                    context: "SST index entry offset",
                })?;
        entries.push(IndexEntry {
            handle: decoded.handle,
            key: decoded.key_metadata,
        });
        first_handle = false;
        if at_restart {
            restart_index += 1;
        }
    }
    if position != layout.entries_end || restart_index != layout.restarts.len() {
        return Err(RocksDbWireError::InvalidSstIndex {
            reason: "index restart offsets do not align with decoded entries",
        });
    }
    if previous_handle.is_none_or(|handle| handle.serialized_end().ok() != Some(data_boundary)) {
        return Err(RocksDbWireError::InvalidSstIndex {
            reason: "data block ranges do not end at the metadata boundary",
        });
    }
    Ok(entries)
}

struct DecodedIndexEntry {
    key: Vec<u8>,
    key_metadata: IndexKeyMetadata,
    handle: BlockHandle,
    consumed: usize,
}

fn decode_index_entry(
    input: &[u8],
    previous_key: &[u8],
    previous_handle: Option<BlockHandle>,
    at_restart: bool,
    key_kind: IndexKeyKind,
    options: SstReadOptions,
) -> Result<DecodedIndexEntry> {
    let mut cursor = WireCursor::new(input);
    let shared = cursor.read_varint_u32("SST index shared key length")? as usize;
    let non_shared = cursor.read_varint_u32("SST index non-shared key length")? as usize;
    if shared > previous_key.len() || (at_restart && shared != 0) {
        return Err(RocksDbWireError::InvalidSstIndex {
            reason: "invalid index shared key length",
        });
    }
    let key_len = shared
        .checked_add(non_shared)
        .ok_or(RocksDbWireError::LengthOverflow {
            context: "SST index key length",
        })?;
    if key_len > options.max_key_bytes {
        return Err(RocksDbWireError::SstKeyLengthLimit {
            length: key_len,
            limit: options.max_key_bytes,
        });
    }
    let suffix = cursor.read_exact(non_shared, "SST index key suffix")?;
    let handle = match (shared, previous_handle) {
        (0, _) => BlockHandle::decode(&mut cursor, "SST full index handle")?,
        (_, Some(previous)) => decode_delta_handle(&mut cursor, previous)?,
        (_, None) => {
            return Err(RocksDbWireError::InvalidSstIndex {
                reason: "first index entry is delta encoded",
            });
        }
    };
    let mut key = Vec::with_capacity(key_len);
    key.extend_from_slice(&previous_key[..shared]);
    key.extend_from_slice(suffix);
    let key_metadata = decode_index_key(&key, key_kind)?;
    if !previous_key.is_empty() && !index_key_is_strictly_after(previous_key, &key, key_kind)? {
        return Err(RocksDbWireError::InvalidSstIndex {
            reason: "index keys are not strictly ordered",
        });
    }
    Ok(DecodedIndexEntry {
        key,
        key_metadata,
        handle,
        consumed: cursor.position(),
    })
}

fn decode_delta_handle(cursor: &mut WireCursor<'_>, previous: BlockHandle) -> Result<BlockHandle> {
    let zigzag = cursor.read_varint_u64("SST index handle size delta")?;
    let delta = ((zigzag >> 1) as i64) ^ -((zigzag & 1) as i64);
    let size = if delta >= 0 {
        previous.size.checked_add(delta as u64)
    } else {
        previous.size.checked_sub(delta.unsigned_abs())
    }
    .ok_or(RocksDbWireError::InvalidSstIndex {
        reason: "delta index handle size overflows",
    })?;
    if size == 0 {
        return Err(RocksDbWireError::InvalidSstIndex {
            reason: "delta index handle has zero size",
        });
    }
    Ok(BlockHandle {
        offset: previous.serialized_end()?,
        size,
    })
}

fn decode_index_key(key: &[u8], kind: IndexKeyKind) -> Result<IndexKeyMetadata> {
    match kind {
        IndexKeyKind::User => Ok(IndexKeyMetadata {
            key_length: index_key_length(key)?,
            kind,
            sequence: None,
            value_type: None,
            xxh3_digest: xxhash_rust::xxh3::xxh3_64(key),
        }),
        IndexKeyKind::Internal => decode_internal_index_key(key),
    }
}

fn decode_internal_index_key(key: &[u8]) -> Result<IndexKeyMetadata> {
    if key.len() < 8 {
        return Err(RocksDbWireError::InternalKeyTooShort {
            context: "SST index separator",
            length: key.len(),
        });
    }
    let trailer = u64::from_le_bytes(key[key.len() - 8..].try_into().map_err(|_| {
        RocksDbWireError::InvalidSstIndex {
            reason: "index key trailer has invalid width",
        }
    })?);
    let value_type = trailer as u8;
    if !matches!(
        value_type,
        0x00 | 0x01 | 0x02 | 0x07 | 0x0f | 0x11 | 0x14 | 0x16
    ) {
        return Err(RocksDbWireError::UnsupportedSstEntryType { value_type });
    }
    Ok(IndexKeyMetadata {
        key_length: index_key_length(key)?,
        kind: IndexKeyKind::Internal,
        sequence: Some(trailer >> 8),
        value_type: Some(value_type),
        xxh3_digest: xxhash_rust::xxh3::xxh3_64(key),
    })
}

fn index_key_length(key: &[u8]) -> Result<u32> {
    u32::try_from(key.len()).map_err(|_| RocksDbWireError::LengthOverflow {
        context: "SST index key length",
    })
}

fn index_key_is_strictly_after(
    previous: &[u8],
    current: &[u8],
    kind: IndexKeyKind,
) -> Result<bool> {
    match kind {
        IndexKeyKind::User => Ok(previous < current),
        IndexKeyKind::Internal => internal_key_is_strictly_after(previous, current),
    }
}

fn internal_key_is_strictly_after(previous: &[u8], current: &[u8]) -> Result<bool> {
    let previous_user = &previous[..previous.len() - 8];
    let current_user = &current[..current.len() - 8];
    match previous_user.cmp(current_user) {
        std::cmp::Ordering::Less => Ok(true),
        std::cmp::Ordering::Greater => Ok(false),
        std::cmp::Ordering::Equal => {
            let previous_trailer = fixed64_tail(previous)?;
            let current_trailer = fixed64_tail(current)?;
            Ok(previous_trailer > current_trailer)
        }
    }
}

fn fixed64_tail(key: &[u8]) -> Result<u64> {
    Ok(u64::from_le_bytes(
        key[key.len() - 8..]
            .try_into()
            .map_err(|_| RocksDbWireError::InvalidSstIndex {
                reason: "index key trailer has invalid width",
            })?,
    ))
}

struct IndexLayout {
    entries_end: usize,
    restarts: Vec<usize>,
    hash_index: bool,
}

impl IndexLayout {
    fn decode(block: &[u8], max_restarts: usize) -> Result<Self> {
        if block.len() < 8 {
            return Err(RocksDbWireError::InvalidSstIndex {
                reason: "index block is too short",
            });
        }
        let footer = fixed32(block, block.len() - 4)?;
        let hash_index = footer & HASH_INDEX_FLAG != 0;
        let count = (footer & !HASH_INDEX_FLAG) as usize;
        if count == 0 {
            return Err(RocksDbWireError::InvalidSstIndex {
                reason: "index restart count is zero",
            });
        }
        if count > max_restarts {
            return Err(RocksDbWireError::SstEntryLimit {
                limit: max_restarts,
            });
        }
        let restart_bytes = count
            .checked_mul(4)
            .ok_or(RocksDbWireError::LengthOverflow {
                context: "SST index restart bytes",
            })?;
        let entries_end = block.len().checked_sub(4 + restart_bytes).ok_or(
            RocksDbWireError::InvalidSstIndex {
                reason: "index restart array exceeds block",
            },
        )?;
        let mut restarts = Vec::with_capacity(count);
        let mut previous = None;
        for index in 0..count {
            let offset = fixed32(block, entries_end + index * 4)? as usize;
            if offset >= entries_end
                || previous.is_some_and(|prior| offset <= prior)
                || (index == 0 && offset != 0)
            {
                return Err(RocksDbWireError::InvalidSstIndex {
                    reason: "index restart offsets are invalid",
                });
            }
            restarts.push(offset);
            previous = Some(offset);
        }
        Ok(Self {
            entries_end,
            restarts,
            hash_index,
        })
    }
}

fn fixed32(input: &[u8], offset: usize) -> Result<u32> {
    let bytes = input
        .get(offset..offset + 4)
        .ok_or(RocksDbWireError::InvalidSstIndex {
            reason: "truncated index fixed32",
        })?;
    Ok(u32::from_le_bytes(bytes.try_into().map_err(|_| {
        RocksDbWireError::InvalidSstIndex {
            reason: "invalid index fixed32",
        }
    })?))
}
