use crate::RocksDbWireError;

use super::block::read_block;
use super::entry::{
    SstDataEntry, SstEntryStreamSummary, SstEntryVisitor, SstRangeDeletionEntry, SstVisitError,
    SstVisitOptions,
};
use super::internal_key::{decode_data_kind, decode_internal_key, validate_internal_order};
use super::layout::{
    read_layout_with_property_validation, validate_block_compression, ParsedLayout,
};
use super::restart::{try_visit_restart_block, RestartEntry, RestartVisitError, ValueEncoding};
use super::stream_totals::{checked_increment, StreamTotals};
use super::{BlockCompression, BlockHandle, RangeReader, TableProperties};

pub fn visit_sst_entries<R, V>(
    reader: &mut R,
    file_size: u64,
    options: SstVisitOptions,
    visitor: &mut V,
) -> std::result::Result<SstEntryStreamSummary, SstVisitError<V::Error>>
where
    R: RangeReader,
    V: SstEntryVisitor,
{
    let layout =
        read_layout_with_property_validation(reader, file_size, options.read, |properties| {
            validate_visit_properties(properties, options)
        })?;
    validate_data_block_limit(&layout, options)?;
    let mut totals = StreamTotals::default();
    visit_data_blocks(reader, &layout, options, visitor, &mut totals)?;
    visit_range_deletions(reader, &layout, options, visitor, &mut totals)?;
    finish_stream_summary(file_size, layout.index.len(), layout.properties, totals)
}

fn visit_data_blocks<R, V>(
    reader: &mut R,
    layout: &ParsedLayout,
    options: SstVisitOptions,
    visitor: &mut V,
    totals: &mut StreamTotals,
) -> std::result::Result<(), SstVisitError<V::Error>>
where
    R: RangeReader,
    V: SstEntryVisitor,
{
    let mut previous_internal_key = Vec::new();
    for (block_ordinal, index_entry) in layout.index.iter().enumerate() {
        let block = read_block(
            reader,
            index_entry.handle,
            layout.footer.checksum_type,
            options.read,
            layout.compression_dictionary.as_deref(),
        )?;
        validate_block_compression(block.compression, &layout.properties.compression_name)?;
        let block_ordinal =
            u64::try_from(block_ordinal).map_err(|_| RocksDbWireError::LengthOverflow {
                context: "SST stream block ordinal",
            })?;
        visit_data_block(
            &block.data,
            DataBlockLocation {
                block_handle: index_entry.handle,
                block_ordinal,
                column_family_id: layout.properties.column_family_id,
            },
            options,
            visitor,
            totals,
            &mut previous_internal_key,
        )?;
    }
    Ok(())
}

pub(super) fn visit_data_block<V>(
    block: &[u8],
    location: DataBlockLocation,
    options: SstVisitOptions,
    visitor: &mut V,
    totals: &mut StreamTotals,
    previous_internal_key: &mut Vec<u8>,
) -> std::result::Result<(), SstVisitError<V::Error>>
where
    V: SstEntryVisitor,
{
    visit_data_block_with_observer(
        block,
        location,
        options,
        visitor,
        totals,
        previous_internal_key,
        |_| Ok(()),
    )
}

pub(super) struct DataEntryObservation<'a> {
    pub(super) user_key: &'a [u8],
    pub(super) sequence: u64,
    pub(super) kind: super::SstEntryKind,
    pub(super) key_size: usize,
    pub(super) value_size: usize,
}

#[derive(Clone, Copy)]
pub(super) struct DataBlockLocation {
    pub(super) block_handle: BlockHandle,
    pub(super) block_ordinal: u64,
    pub(super) column_family_id: u32,
}

pub(super) fn visit_data_block_with_observer<V, O>(
    block: &[u8],
    location: DataBlockLocation,
    options: SstVisitOptions,
    visitor: &mut V,
    totals: &mut StreamTotals,
    previous_internal_key: &mut Vec<u8>,
    mut observe: O,
) -> std::result::Result<(), SstVisitError<V::Error>>
where
    V: SstEntryVisitor,
    O: FnMut(DataEntryObservation<'_>) -> std::result::Result<(), RocksDbWireError>,
{
    totals.observe_decompressed_bytes(block.len(), options.max_total_decompressed_bytes)?;
    let mut entry_ordinal = 0u64;
    map_restart_result(try_visit_restart_block(
        block,
        ValueEncoding::Full,
        options.read,
        |entry| {
            let result = visit_data_entry(
                entry,
                DataEntryLocation {
                    block_handle: location.block_handle,
                    block_ordinal: location.block_ordinal,
                    entry_ordinal,
                    column_family_id: location.column_family_id,
                },
                visitor,
                totals,
                previous_internal_key,
                options,
                &mut observe,
            );
            if result.is_ok() {
                entry_ordinal = checked_increment(entry_ordinal, "SST stream entry ordinal")?;
            }
            result
        },
    ))
    .map(|_| ())
}

#[derive(Clone, Copy)]
struct DataEntryLocation {
    block_handle: BlockHandle,
    block_ordinal: u64,
    entry_ordinal: u64,
    column_family_id: u32,
}

fn visit_data_entry<V, O>(
    entry: RestartEntry<'_>,
    location: DataEntryLocation,
    visitor: &mut V,
    totals: &mut StreamTotals,
    previous_internal_key: &mut Vec<u8>,
    options: SstVisitOptions,
    observe: &mut O,
) -> std::result::Result<(), SstVisitError<V::Error>>
where
    V: SstEntryVisitor,
    O: FnMut(DataEntryObservation<'_>) -> std::result::Result<(), RocksDbWireError>,
{
    let internal = decode_internal_key(entry.key)?;
    validate_internal_order(previous_internal_key, entry.key)?;
    let kind = decode_data_kind(internal.value_type)?;
    totals.observe_data(
        entry.key.len(),
        entry.value.len(),
        internal.sequence,
        kind,
        options.max_total_entries,
    )?;
    observe(DataEntryObservation {
        user_key: internal.user_key,
        sequence: internal.sequence,
        kind,
        key_size: entry.key.len(),
        value_size: entry.value.len(),
    })?;
    visitor
        .visit_data(SstDataEntry {
            column_family_id: location.column_family_id,
            block_handle: location.block_handle,
            block_ordinal: location.block_ordinal,
            entry_ordinal: location.entry_ordinal,
            internal_key: entry.key,
            user_key: internal.user_key,
            sequence: internal.sequence,
            kind,
            value: entry.value,
        })
        .map_err(SstVisitError::Visitor)?;
    previous_internal_key.clear();
    previous_internal_key.extend_from_slice(entry.key);
    Ok(())
}

fn visit_range_deletions<R, V>(
    reader: &mut R,
    layout: &ParsedLayout,
    options: SstVisitOptions,
    visitor: &mut V,
    totals: &mut StreamTotals,
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
    validate_range_deletion_compression(block.compression)?;
    visit_range_deletion_block(
        &block.data,
        handle,
        layout.properties.column_family_id,
        options,
        visitor,
        totals,
    )
}

pub(super) fn validate_range_deletion_compression<E>(
    compression: BlockCompression,
) -> std::result::Result<(), SstVisitError<E>> {
    if compression != BlockCompression::None {
        return Err(SstVisitError::Wire(
            RocksDbWireError::UnsupportedSstFeature {
                feature: "compressed range deletion block",
                value: 1,
            },
        ));
    }
    Ok(())
}

pub(super) fn visit_range_deletion_block<V>(
    block: &[u8],
    handle: BlockHandle,
    column_family_id: u32,
    options: SstVisitOptions,
    visitor: &mut V,
    totals: &mut StreamTotals,
) -> std::result::Result<(), SstVisitError<V::Error>>
where
    V: SstEntryVisitor,
{
    totals.observe_decompressed_bytes(block.len(), options.max_total_decompressed_bytes)?;
    let mut entry_ordinal = 0u64;
    map_restart_result(try_visit_restart_block(
        block,
        ValueEncoding::Full,
        options.read,
        |entry| {
            if !entry.at_restart {
                return Err(SstVisitError::Wire(RocksDbWireError::InvalidRestartBlock {
                    reason: "range deletion entries must each begin at a restart point",
                }));
            }
            let internal = decode_internal_key(entry.key)?;
            if internal.value_type != 0x0f {
                return Err(SstVisitError::Wire(
                    RocksDbWireError::UnsupportedSstEntryType {
                        value_type: internal.value_type,
                    },
                ));
            }
            if internal.user_key > entry.value {
                return Err(SstVisitError::Wire(RocksDbWireError::InvalidSstProperty {
                    context: "range deletion entry",
                    reason: "start key is after end key",
                }));
            }
            totals.observe_range(
                entry.key.len(),
                entry.value.len(),
                internal.sequence,
                options.max_total_entries,
                options.max_range_deletions,
            )?;
            visitor
                .visit_range_deletion(SstRangeDeletionEntry {
                    column_family_id,
                    block_handle: handle,
                    entry_ordinal,
                    internal_key: entry.key,
                    start_user_key: internal.user_key,
                    end_user_key: entry.value,
                    sequence: internal.sequence,
                })
                .map_err(SstVisitError::Visitor)?;
            entry_ordinal = checked_increment(entry_ordinal, "SST range entry ordinal")?;
            Ok(())
        },
    ))
    .map(|_| ())
}

pub(super) fn validate_visit_properties(
    properties: &TableProperties,
    options: SstVisitOptions,
) -> std::result::Result<(), RocksDbWireError> {
    if properties.num_data_blocks > options.max_data_blocks {
        return Err(RocksDbWireError::SstStreamDataBlockLimit {
            count: properties.num_data_blocks,
            limit: options.max_data_blocks,
        });
    }
    if properties.num_entries > options.max_total_entries {
        return Err(RocksDbWireError::SstStreamEntryLimit {
            limit: options.max_total_entries,
        });
    }
    if properties.num_range_deletions > options.max_range_deletions {
        return Err(RocksDbWireError::SstStreamRangeDeletionLimit {
            limit: options.max_range_deletions,
        });
    }
    Ok(())
}

pub(super) fn validate_data_block_limit(
    layout: &ParsedLayout,
    options: SstVisitOptions,
) -> std::result::Result<(), RocksDbWireError> {
    let data_blocks = layout.index.len() as u64;
    if data_blocks > options.max_data_blocks {
        return Err(RocksDbWireError::SstStreamDataBlockLimit {
            count: data_blocks,
            limit: options.max_data_blocks,
        });
    }
    Ok(())
}

pub(super) fn finish_stream_summary<E>(
    file_size: u64,
    data_block_count: usize,
    properties: TableProperties,
    mut totals: StreamTotals,
) -> std::result::Result<SstEntryStreamSummary, SstVisitError<E>> {
    totals.finish();
    totals.validate(&properties)?;
    Ok(SstEntryStreamSummary {
        file_size,
        properties,
        data_block_count: data_block_count as u64,
        scanned_decompressed_bytes: totals.scanned_decompressed_bytes,
        counts: totals.counts,
        raw_key_size: totals.raw_key_size,
        raw_value_size: totals.raw_value_size,
        smallest_sequence: totals.smallest_sequence,
        largest_sequence: totals.largest_sequence,
    })
}

fn map_restart_result<T, E>(
    result: std::result::Result<T, RestartVisitError<SstVisitError<E>>>,
) -> std::result::Result<T, SstVisitError<E>> {
    match result {
        Ok(value) => Ok(value),
        Err(RestartVisitError::Wire(error)) => Err(SstVisitError::Wire(error)),
        Err(RestartVisitError::Visitor(error)) => Err(error),
    }
}
