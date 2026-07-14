use std::collections::HashSet;

use crate::cursor::WireCursor;
use crate::{Result, RocksDbWireError};

use super::restart::{visit_restart_block, ValueEncoding};
use super::{BlockHandle, SstReadOptions};

const PROPERTIES: &[u8] = b"rocksdb.properties";
const OLD_PROPERTIES: &[u8] = b"rocksdb.stats";
const COMPRESSION_DICT: &[u8] = b"rocksdb.compression_dict";
const RANGE_DELETION: &[u8] = b"rocksdb.range_del";

pub(crate) struct MetaIndex {
    pub properties_handle: BlockHandle,
    pub filter_handle_count: u32,
    pub compression_dictionary_handle: Option<BlockHandle>,
    pub range_deletion_handle: Option<BlockHandle>,
    pub unknown_meta_block_count: u32,
    pub referenced_handles: Vec<BlockHandle>,
    pub auxiliary_verification_handles: Vec<BlockHandle>,
}

pub(crate) fn parse_metaindex(
    block: &[u8],
    file_boundary: u64,
    options: SstReadOptions,
) -> Result<MetaIndex> {
    let options = SstReadOptions {
        max_entries_per_block: options
            .max_entries_per_block
            .min(options.max_metaindex_entries),
        ..options
    };
    let mut names = HashSet::new();
    let mut properties_handle = None;
    let mut filter_handle_count = 0u32;
    let mut compression_dictionary_handle = None;
    let mut range_deletion_handle = None;
    let mut unknown_meta_block_count = 0u32;
    let mut referenced_handles = Vec::new();
    let mut auxiliary_verification_handles = Vec::new();
    let mut previous_name = Vec::new();
    let count = visit_restart_block(block, ValueEncoding::Full, options, |entry| {
        validate_meta_name(entry.key)?;
        if !previous_name.is_empty() && previous_name.as_slice() >= entry.key {
            return Err(RocksDbWireError::InvalidMetaIndex {
                reason: "metaindex keys are not strictly ordered",
            });
        }
        previous_name.clear();
        previous_name.extend_from_slice(entry.key);
        let name = String::from_utf8(entry.key.to_vec()).map_err(|_| {
            RocksDbWireError::InvalidMetaIndex {
                reason: "metaindex key is not UTF-8",
            }
        })?;
        if !names.insert(name) {
            return Err(RocksDbWireError::DuplicateMetaBlock);
        }
        let handle = decode_handle(entry.value, file_boundary)?;
        referenced_handles.push(handle);
        match entry.key {
            PROPERTIES | OLD_PROPERTIES => {
                if properties_handle.replace(handle).is_some() {
                    return Err(RocksDbWireError::InvalidMetaIndex {
                        reason: "multiple properties block aliases are present",
                    });
                }
            }
            COMPRESSION_DICT => compression_dictionary_handle = Some(handle),
            RANGE_DELETION => range_deletion_handle = Some(handle),
            key if is_filter_name(key) => {
                filter_handle_count =
                    filter_handle_count
                        .checked_add(1)
                        .ok_or(RocksDbWireError::LengthOverflow {
                            context: "SST filter block count",
                        })?;
                auxiliary_verification_handles.push(handle);
            }
            _ => {
                unknown_meta_block_count = unknown_meta_block_count.checked_add(1).ok_or(
                    RocksDbWireError::LengthOverflow {
                        context: "SST unknown meta block count",
                    },
                )?;
                auxiliary_verification_handles.push(handle);
            }
        }
        Ok(())
    })?;
    debug_assert!(count <= options.max_metaindex_entries);
    let properties_handle = properties_handle.ok_or(RocksDbWireError::MissingPropertiesBlock)?;
    Ok(MetaIndex {
        properties_handle,
        filter_handle_count,
        compression_dictionary_handle,
        range_deletion_handle,
        unknown_meta_block_count,
        referenced_handles,
        auxiliary_verification_handles,
    })
}

fn decode_handle(input: &[u8], boundary: u64) -> Result<BlockHandle> {
    let mut cursor = WireCursor::new(input);
    let handle = BlockHandle::decode(&mut cursor, "SST meta block handle")?;
    if !cursor.is_empty() {
        return Err(RocksDbWireError::InvalidMetaIndex {
            reason: "meta block handle has trailing bytes",
        });
    }
    handle.validate_before(boundary)?;
    Ok(handle)
}

fn validate_meta_name(name: &[u8]) -> Result<()> {
    if name.is_empty()
        || name.len() > 1024
        || name.contains(&0)
        || name.contains(&b'/')
        || name.contains(&b'\\')
    {
        return Err(RocksDbWireError::InvalidMetaIndex {
            reason: "metaindex key is empty, path-like, or too long",
        });
    }
    Ok(())
}

fn is_filter_name(name: &[u8]) -> bool {
    name.starts_with(b"filter.")
        || name.starts_with(b"fullfilter.")
        || name.starts_with(b"partitionedfilter.")
}
