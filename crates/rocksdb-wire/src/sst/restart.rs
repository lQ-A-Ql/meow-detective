use crate::cursor::WireCursor;
use crate::{Result, RocksDbWireError};

use super::SstReadOptions;

const HASH_INDEX_FLAG: u32 = 1 << 31;

pub(crate) struct RestartEntry<'a> {
    pub key: &'a [u8],
    pub value: &'a [u8],
    pub at_restart: bool,
}

pub(crate) enum RestartVisitError<E> {
    Wire(RocksDbWireError),
    Visitor(E),
}

pub(crate) enum ValueEncoding {
    Full,
}

pub(crate) fn visit_restart_block<F>(
    block: &[u8],
    encoding: ValueEncoding,
    options: SstReadOptions,
    mut visit: F,
) -> Result<usize>
where
    F: FnMut(RestartEntry<'_>) -> Result<()>,
{
    match try_visit_restart_block(block, encoding, options, &mut visit) {
        Ok(count) => Ok(count),
        Err(RestartVisitError::Wire(error) | RestartVisitError::Visitor(error)) => Err(error),
    }
}

pub(crate) fn try_visit_restart_block<F, E>(
    block: &[u8],
    encoding: ValueEncoding,
    options: SstReadOptions,
    mut visit: F,
) -> std::result::Result<usize, RestartVisitError<E>>
where
    F: FnMut(RestartEntry<'_>) -> std::result::Result<(), E>,
{
    let layout = RestartLayout::decode(block, options.max_entries_per_block)?;
    if layout.entries_end == 0 {
        return Ok(0);
    }
    let mut position = 0usize;
    let mut previous_key = Vec::new();
    let mut entry_count = 0usize;
    let mut restart_index = 0usize;
    while position < layout.entries_end {
        if entry_count >= options.max_entries_per_block {
            return Err(RestartVisitError::Wire(RocksDbWireError::SstEntryLimit {
                limit: options.max_entries_per_block,
            }));
        }
        let at_restart =
            restart_index < layout.restarts.len() && layout.restarts[restart_index] == position;
        let (entry, consumed) = decode_entry(
            &block[position..layout.entries_end],
            &previous_key,
            at_restart,
            &encoding,
            options,
        )
        .map_err(RestartVisitError::Wire)?;
        visit(RestartEntry {
            key: &entry.key,
            value: entry.value,
            at_restart,
        })
        .map_err(RestartVisitError::Visitor)?;
        previous_key = entry.key;
        position = position
            .checked_add(consumed)
            .ok_or(RestartVisitError::Wire(RocksDbWireError::LengthOverflow {
                context: "restart block entry offset",
            }))?;
        entry_count += 1;
        if at_restart {
            restart_index += 1;
        }
    }
    if position != layout.entries_end || restart_index != layout.restarts.len() {
        return Err(RestartVisitError::Wire(
            RocksDbWireError::InvalidRestartBlock {
                reason: "restart offsets do not align with decoded entries",
            },
        ));
    }
    Ok(entry_count)
}

impl<E> From<RocksDbWireError> for RestartVisitError<E> {
    fn from(error: RocksDbWireError) -> Self {
        Self::Wire(error)
    }
}

struct DecodedEntry<'a> {
    key: Vec<u8>,
    value: &'a [u8],
}

fn decode_entry<'a>(
    input: &'a [u8],
    previous_key: &[u8],
    at_restart: bool,
    encoding: &ValueEncoding,
    options: SstReadOptions,
) -> Result<(DecodedEntry<'a>, usize)> {
    let mut cursor = WireCursor::new(input);
    let shared = cursor.read_varint_u32("SST shared key length")? as usize;
    let non_shared = cursor.read_varint_u32("SST non-shared key length")? as usize;
    if shared > previous_key.len() || (at_restart && shared != 0) {
        return Err(RocksDbWireError::InvalidRestartBlock {
            reason: "invalid shared key length",
        });
    }
    let full_key_len = shared
        .checked_add(non_shared)
        .ok_or(RocksDbWireError::LengthOverflow {
            context: "SST reconstructed key length",
        })?;
    if full_key_len > options.max_key_bytes {
        return Err(RocksDbWireError::SstKeyLengthLimit {
            length: full_key_len,
            limit: options.max_key_bytes,
        });
    }
    let value_len = match encoding {
        ValueEncoding::Full => cursor.read_varint_u32("SST value length")? as usize,
    };
    if value_len > options.max_value_bytes {
        return Err(RocksDbWireError::SstValueLengthLimit {
            length: value_len,
            limit: options.max_value_bytes,
        });
    }
    let suffix = cursor.read_exact(non_shared, "SST key suffix")?;
    let value = cursor.read_exact(value_len, "SST entry value")?;
    let mut key = Vec::with_capacity(full_key_len);
    key.extend_from_slice(&previous_key[..shared]);
    key.extend_from_slice(suffix);
    Ok((DecodedEntry { key, value }, cursor.position()))
}

struct RestartLayout {
    entries_end: usize,
    restarts: Vec<usize>,
}

impl RestartLayout {
    fn decode(block: &[u8], max_restarts: usize) -> Result<Self> {
        if block.len() < 8 {
            return Err(RocksDbWireError::InvalidRestartBlock {
                reason: "block is too short for a restart array",
            });
        }
        let footer = read_fixed32(block, block.len() - 4)?;
        let hash_index = footer & HASH_INDEX_FLAG != 0;
        if hash_index {
            return Err(RocksDbWireError::UnsupportedSstFeature {
                feature: "data block hash index",
                value: 1,
            });
        }
        let restart_count = (footer & !HASH_INDEX_FLAG) as usize;
        if restart_count == 0 {
            return Err(RocksDbWireError::InvalidRestartBlock {
                reason: "restart count is zero",
            });
        }
        if restart_count > max_restarts {
            return Err(RocksDbWireError::SstEntryLimit {
                limit: max_restarts,
            });
        }
        let restart_bytes =
            restart_count
                .checked_mul(4)
                .ok_or(RocksDbWireError::LengthOverflow {
                    context: "SST restart array length",
                })?;
        let entries_end = block
            .len()
            .checked_sub(4)
            .and_then(|end| end.checked_sub(restart_bytes))
            .ok_or(RocksDbWireError::InvalidRestartBlock {
                reason: "restart array exceeds block length",
            })?;
        let mut restarts = Vec::with_capacity(restart_count);
        let mut previous = None;
        for index in 0..restart_count {
            let offset = read_fixed32(block, entries_end + index * 4)? as usize;
            let valid_empty = entries_end == 0 && restart_count == 1 && index == 0 && offset == 0;
            if (!valid_empty && offset >= entries_end)
                || previous.is_some_and(|prior| offset <= prior)
                || (index == 0 && offset != 0)
            {
                return Err(RocksDbWireError::InvalidRestartBlock {
                    reason: "restart offsets are invalid",
                });
            }
            restarts.push(offset);
            previous = Some(offset);
        }
        Ok(Self {
            entries_end,
            restarts,
        })
    }
}

fn read_fixed32(input: &[u8], offset: usize) -> Result<u32> {
    let end = offset
        .checked_add(4)
        .ok_or(RocksDbWireError::LengthOverflow {
            context: "SST fixed32 offset",
        })?;
    let bytes = input
        .get(offset..end)
        .ok_or(RocksDbWireError::InvalidRestartBlock {
            reason: "truncated restart footer",
        })?;
    Ok(u32::from_le_bytes(bytes.try_into().map_err(|_| {
        RocksDbWireError::InvalidRestartBlock {
            reason: "invalid restart fixed32",
        }
    })?))
}
