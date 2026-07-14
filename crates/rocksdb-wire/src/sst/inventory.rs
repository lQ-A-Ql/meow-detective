use crate::{Result, RocksDbWireError};

use super::block::read_block;
use super::census::CensusBuilder;
use super::data::{parse_data_block, parse_range_deletion_block};
use super::layout::{read_layout, validate_block_compression, ParsedLayout};
use super::{
    DataBlockStats, EntryTypeCounts, KeySpaceCensus, KeySpaceCensusContext, RangeReader,
    SstInspection, SstReadOptions, TableProperties,
};

pub fn inspect_sst<R: RangeReader>(
    reader: &mut R,
    file_size: u64,
    options: SstReadOptions,
    census_context: &KeySpaceCensusContext,
) -> Result<SstInspection> {
    let layout = read_layout(reader, file_size, options)?;
    census_context.validate_column_family(&layout.properties.column_family_name)?;
    let blocks = inspect_blocks(reader, &layout, options, census_context)?;
    Ok(SstInspection {
        file_size,
        footer: layout.footer,
        properties_handle: layout.metaindex.properties_handle,
        filter_handle_count: layout.metaindex.filter_handle_count,
        compression_dictionary_present: layout.compression_dictionary.is_some(),
        range_deletion_block_present: layout.metaindex.range_deletion_handle.is_some(),
        unknown_meta_block_count: layout.metaindex.unknown_meta_block_count,
        properties: layout.properties,
        data_blocks: blocks.data_blocks,
        first_index_key: layout.first_index_key,
        last_index_key: layout.last_index_key,
        counts: blocks.counts,
        raw_key_size: blocks.raw_key_size,
        raw_value_size: blocks.raw_value_size,
        smallest_sequence: blocks.smallest_sequence,
        largest_sequence: blocks.largest_sequence,
        census: blocks.census,
    })
}

struct BlockInspection {
    data_blocks: Vec<DataBlockStats>,
    counts: EntryTypeCounts,
    raw_key_size: u64,
    raw_value_size: u64,
    smallest_sequence: u64,
    largest_sequence: u64,
    census: KeySpaceCensus,
}

fn inspect_blocks<R: RangeReader>(
    reader: &mut R,
    layout: &ParsedLayout,
    options: SstReadOptions,
    census_context: &KeySpaceCensusContext,
) -> Result<BlockInspection> {
    let mut census = CensusBuilder::new(options, census_context);
    let mut data_blocks = Vec::with_capacity(layout.index.len());
    let mut totals = EntryTotals::default();
    for entry in &layout.index {
        let block = read_block(
            reader,
            entry.handle,
            layout.footer.checksum_type,
            options,
            layout.compression_dictionary.as_deref(),
        )?;
        census.add_decompressed_bytes(block.data.len() as u64)?;
        let stats = parse_data_block(
            entry.handle,
            block.compression,
            &block.data,
            options,
            &mut census,
        )?;
        validate_block_compression(stats.compression, &layout.properties.compression_name)?;
        totals.add_data_block(&stats)?;
        data_blocks.push(stats);
    }
    inspect_range_deletions(reader, layout, options, &mut totals)?;
    totals.finish();
    validate_counts(
        &totals.counts,
        totals.raw_key_size,
        totals.raw_value_size,
        &layout.properties,
    )?;
    Ok(BlockInspection {
        data_blocks,
        counts: totals.counts,
        raw_key_size: totals.raw_key_size,
        raw_value_size: totals.raw_value_size,
        smallest_sequence: totals.smallest_sequence,
        largest_sequence: totals.largest_sequence,
        census: census.finish(),
    })
}

fn inspect_range_deletions<R: RangeReader>(
    reader: &mut R,
    layout: &ParsedLayout,
    options: SstReadOptions,
    totals: &mut EntryTotals,
) -> Result<()> {
    let Some(handle) = layout.metaindex.range_deletion_handle else {
        return Ok(());
    };
    let block = read_block(
        reader,
        handle,
        layout.footer.checksum_type,
        options,
        layout.compression_dictionary.as_deref(),
    )?;
    validate_block_compression(block.compression, &layout.properties.compression_name)?;
    let stats = parse_range_deletion_block(&block.data, options)?;
    totals.add(
        &stats.counts,
        stats.raw_key_size,
        stats.raw_value_size,
        stats.smallest_sequence,
        stats.largest_sequence,
    )?;
    Ok(())
}

#[derive(Default)]
pub(super) struct EntryTotals {
    pub(super) counts: EntryTypeCounts,
    pub(super) raw_key_size: u64,
    pub(super) raw_value_size: u64,
    pub(super) smallest_sequence: u64,
    pub(super) largest_sequence: u64,
}

impl EntryTotals {
    pub(super) fn add_data_block(&mut self, stats: &DataBlockStats) -> Result<()> {
        self.add(
            &stats.counts,
            stats.raw_key_size,
            stats.raw_value_size,
            stats.smallest_sequence,
            stats.largest_sequence,
        )
    }

    pub(super) fn add(
        &mut self,
        counts: &EntryTypeCounts,
        raw_key_size: u64,
        raw_value_size: u64,
        smallest_sequence: u64,
        largest_sequence: u64,
    ) -> Result<()> {
        let first_entries = self.counts.entries == 0 && counts.entries != 0;
        add_counts(&mut self.counts, counts)?;
        self.raw_key_size = self.raw_key_size.checked_add(raw_key_size).ok_or(
            RocksDbWireError::LengthOverflow {
                context: "SST aggregate raw key bytes",
            },
        )?;
        self.raw_value_size = self.raw_value_size.checked_add(raw_value_size).ok_or(
            RocksDbWireError::LengthOverflow {
                context: "SST aggregate raw value bytes",
            },
        )?;
        if first_entries {
            self.smallest_sequence = smallest_sequence;
        } else if counts.entries != 0 {
            self.smallest_sequence = self.smallest_sequence.min(smallest_sequence);
        }
        self.largest_sequence = self.largest_sequence.max(largest_sequence);
        Ok(())
    }

    pub(super) fn finish(&mut self) {
        if self.counts.entries == 0 {
            self.smallest_sequence = 0;
        }
    }
}

pub(super) fn validate_counts(
    counts: &EntryTypeCounts,
    raw_key_size: u64,
    raw_value_size: u64,
    properties: &TableProperties,
) -> Result<()> {
    compare_count("entries", counts.entries, properties.num_entries)?;
    compare_count("deletions", counts.deletions, properties.deleted_keys)?;
    compare_count("merge operands", counts.merges, properties.merge_operands)?;
    compare_count(
        "range deletions",
        counts.range_deletions,
        properties.num_range_deletions,
    )?;
    compare_count("raw key bytes", raw_key_size, properties.raw_key_size)?;
    compare_count("raw value bytes", raw_value_size, properties.raw_value_size)
}

fn compare_count(field: &'static str, parsed: u64, properties: u64) -> Result<()> {
    if parsed != properties {
        return Err(RocksDbWireError::SstCountMismatch {
            field,
            parsed,
            properties,
        });
    }
    Ok(())
}

fn add_counts(target: &mut EntryTypeCounts, source: &EntryTypeCounts) -> Result<()> {
    target.entries =
        target
            .entries
            .checked_add(source.entries)
            .ok_or(RocksDbWireError::LengthOverflow {
                context: "SST aggregate entry count",
            })?;
    target.values =
        target
            .values
            .checked_add(source.values)
            .ok_or(RocksDbWireError::LengthOverflow {
                context: "SST aggregate value count",
            })?;
    target.deletions =
        target
            .deletions
            .checked_add(source.deletions)
            .ok_or(RocksDbWireError::LengthOverflow {
                context: "SST aggregate deletion count",
            })?;
    target.merges =
        target
            .merges
            .checked_add(source.merges)
            .ok_or(RocksDbWireError::LengthOverflow {
                context: "SST aggregate merge count",
            })?;
    target.range_deletions = target
        .range_deletions
        .checked_add(source.range_deletions)
        .ok_or(RocksDbWireError::LengthOverflow {
            context: "SST aggregate range deletion count",
        })?;
    Ok(())
}
