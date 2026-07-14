use crate::{Result, RocksDbWireError};

use super::census::CensusBuilder;
use super::restart::{visit_restart_block, ValueEncoding};
use super::{BlockCompression, BlockHandle, DataBlockStats, EntryTypeCounts, SstReadOptions};

pub(crate) struct ParsedRangeDeletionStats {
    pub counts: EntryTypeCounts,
    pub raw_key_size: u64,
    pub raw_value_size: u64,
    pub smallest_sequence: u64,
    pub largest_sequence: u64,
}

pub(crate) fn parse_data_block(
    handle: BlockHandle,
    compression: BlockCompression,
    block: &[u8],
    options: SstReadOptions,
    census: &mut CensusBuilder<'_>,
) -> Result<DataBlockStats> {
    let mut counts = EntryTypeCounts::default();
    let mut raw_key_size = 0u64;
    let mut raw_value_size = 0u64;
    let mut smallest_sequence = u64::MAX;
    let mut largest_sequence = 0u64;
    visit_restart_block(block, ValueEncoding::Full, options, |entry| {
        let metadata = decode_internal_key(entry.key)?;
        counts.entries += 1;
        raw_key_size += entry.key.len() as u64;
        raw_value_size += entry.value.len() as u64;
        match metadata.value_type {
            0x00 | 0x07 | 0x14 => counts.deletions += 1,
            0x01 | 0x11 | 0x16 => counts.values += 1,
            0x02 => counts.merges += 1,
            value_type => {
                return Err(RocksDbWireError::UnsupportedSstEntryType { value_type });
            }
        }
        smallest_sequence = smallest_sequence.min(metadata.sequence);
        largest_sequence = largest_sequence.max(metadata.sequence);
        census.observe(metadata.user_key)?;
        Ok(())
    })?;
    if counts.entries == 0 {
        smallest_sequence = 0;
    }
    Ok(DataBlockStats {
        handle,
        compression,
        uncompressed_size: block.len() as u64,
        counts,
        raw_key_size,
        raw_value_size,
        smallest_sequence,
        largest_sequence,
    })
}

pub(crate) fn parse_range_deletion_block(
    block: &[u8],
    options: SstReadOptions,
) -> Result<ParsedRangeDeletionStats> {
    let mut counts = EntryTypeCounts::default();
    let mut raw_key_size = 0u64;
    let mut raw_value_size = 0u64;
    let mut smallest_sequence = u64::MAX;
    let mut largest_sequence = 0u64;
    visit_restart_block(block, ValueEncoding::Full, options, |entry| {
        let metadata = decode_internal_key(entry.key)?;
        if metadata.value_type != 0x0f {
            return Err(RocksDbWireError::UnsupportedSstEntryType {
                value_type: metadata.value_type,
            });
        }
        counts.entries += 1;
        counts.deletions += 1;
        counts.range_deletions += 1;
        raw_key_size += entry.key.len() as u64;
        raw_value_size += entry.value.len() as u64;
        smallest_sequence = smallest_sequence.min(metadata.sequence);
        largest_sequence = largest_sequence.max(metadata.sequence);
        Ok(())
    })?;
    if counts.entries == 0 {
        smallest_sequence = 0;
    }
    Ok(ParsedRangeDeletionStats {
        counts,
        raw_key_size,
        raw_value_size,
        smallest_sequence,
        largest_sequence,
    })
}

struct InternalKeyMetadata<'a> {
    user_key: &'a [u8],
    sequence: u64,
    value_type: u8,
}

fn decode_internal_key(key: &[u8]) -> Result<InternalKeyMetadata<'_>> {
    if key.len() < 8 {
        return Err(RocksDbWireError::InternalKeyTooShort {
            context: "SST data entry",
            length: key.len(),
        });
    }
    let trailer = u64::from_le_bytes(key[key.len() - 8..].try_into().map_err(|_| {
        RocksDbWireError::InvalidField {
            context: "SST internal key trailer",
            reason: "fixed64 width",
        }
    })?);
    Ok(InternalKeyMetadata {
        user_key: &key[..key.len() - 8],
        sequence: trailer >> 8,
        value_type: trailer as u8,
    })
}
