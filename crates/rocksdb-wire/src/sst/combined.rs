use crate::RocksDbWireError;

use super::block::read_block;
use super::census::CensusBuilder;
use super::data::parse_range_deletion_block;
use super::inventory::{validate_counts, EntryTotals};
use super::layout::{
    read_layout_with_property_validation, validate_block_compression, ParsedLayout,
};
use super::stream_totals::StreamTotals;
use super::visitor::{
    finish_stream_summary, validate_data_block_limit, validate_range_deletion_compression,
    validate_visit_properties, visit_data_block_with_observer, visit_range_deletion_block,
    DataBlockLocation, DataEntryObservation,
};
use super::{
    BlockCompression, BlockHandle, DataBlockStats, EntryTypeCounts, KeySpaceCensusContext,
    RangeReader, SstEntryKind, SstEntryVisitor, SstInspection, SstInspectionStream, SstVisitError,
    SstVisitOptions,
};

pub fn inspect_sst_with_visitor<R, V>(
    reader: &mut R,
    file_size: u64,
    options: SstVisitOptions,
    census_context: &KeySpaceCensusContext,
    visitor: &mut V,
) -> std::result::Result<SstInspectionStream, SstVisitError<V::Error>>
where
    R: RangeReader,
    V: SstEntryVisitor,
{
    let layout =
        read_layout_with_property_validation(reader, file_size, options.read, |properties| {
            validate_visit_properties(properties, options)
        })?;
    census_context.validate_column_family(&layout.properties.column_family_name)?;
    validate_data_block_limit(&layout, options)?;

    let mut census = CensusBuilder::new(options.read, census_context);
    let mut inspection_totals = EntryTotals::default();
    let mut stream_totals = StreamTotals::default();
    let mut previous_internal_key = Vec::new();
    let mut data_blocks = Vec::with_capacity(layout.index.len());

    for (block_ordinal, index_entry) in layout.index.iter().enumerate() {
        let block = read_block(
            reader,
            index_entry.handle,
            layout.footer.checksum_type,
            options.read,
            layout.compression_dictionary.as_deref(),
        )?;
        census.add_decompressed_bytes(block.data.len() as u64)?;
        let mut stats = DataBlockStatsBuilder::default();
        visit_data_block_with_observer(
            &block.data,
            DataBlockLocation {
                block_handle: index_entry.handle,
                block_ordinal: block_ordinal_u64(block_ordinal)?,
                column_family_id: layout.properties.column_family_id,
            },
            options,
            visitor,
            &mut stream_totals,
            &mut previous_internal_key,
            |entry| stats.observe(entry, &mut census),
        )?;
        let stats = stats.finish(index_entry.handle, block.compression, block.data.len());
        validate_block_compression(stats.compression, &layout.properties.compression_name)?;
        inspection_totals.add_data_block(&stats)?;
        data_blocks.push(stats);
    }

    inspect_range_deletions(
        reader,
        &layout,
        options,
        visitor,
        &mut inspection_totals,
        &mut stream_totals,
    )?;

    inspection_totals.finish();
    validate_counts(
        &inspection_totals.counts,
        inspection_totals.raw_key_size,
        inspection_totals.raw_value_size,
        &layout.properties,
    )?;
    let stream = finish_stream_summary(
        file_size,
        layout.index.len(),
        layout.properties.clone(),
        stream_totals,
    )?;
    let inspection = build_inspection(
        file_size,
        layout,
        data_blocks,
        inspection_totals,
        census.finish(),
    );
    Ok(SstInspectionStream { inspection, stream })
}

fn inspect_range_deletions<R, V>(
    reader: &mut R,
    layout: &ParsedLayout,
    options: SstVisitOptions,
    visitor: &mut V,
    inspection_totals: &mut EntryTotals,
    stream_totals: &mut StreamTotals,
) -> std::result::Result<(), SstVisitError<V::Error>>
where
    R: RangeReader,
    V: SstEntryVisitor,
{
    let Some(handle) = layout.metaindex.range_deletion_handle else {
        return Ok(());
    };
    let block = read_block(
        reader,
        handle,
        layout.footer.checksum_type,
        options.read,
        layout.compression_dictionary.as_deref(),
    )?;
    validate_block_compression(block.compression, &layout.properties.compression_name)?;
    let stats = parse_range_deletion_block(&block.data, options.read)?;
    inspection_totals.add(
        &stats.counts,
        stats.raw_key_size,
        stats.raw_value_size,
        stats.smallest_sequence,
        stats.largest_sequence,
    )?;
    validate_range_deletion_compression(block.compression)?;
    visit_range_deletion_block(
        &block.data,
        handle,
        layout.properties.column_family_id,
        options,
        visitor,
        stream_totals,
    )
}

#[derive(Default)]
struct DataBlockStatsBuilder {
    counts: EntryTypeCounts,
    raw_key_size: u64,
    raw_value_size: u64,
    smallest_sequence: Option<u64>,
    largest_sequence: u64,
}

impl DataBlockStatsBuilder {
    fn observe(
        &mut self,
        entry: DataEntryObservation<'_>,
        census: &mut CensusBuilder<'_>,
    ) -> Result<(), RocksDbWireError> {
        increment(&mut self.counts.entries, "SST combined entry count")?;
        match entry.kind {
            SstEntryKind::Deletion
            | SstEntryKind::SingleDeletion
            | SstEntryKind::DeletionWithTimestamp => {
                increment(&mut self.counts.deletions, "SST combined deletion count")?;
            }
            SstEntryKind::Merge => {
                increment(&mut self.counts.merges, "SST combined merge count")?;
            }
            SstEntryKind::Value | SstEntryKind::BlobIndex | SstEntryKind::WideColumnEntity => {
                increment(&mut self.counts.values, "SST combined value count")?;
            }
        }
        self.raw_key_size = add_size(
            self.raw_key_size,
            entry.key_size,
            "SST combined raw key bytes",
        )?;
        self.raw_value_size = add_size(
            self.raw_value_size,
            entry.value_size,
            "SST combined raw value bytes",
        )?;
        self.smallest_sequence = Some(
            self.smallest_sequence
                .map_or(entry.sequence, |current| current.min(entry.sequence)),
        );
        self.largest_sequence = self.largest_sequence.max(entry.sequence);
        census.observe(entry.user_key)
    }

    fn finish(
        self,
        handle: BlockHandle,
        compression: BlockCompression,
        uncompressed_size: usize,
    ) -> DataBlockStats {
        DataBlockStats {
            handle,
            compression,
            uncompressed_size: uncompressed_size as u64,
            counts: self.counts,
            raw_key_size: self.raw_key_size,
            raw_value_size: self.raw_value_size,
            smallest_sequence: self.smallest_sequence.unwrap_or_default(),
            largest_sequence: self.largest_sequence,
        }
    }
}

fn increment(value: &mut u64, context: &'static str) -> Result<(), RocksDbWireError> {
    *value = value
        .checked_add(1)
        .ok_or(RocksDbWireError::LengthOverflow { context })?;
    Ok(())
}

fn add_size(value: u64, size: usize, context: &'static str) -> Result<u64, RocksDbWireError> {
    value
        .checked_add(size as u64)
        .ok_or(RocksDbWireError::LengthOverflow { context })
}

fn build_inspection(
    file_size: u64,
    layout: super::layout::ParsedLayout,
    data_blocks: Vec<DataBlockStats>,
    totals: EntryTotals,
    census: super::KeySpaceCensus,
) -> SstInspection {
    SstInspection {
        file_size,
        footer: layout.footer,
        properties_handle: layout.metaindex.properties_handle,
        filter_handle_count: layout.metaindex.filter_handle_count,
        compression_dictionary_present: layout.compression_dictionary.is_some(),
        range_deletion_block_present: layout.metaindex.range_deletion_handle.is_some(),
        unknown_meta_block_count: layout.metaindex.unknown_meta_block_count,
        properties: layout.properties,
        data_blocks,
        first_index_key: layout.first_index_key,
        last_index_key: layout.last_index_key,
        counts: totals.counts,
        raw_key_size: totals.raw_key_size,
        raw_value_size: totals.raw_value_size,
        smallest_sequence: totals.smallest_sequence,
        largest_sequence: totals.largest_sequence,
        census,
    }
}

fn block_ordinal_u64(block_ordinal: usize) -> Result<u64, RocksDbWireError> {
    u64::try_from(block_ordinal).map_err(|_| RocksDbWireError::LengthOverflow {
        context: "SST combined block ordinal",
    })
}
