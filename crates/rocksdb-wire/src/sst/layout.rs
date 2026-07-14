use crate::{Result, RocksDbWireError};

use super::block::{read_block, read_exact_range, verify_block_checksum};
use super::footer::FOOTER_LENGTH;
use super::index::{parse_index, IndexEntry};
use super::metaindex::{parse_metaindex, MetaIndex};
use super::properties::parse_properties;
use super::{
    BlockCompression, Footer, IndexKeyMetadata, RangeReader, SstReadOptions, TableProperties,
};

pub(super) struct ParsedLayout {
    pub(super) footer: Footer,
    pub(super) metaindex: MetaIndex,
    pub(super) properties: TableProperties,
    pub(super) compression_dictionary: Option<Vec<u8>>,
    pub(super) index: Vec<IndexEntry>,
    pub(super) first_index_key: IndexKeyMetadata,
    pub(super) last_index_key: IndexKeyMetadata,
}

pub(super) fn read_layout<R: RangeReader>(
    reader: &mut R,
    file_size: u64,
    options: SstReadOptions,
) -> Result<ParsedLayout> {
    if file_size < FOOTER_LENGTH as u64 {
        return Err(RocksDbWireError::SstFileTooShort {
            file_size,
            minimum: FOOTER_LENGTH as u64,
        });
    }
    let footer_offset = file_size - FOOTER_LENGTH as u64;
    let footer_bytes = read_exact_range(reader, footer_offset, FOOTER_LENGTH)?;
    let footer = Footer::decode(&footer_bytes, file_size)?;
    let meta_block = read_block(
        reader,
        footer.metaindex_handle,
        footer.checksum_type,
        options,
        None,
    )?;
    require_uncompressed_metadata(meta_block.compression, "metaindex block")?;
    let metaindex = parse_metaindex(&meta_block.data, footer_offset, options)?;
    let properties_block = read_block(
        reader,
        metaindex.properties_handle,
        footer.checksum_type,
        options,
        None,
    )?;
    require_uncompressed_metadata(properties_block.compression, "properties block")?;
    let properties = parse_properties(&properties_block.data, options)?;
    validate_properties(&properties)?;
    validate_supported_table_shape(&properties)?;
    validate_census_entry_budget(&properties, options)?;
    validate_metadata_ranges(&footer, &metaindex)?;
    let compression_dictionary = read_compression_dictionary(reader, &footer, &metaindex, options)?;
    verify_auxiliary_meta_blocks(reader, &footer, &metaindex, options)?;
    let data_boundary = validate_data_boundary(&footer, &metaindex, properties.data_size)?;
    let index = read_index(
        reader,
        &footer,
        &properties,
        data_boundary,
        compression_dictionary.as_deref(),
        options,
    )?;
    let first_index_key = index_key(&index, true)?;
    let last_index_key = index_key(&index, false)?;
    Ok(ParsedLayout {
        footer,
        metaindex,
        properties,
        compression_dictionary,
        index,
        first_index_key,
        last_index_key,
    })
}

fn read_index<R: RangeReader>(
    reader: &mut R,
    footer: &Footer,
    properties: &TableProperties,
    data_boundary: u64,
    compression_dictionary: Option<&[u8]>,
    options: SstReadOptions,
) -> Result<Vec<IndexEntry>> {
    let index_block = read_block(
        reader,
        footer.index_handle,
        footer.checksum_type,
        options,
        compression_dictionary,
    )?;
    validate_block_compression(index_block.compression, &properties.compression_name)?;
    validate_index_size(index_block.data.len(), properties.index_size)?;
    let key_kind = if properties.index_key_is_user_key {
        super::IndexKeyKind::User
    } else {
        super::IndexKeyKind::Internal
    };
    let index = parse_index(&index_block.data, data_boundary, key_kind, options)?;
    if index.len() as u64 != properties.num_data_blocks {
        return Err(RocksDbWireError::SstCountMismatch {
            field: "data blocks",
            parsed: index.len() as u64,
            properties: properties.num_data_blocks,
        });
    }
    Ok(index)
}

fn index_key(index: &[IndexEntry], first: bool) -> Result<IndexKeyMetadata> {
    let entry = if first { index.first() } else { index.last() };
    entry
        .map(|entry| entry.key.clone())
        .ok_or(RocksDbWireError::InvalidSstIndex {
            reason: "index contains no data blocks",
        })
}

fn validate_data_boundary(footer: &Footer, metaindex: &MetaIndex, data_size: u64) -> Result<u64> {
    if data_size == 0 {
        return Err(RocksDbWireError::InvalidSstIndex {
            reason: "data size property is zero",
        });
    }
    let metadata_start = std::iter::once(footer.metaindex_handle)
        .chain(std::iter::once(footer.index_handle))
        .chain(metaindex.referenced_handles.iter().copied())
        .map(|handle| handle.offset)
        .min()
        .ok_or(RocksDbWireError::InvalidSstIndex {
            reason: "SST has no metadata handles",
        })?;
    if data_size > metadata_start {
        return Err(RocksDbWireError::InvalidSstProperty {
            context: "data size property",
            reason: "data size extends into metadata blocks",
        });
    }
    Ok(data_size)
}

fn read_compression_dictionary<R: RangeReader>(
    reader: &mut R,
    footer: &Footer,
    metaindex: &MetaIndex,
    options: SstReadOptions,
) -> Result<Option<Vec<u8>>> {
    let Some(handle) = metaindex.compression_dictionary_handle else {
        return Ok(None);
    };
    let block = read_block(reader, handle, footer.checksum_type, options, None)?;
    if block.compression != BlockCompression::None {
        return Err(RocksDbWireError::UnsupportedSstFeature {
            feature: "compressed compression dictionary",
            value: 1,
        });
    }
    if block.data.len() > options.max_compression_dictionary_bytes {
        return Err(RocksDbWireError::SstDecompressedBlockLimit {
            size: block.data.len(),
            limit: options.max_compression_dictionary_bytes,
        });
    }
    Ok(Some(block.data))
}

fn verify_auxiliary_meta_blocks<R: RangeReader>(
    reader: &mut R,
    footer: &Footer,
    metaindex: &MetaIndex,
    options: SstReadOptions,
) -> Result<()> {
    let total =
        metaindex
            .auxiliary_verification_handles
            .iter()
            .try_fold(0u64, |total, handle| {
                total
                    .checked_add(handle.size)
                    .and_then(|value| value.checked_add(super::BLOCK_TRAILER_LENGTH as u64))
                    .ok_or(RocksDbWireError::LengthOverflow {
                        context: "SST auxiliary metadata bytes",
                    })
            })?;
    if total > options.max_auxiliary_metadata_bytes as u64 {
        return Err(RocksDbWireError::SstAuxiliaryMetadataLimit {
            total,
            limit: options.max_auxiliary_metadata_bytes,
        });
    }
    for handle in &metaindex.auxiliary_verification_handles {
        verify_block_checksum(reader, *handle, footer.checksum_type, options)?;
    }
    Ok(())
}

fn validate_census_entry_budget(
    properties: &TableProperties,
    options: SstReadOptions,
) -> Result<()> {
    let data_entries = properties
        .num_entries
        .checked_sub(properties.num_range_deletions)
        .ok_or(RocksDbWireError::InvalidSstProperty {
            context: "entry count properties",
            reason: "range deletions exceed total entries",
        })?;
    if data_entries > options.max_census_entries {
        return Err(RocksDbWireError::SstCensusEntryLimit {
            limit: options.max_census_entries,
        });
    }
    Ok(())
}

fn validate_metadata_ranges(footer: &Footer, metaindex: &MetaIndex) -> Result<()> {
    let mut handles = vec![footer.metaindex_handle, footer.index_handle];
    handles.extend(metaindex.referenced_handles.iter().copied());
    handles.sort_by_key(|handle| handle.offset);
    for pair in handles.windows(2) {
        if pair[0].serialized_end()? > pair[1].offset {
            return Err(RocksDbWireError::InvalidBlockHandle {
                context: "SST metadata blocks",
                reason: "metadata blocks overlap",
            });
        }
    }
    Ok(())
}

fn validate_properties(properties: &TableProperties) -> Result<()> {
    match properties.compression_name.as_str() {
        "NoCompression" | "LZ4" | "LZ4HC" => {}
        _ => {
            return Err(RocksDbWireError::UnsupportedSstFeature {
                feature: "table compression",
                value: 0,
            });
        }
    }
    if properties.comparator_name != "leveldb.BytewiseComparator" {
        return Err(RocksDbWireError::UnsupportedSstFeature {
            feature: "table comparator",
            value: 0,
        });
    }
    Ok(())
}

fn validate_supported_table_shape(properties: &TableProperties) -> Result<()> {
    if properties.num_data_blocks == 0 {
        return Err(RocksDbWireError::UnsupportedSstFeature {
            feature: "table without data blocks",
            value: properties.num_entries,
        });
    }
    Ok(())
}

fn validate_index_size(decoded_size: usize, property_size: u64) -> Result<()> {
    let decoded_size = u64::try_from(decoded_size)
        .ok()
        .and_then(|size| size.checked_add(super::BLOCK_TRAILER_LENGTH as u64))
        .ok_or(RocksDbWireError::LengthOverflow {
            context: "decoded SST index size",
        })?;
    if property_size != decoded_size {
        return Err(RocksDbWireError::InvalidSstProperty {
            context: "index size property",
            reason: "property differs from decoded index size",
        });
    }
    Ok(())
}

fn require_uncompressed_metadata(
    compression: BlockCompression,
    feature: &'static str,
) -> Result<()> {
    if compression != BlockCompression::None {
        return Err(RocksDbWireError::UnsupportedSstFeature { feature, value: 1 });
    }
    Ok(())
}

pub(super) fn validate_block_compression(
    compression: BlockCompression,
    table_compression: &str,
) -> Result<()> {
    let supported = match table_compression {
        "NoCompression" => compression == BlockCompression::None,
        "LZ4" => matches!(compression, BlockCompression::None | BlockCompression::Lz4),
        "LZ4HC" => matches!(
            compression,
            BlockCompression::None | BlockCompression::Lz4Hc
        ),
        _ => false,
    };
    if !supported {
        return Err(RocksDbWireError::UnsupportedSstFeature {
            feature: "block compression differs from table compression",
            value: 1,
        });
    }
    Ok(())
}
