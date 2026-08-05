use crate::io::{read_exact_at, read_u32, SharedReader};
use crate::{ErofsError, Result};

use super::{head_kind, IndexEntry};

pub(crate) struct CompactIndexLayout {
    offset: u64,
    pub(super) entries: u64,
    initial_4b: u64,
    compact_2b: u64,
}

struct CompactPackLocation {
    offset: u64,
    bytes: usize,
    entries: usize,
    entry_index: usize,
    encoded_bits: u32,
}

impl CompactIndexLayout {
    pub(super) fn new(
        header_offset: u64,
        advise: u16,
        size: u64,
        superblock: &crate::ErofsSuperblock,
        volume_offset: u64,
    ) -> Result<Self> {
        let offset = header_offset
            .checked_add(super::MAP_HEADER_BYTES)
            .ok_or_else(|| ErofsError::Invalid("compact index offset overflows".to_string()))?;
        let entries = size.div_ceil(superblock.block_size as u64);
        let alignment_entries = ((32 - offset % 32) / 4) & 7;
        let initial_4b = alignment_entries.min(entries);
        let compact_2b =
            if advise & super::MAP_ADVISE_COMPACTED_2B != 0 && alignment_entries < entries {
                (entries - alignment_entries) / super::COMPACT_PACK_2B_ENTRIES
                    * super::COMPACT_PACK_2B_ENTRIES
            } else {
                0
            };
        let trailing_4b = entries
            .checked_sub(initial_4b)
            .and_then(|remaining| remaining.checked_sub(compact_2b))
            .ok_or_else(|| ErofsError::Invalid("compact index counts underflow".to_string()))?;
        let initial_bytes = packed_bytes(
            initial_4b,
            super::COMPACT_PACK_4B_ENTRIES,
            super::COMPACT_PACK_4B_BYTES,
        )?;
        let middle_bytes = compact_2b.checked_mul(2).ok_or_else(|| {
            ErofsError::Invalid("compact 2-byte index size overflows".to_string())
        })?;
        let trailing_bytes = packed_bytes(
            trailing_4b,
            super::COMPACT_PACK_4B_ENTRIES,
            super::COMPACT_PACK_4B_BYTES,
        )?;
        let index_bytes = initial_bytes
            .checked_add(middle_bytes)
            .and_then(|bytes| bytes.checked_add(trailing_bytes))
            .ok_or_else(|| ErofsError::Invalid("compact index table overflows".to_string()))?;
        super::validate_metadata_end(
            offset,
            index_bytes,
            superblock.block_size,
            volume_offset,
            superblock.block_count,
        )?;
        Ok(Self {
            offset,
            entries,
            initial_4b,
            compact_2b,
        })
    }

    fn locate(&self, index: u64) -> Result<CompactPackLocation> {
        if index >= self.entries {
            return Err(ErofsError::Invalid(format!(
                "compact cluster index {index} exceeds file metadata"
            )));
        }
        if index < self.initial_4b {
            return compact_pack_location(
                self.offset,
                index,
                super::COMPACT_PACK_4B_ENTRIES,
                super::COMPACT_PACK_4B_BYTES,
            );
        }
        let after_initial = self
            .offset
            .checked_add(self.initial_4b.checked_mul(4).ok_or_else(|| {
                ErofsError::Invalid("compact initial index offset overflows".to_string())
            })?)
            .ok_or_else(|| {
                ErofsError::Invalid("compact initial index offset overflows".to_string())
            })?;
        let relative = index
            .checked_sub(self.initial_4b)
            .ok_or_else(|| ErofsError::Invalid("compact index position underflows".to_string()))?;
        if relative < self.compact_2b {
            return compact_pack_location(
                after_initial,
                relative,
                super::COMPACT_PACK_2B_ENTRIES,
                super::COMPACT_PACK_2B_BYTES,
            );
        }
        let trailing_offset = after_initial
            .checked_add(self.compact_2b.checked_mul(2).ok_or_else(|| {
                ErofsError::Invalid("compact trailing index offset overflows".to_string())
            })?)
            .ok_or_else(|| {
                ErofsError::Invalid("compact trailing index offset overflows".to_string())
            })?;
        compact_pack_location(
            trailing_offset,
            relative.checked_sub(self.compact_2b).ok_or_else(|| {
                ErofsError::Invalid("compact trailing index position underflows".to_string())
            })?,
            super::COMPACT_PACK_4B_ENTRIES,
            super::COMPACT_PACK_4B_BYTES,
        )
    }
}

pub(super) fn read_entry(
    source: &SharedReader,
    layout: &CompactIndexLayout,
    index: u64,
) -> Result<IndexEntry> {
    let location = layout.locate(index)?;
    let bytes = read_exact_at(source, location.offset, location.bytes)?;
    let (kind, low) = decode_compact_entry(
        &bytes,
        location.entry_index,
        location.encoded_bits,
        location.entries,
    )?;
    if kind == super::LCLUSTER_TYPE_NONHEAD {
        return Ok(IndexEntry::NonHead {
            delta_back: compact_delta_back(&bytes, &location, low)?,
            delta_forward: compact_delta_forward(&bytes, &location)?,
        });
    }
    Ok(IndexEntry::Head {
        kind: head_kind(kind)?,
        cluster_offset: low,
        block: compact_head_block(&bytes, &location)?,
    })
}

fn compact_delta_back(bytes: &[u8], location: &CompactPackLocation, low: u16) -> Result<u16> {
    if low & super::LCLUSTER_D0_CBLKCNT != 0 {
        return Err(ErofsError::Unsupported(
            "big physical compression clusters".to_string(),
        ));
    }
    if location.entry_index + 1 != location.entries {
        if low == 0 {
            return Err(ErofsError::Invalid(
                "compact lookback distance is zero".to_string(),
            ));
        }
        return Ok(low);
    }
    let previous = location.entry_index.checked_sub(1).ok_or_else(|| {
        ErofsError::Invalid("compact terminal NONHEAD has no predecessor".to_string())
    })?;
    let (kind, previous_low) =
        decode_compact_entry(bytes, previous, location.encoded_bits, location.entries)?;
    if kind != super::LCLUSTER_TYPE_NONHEAD {
        return Ok(1);
    }
    if previous_low & super::LCLUSTER_D0_CBLKCNT != 0 {
        return Err(ErofsError::Unsupported(
            "big physical compression clusters".to_string(),
        ));
    }
    previous_low
        .checked_add(1)
        .ok_or_else(|| ErofsError::Invalid("compact lookback distance overflows".to_string()))
}

fn compact_delta_forward(bytes: &[u8], location: &CompactPackLocation) -> Result<u16> {
    let mut distance = 0u16;
    let mut entry = location.entry_index;
    loop {
        let (kind, low) =
            decode_compact_entry(bytes, entry, location.encoded_bits, location.entries)?;
        if kind != super::LCLUSTER_TYPE_NONHEAD {
            return Ok(distance);
        }
        if low & super::LCLUSTER_D0_CBLKCNT != 0 {
            return Err(ErofsError::Unsupported(
                "big physical compression clusters".to_string(),
            ));
        }
        distance = distance.checked_add(1).ok_or_else(|| {
            ErofsError::Invalid("compact lookahead distance overflows".to_string())
        })?;
        entry += 1;
        if entry == location.entries {
            if low == 0 {
                return Err(ErofsError::Invalid(
                    "compact terminal lookahead distance is zero".to_string(),
                ));
            }
            return distance.checked_add(low - 1).ok_or_else(|| {
                ErofsError::Invalid("compact lookahead distance overflows".to_string())
            });
        }
    }
}

fn compact_head_block(bytes: &[u8], location: &CompactPackLocation) -> Result<u64> {
    let mut entry = i32::try_from(location.entry_index)
        .map_err(|_| ErofsError::Invalid("compact entry index exceeds i32".to_string()))?;
    let mut physical_blocks = 1u64;
    while entry > 0 {
        entry -= 1;
        let entry_index = usize::try_from(entry)
            .map_err(|_| ErofsError::Invalid("compact entry index is negative".to_string()))?;
        let (kind, low) =
            decode_compact_entry(bytes, entry_index, location.encoded_bits, location.entries)?;
        if kind == super::LCLUSTER_TYPE_NONHEAD {
            if low & super::LCLUSTER_D0_CBLKCNT != 0 {
                return Err(ErofsError::Unsupported(
                    "big physical compression clusters".to_string(),
                ));
            }
            if low == 0 {
                return Err(ErofsError::Invalid(
                    "compact NONHEAD lookback distance is zero".to_string(),
                ));
            }
            entry -= i32::from(low);
        }
        if entry >= 0 {
            physical_blocks = physical_blocks.checked_add(1).ok_or_else(|| {
                ErofsError::Invalid("compact physical distance overflows".to_string())
            })?;
        }
    }
    let base = u64::from(read_u32(
        bytes,
        location.bytes - 4,
        "compact compression block base",
    )?);
    base.checked_add(physical_blocks)
        .ok_or_else(|| ErofsError::Invalid("compact block address overflows".to_string()))
}

fn packed_bytes(entries: u64, entries_per_pack: u64, pack_bytes: u64) -> Result<u64> {
    entries
        .div_ceil(entries_per_pack)
        .checked_mul(pack_bytes)
        .ok_or_else(|| ErofsError::Invalid("compact index byte count overflows".to_string()))
}

fn compact_pack_location(
    segment_offset: u64,
    relative_index: u64,
    entries_per_pack: u64,
    pack_bytes: u64,
) -> Result<CompactPackLocation> {
    let pack = relative_index / entries_per_pack;
    let offset = pack
        .checked_mul(pack_bytes)
        .and_then(|bytes| segment_offset.checked_add(bytes))
        .ok_or_else(|| ErofsError::Invalid("compact pack offset overflows".to_string()))?;
    Ok(CompactPackLocation {
        offset,
        bytes: pack_bytes as usize,
        entries: entries_per_pack as usize,
        entry_index: (relative_index % entries_per_pack) as usize,
        encoded_bits: ((pack_bytes - 4) * 8 / entries_per_pack) as u32,
    })
}

fn decode_compact_entry(
    bytes: &[u8],
    index: usize,
    encoded_bits: u32,
    entries: usize,
) -> Result<(u16, u16)> {
    if index >= entries {
        return Err(ErofsError::Invalid(
            "compact entry index exceeds its pack".to_string(),
        ));
    }
    let bit_offset = (index as u32)
        .checked_mul(encoded_bits)
        .ok_or_else(|| ErofsError::Invalid("compact entry bit offset overflows".to_string()))?;
    let byte_offset = (bit_offset / 8) as usize;
    let value = read_u32(bytes, byte_offset, "compact compression entry")? >> (bit_offset % 8);
    let encoded_mask = (1u32 << encoded_bits) - 1;
    if value & encoded_mask & !((1u32 << super::COMPACT_VALUE_BITS) - 1) != 0 {
        return Err(ErofsError::Invalid(
            "compact compression entry reserved bits are non-zero".to_string(),
        ));
    }
    Ok((
        ((value >> super::COMPACT_LOW_BITS) & u32::from(super::LCLUSTER_TYPE_MASK)) as u16,
        (value & ((1u32 << super::COMPACT_LOW_BITS) - 1)) as u16,
    ))
}
