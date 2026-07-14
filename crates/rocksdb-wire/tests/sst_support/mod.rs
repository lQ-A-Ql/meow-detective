use std::fmt::{Display, Formatter};

use rocksdb_wire::{
    BlockHandle, RangeReader, BLOCK_BASED_TABLE_MAGIC, BLOCK_TRAILER_LENGTH, FOOTER_LENGTH,
};

const XXH3_LAST_BYTE_PRIME: u32 = 0x6b90_83d9;

#[derive(Debug, Clone, Copy)]
pub enum DataCompression {
    None,
    Lz4,
    Lz4Hc,
}

impl DataCompression {
    fn id(self) -> u8 {
        match self {
            Self::None => 0x00,
            Self::Lz4 => 0x04,
            Self::Lz4Hc => 0x05,
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::None => "NoCompression",
            Self::Lz4 => "LZ4",
            Self::Lz4Hc => "LZ4HC",
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct FixtureOptions {
    pub compression: DataCompression,
    pub with_dictionary: bool,
    pub index_keys_are_user: bool,
    pub properties_format_version: u64,
    pub range_tombstone_only: bool,
    pub external_sst_properties: bool,
    pub additional_unknown_meta_blocks: usize,
}

impl Default for FixtureOptions {
    fn default() -> Self {
        Self {
            compression: DataCompression::Lz4,
            with_dictionary: false,
            index_keys_are_user: true,
            properties_format_version: 0,
            range_tombstone_only: false,
            external_sst_properties: false,
            additional_unknown_meta_blocks: 0,
        }
    }
}

pub struct BuiltSst {
    pub bytes: Vec<u8>,
    pub data_handles: Vec<BlockHandle>,
    #[allow(dead_code)]
    pub range_handle: BlockHandle,
    pub unknown_meta_handle: BlockHandle,
    pub unknown_meta_handles: Vec<BlockHandle>,
    pub index_handle: BlockHandle,
    pub footer_offset: usize,
}

pub fn build_sst(options: FixtureOptions) -> BuiltSst {
    let all_entries = [
        Entry::new(b"m-key-a", 30, 1, b"value-a"),
        Entry::new(b"m-key-b", 20, 0, b""),
        Entry::new(b"m-key-c", 10, 2, b"merge"),
    ];
    let entries = if options.range_tombstone_only {
        &all_entries[..0]
    } else {
        &all_entries[..]
    };
    let range_entry = Entry::new(b"m-key-a", 5, 0x0f, b"m-key-z");
    let raw_key_size = entries
        .iter()
        .map(|entry| entry.key.len() as u64)
        .sum::<u64>()
        + range_entry.key.len() as u64;
    let raw_value_size = entries
        .iter()
        .map(|entry| entry.value.len() as u64)
        .sum::<u64>()
        + range_entry.value.len() as u64;
    let dictionary = options
        .with_dictionary
        .then_some(b"m-key-value-".as_slice());

    let mut bytes = Vec::new();
    let mut data_handles = Vec::new();
    for entry in entries {
        let block = restart_block(&[(entry.key.as_slice(), entry.value.as_slice())], 1);
        data_handles.push(append_block(
            &mut bytes,
            &block,
            options.compression,
            dictionary,
        ));
    }
    let data_size = bytes.len() as u64;
    let range_block = restart_block(
        &[(range_entry.key.as_slice(), range_entry.value.as_slice())],
        1,
    );
    let range_handle = append_block(&mut bytes, &range_block, DataCompression::None, None);
    let dictionary_handle =
        dictionary.map(|value| append_block(&mut bytes, value, DataCompression::None, None));
    let mut unknown_handles = vec![append_block(
        &mut bytes,
        b"opaque-meta",
        DataCompression::None,
        None,
    )];
    for _ in 0..options.additional_unknown_meta_blocks {
        unknown_handles.push(append_block(
            &mut bytes,
            b"opaque-extra-meta",
            DataCompression::None,
            None,
        ));
    }

    let index_keys = if options.index_keys_are_user {
        vec![
            b"m-key-a".to_vec(),
            b"m-key-b".to_vec(),
            b"m-key-c".to_vec(),
        ]
    } else {
        vec![
            internal_key(b"m-key-a", 30, 1),
            internal_key(b"m-key-b", 20, 0),
            internal_key(b"m-key-c", 10, 2),
        ]
    };
    let index_block = delta_index_block(&index_keys, &data_handles);
    let index_size = index_block.len() as u64 + BLOCK_TRAILER_LENGTH as u64;
    let index_handle = append_block(&mut bytes, &index_block, options.compression, dictionary);
    let properties_block =
        properties_block(data_size, index_size, raw_key_size, raw_value_size, options);
    let properties_handle =
        append_block(&mut bytes, &properties_block, DataCompression::None, None);
    let metaindex_block = metaindex_block(
        properties_handle,
        range_handle,
        dictionary_handle,
        &unknown_handles,
    );
    let metaindex_handle = append_block(&mut bytes, &metaindex_block, DataCompression::None, None);
    let footer_offset = bytes.len();
    bytes.extend_from_slice(&footer(metaindex_handle, index_handle));
    BuiltSst {
        bytes,
        data_handles,
        range_handle,
        unknown_meta_handle: unknown_handles[0],
        unknown_meta_handles: unknown_handles,
        index_handle,
        footer_offset,
    }
}

pub fn rewrite_checksum(bytes: &mut [u8], handle: BlockHandle) {
    let offset = handle.offset as usize;
    let size = handle.size as usize;
    let compression = bytes[offset + size];
    let checksum = xxh3_checksum(&bytes[offset..offset + size], compression);
    bytes[offset + size + 1..offset + size + 5].copy_from_slice(&checksum.to_le_bytes());
}

pub fn decode_plain_block(bytes: &[u8], handle: BlockHandle) -> Vec<u8> {
    let offset = handle.offset as usize;
    bytes[offset..offset + handle.size as usize].to_vec()
}

pub fn restart_block(entries: &[(&[u8], &[u8])], restart_interval: usize) -> Vec<u8> {
    let mut block = Vec::new();
    let mut restarts = vec![0u32];
    let mut previous = Vec::new();
    let mut counter = 0usize;
    for (key, value) in entries {
        let shared = if counter >= restart_interval {
            restarts.push(block.len() as u32);
            counter = 0;
            0
        } else {
            common_prefix(&previous, key)
        };
        put_varint(shared as u64, &mut block);
        put_varint((key.len() - shared) as u64, &mut block);
        put_varint(value.len() as u64, &mut block);
        block.extend_from_slice(&key[shared..]);
        block.extend_from_slice(value);
        previous.clear();
        previous.extend_from_slice(key);
        counter += 1;
    }
    let restart_count = restarts.len() as u32;
    for restart in restarts {
        block.extend_from_slice(&restart.to_le_bytes());
    }
    block.extend_from_slice(&restart_count.to_le_bytes());
    block
}

pub fn internal_key(user_key: &[u8], sequence: u64, value_type: u8) -> Vec<u8> {
    let mut key = user_key.to_vec();
    key.extend_from_slice(&((sequence << 8) | u64::from(value_type)).to_le_bytes());
    key
}

fn properties_block(
    data_size: u64,
    index_size: u64,
    raw_key_size: u64,
    raw_value_size: u64,
    options: FixtureOptions,
) -> Vec<u8> {
    let data_entry_count = u64::from(!options.range_tombstone_only) * 3;
    let data_block_count = data_entry_count;
    let entry_count = data_entry_count + 1;
    let deletion_count = 1 + u64::from(data_entry_count != 0);
    let merge_operand_count = u64::from(data_entry_count != 0);
    let mut properties = vec![
        string_property("rocksdb.column.family.name", "m-0"),
        string_property("rocksdb.comparator", "leveldb.BytewiseComparator"),
        string_property("rocksdb.compression", options.compression.name()),
        string_property("rocksdb.creating.db.identity", "db-id"),
        string_property("rocksdb.creating.session.identity", "session-id"),
        numeric_property("rocksdb.column.family.id", 1),
        numeric_property("rocksdb.data.size", data_size),
        numeric_property("rocksdb.deleted.keys", deletion_count),
        numeric_property("rocksdb.filter.size", 0),
        numeric_property("rocksdb.format.version", options.properties_format_version),
        numeric_property(
            "rocksdb.index.key.is.user.key",
            u64::from(options.index_keys_are_user),
        ),
        numeric_property("rocksdb.index.size", index_size),
        numeric_property("rocksdb.index.value.is.delta.encoded", 1),
        numeric_property("rocksdb.merge.operands", merge_operand_count),
        numeric_property("rocksdb.num.data.blocks", data_block_count),
        numeric_property("rocksdb.num.entries", entry_count),
        numeric_property("rocksdb.num.range-deletions", 1),
        numeric_property("rocksdb.original.file.number", 146),
        numeric_property("rocksdb.raw.key.size", raw_key_size),
        numeric_property("rocksdb.raw.value.size", raw_value_size),
        fixed32_property("rocksdb.block.based.table.index.type", 0),
        string_property("user.private.raw", "ignored"),
    ];
    if options.external_sst_properties {
        properties.push(fixed32_property("rocksdb.external_sst_file.version", 2));
        properties.push(fixed64_property(
            "rocksdb.external_sst_file.global_seqno",
            42,
        ));
    }
    properties.sort_by(|left, right| left.0.cmp(&right.0));
    let refs = properties
        .iter()
        .map(|(key, value)| (key.as_slice(), value.as_slice()))
        .collect::<Vec<_>>();
    restart_block(&refs, usize::MAX)
}

fn metaindex_block(
    properties: BlockHandle,
    range: BlockHandle,
    dictionary: Option<BlockHandle>,
    unknown: &[BlockHandle],
) -> Vec<u8> {
    let mut entries = vec![
        (b"rocksdb.properties".to_vec(), encode_handle(properties)),
        (b"rocksdb.range_del".to_vec(), encode_handle(range)),
    ];
    entries.extend(unknown.iter().enumerate().map(|(index, handle)| {
        let name = if index == 0 {
            b"rocksdb.unknown.safe".to_vec()
        } else {
            format!("rocksdb.unknown.safe.{index}").into_bytes()
        };
        (name, encode_handle(*handle))
    }));
    if let Some(dictionary) = dictionary {
        entries.push((
            b"rocksdb.compression_dict".to_vec(),
            encode_handle(dictionary),
        ));
    }
    entries.sort_by(|left, right| left.0.cmp(&right.0));
    let refs = entries
        .iter()
        .map(|(key, value)| (key.as_slice(), value.as_slice()))
        .collect::<Vec<_>>();
    restart_block(&refs, 1)
}

fn delta_index_block(keys: &[Vec<u8>], handles: &[BlockHandle]) -> Vec<u8> {
    let mut block = Vec::new();
    let mut previous_key = Vec::new();
    for (index, (key, handle)) in keys.iter().zip(handles).enumerate() {
        let shared = if index == 0 {
            0
        } else {
            common_prefix(&previous_key, key)
        };
        put_varint(shared as u64, &mut block);
        put_varint((key.len() - shared) as u64, &mut block);
        block.extend_from_slice(&key[shared..]);
        if index == 0 || shared == 0 {
            block.extend_from_slice(&encode_handle(*handle));
        } else {
            let previous = handles[index - 1];
            let delta = handle.size as i64 - previous.size as i64;
            put_varint(zigzag(delta), &mut block);
        }
        previous_key.clear();
        previous_key.extend_from_slice(key);
    }
    block.extend_from_slice(&0u32.to_le_bytes());
    block.extend_from_slice(&1u32.to_le_bytes());
    block
}

fn append_block(
    output: &mut Vec<u8>,
    plain: &[u8],
    compression: DataCompression,
    dictionary: Option<&[u8]>,
) -> BlockHandle {
    let offset = output.len() as u64;
    let stored = match compression {
        DataCompression::None => plain.to_vec(),
        DataCompression::Lz4 | DataCompression::Lz4Hc => {
            let compressed = match dictionary {
                Some(dictionary) => lz4_flex::block::compress_with_dict(plain, dictionary),
                None => lz4_flex::block::compress(plain),
            };
            let mut framed = Vec::new();
            put_varint(plain.len() as u64, &mut framed);
            framed.extend_from_slice(&compressed);
            framed
        }
    };
    let compression_id = compression.id();
    let checksum = xxh3_checksum(&stored, compression_id);
    output.extend_from_slice(&stored);
    output.push(compression_id);
    output.extend_from_slice(&checksum.to_le_bytes());
    BlockHandle {
        offset,
        size: stored.len() as u64,
    }
}

fn footer(metaindex: BlockHandle, index: BlockHandle) -> Vec<u8> {
    let mut footer = Vec::with_capacity(FOOTER_LENGTH);
    footer.push(0x04);
    footer.extend_from_slice(&encode_handle(metaindex));
    footer.extend_from_slice(&encode_handle(index));
    footer.resize(41, 0);
    footer.extend_from_slice(&5u32.to_le_bytes());
    footer.extend_from_slice(&BLOCK_BASED_TABLE_MAGIC.to_le_bytes());
    footer
}

fn encode_handle(handle: BlockHandle) -> Vec<u8> {
    let mut encoded = Vec::new();
    put_varint(handle.offset, &mut encoded);
    put_varint(handle.size, &mut encoded);
    encoded
}

fn numeric_property(name: &str, value: u64) -> (Vec<u8>, Vec<u8>) {
    let mut encoded = Vec::new();
    put_varint(value, &mut encoded);
    (name.as_bytes().to_vec(), encoded)
}

fn fixed32_property(name: &str, value: u32) -> (Vec<u8>, Vec<u8>) {
    (name.as_bytes().to_vec(), value.to_le_bytes().to_vec())
}

fn fixed64_property(name: &str, value: u64) -> (Vec<u8>, Vec<u8>) {
    (name.as_bytes().to_vec(), value.to_le_bytes().to_vec())
}

fn string_property(name: &str, value: &str) -> (Vec<u8>, Vec<u8>) {
    (name.as_bytes().to_vec(), value.as_bytes().to_vec())
}

fn xxh3_checksum(stored: &[u8], compression: u8) -> u32 {
    xxhash_rust::xxh3::xxh3_64(stored) as u32
        ^ u32::from(compression).wrapping_mul(XXH3_LAST_BYTE_PRIME)
}

fn put_varint(mut value: u64, output: &mut Vec<u8>) {
    while value >= 0x80 {
        output.push(value as u8 | 0x80);
        value >>= 7;
    }
    output.push(value as u8);
}

fn zigzag(value: i64) -> u64 {
    ((value << 1) ^ (value >> 63)) as u64
}

fn common_prefix(left: &[u8], right: &[u8]) -> usize {
    left.iter()
        .zip(right)
        .take_while(|(left, right)| left == right)
        .count()
}

struct Entry {
    key: Vec<u8>,
    value: Vec<u8>,
}

impl Entry {
    fn new(user_key: &[u8], sequence: u64, value_type: u8, value: &[u8]) -> Self {
        Self {
            key: internal_key(user_key, sequence, value_type),
            value: value.to_vec(),
        }
    }
}

#[derive(Debug)]
pub struct MemoryReadError;

impl Display for MemoryReadError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("range is outside the memory reader")
    }
}

impl std::error::Error for MemoryReadError {}

pub struct MemoryRangeReader {
    pub bytes: Vec<u8>,
    pub reads: Vec<(u64, usize)>,
    pub short_read_once: bool,
    pub cancelled: bool,
}

impl MemoryRangeReader {
    pub fn new(bytes: Vec<u8>) -> Self {
        Self {
            bytes,
            reads: Vec::new(),
            short_read_once: false,
            cancelled: false,
        }
    }
}

impl RangeReader for MemoryRangeReader {
    type Error = MemoryReadError;

    fn is_cancelled(&self) -> bool {
        self.cancelled
    }

    fn read_range(&mut self, offset: u64, length: usize) -> Result<Vec<u8>, Self::Error> {
        self.reads.push((offset, length));
        let start = usize::try_from(offset).map_err(|_| MemoryReadError)?;
        let end = start.checked_add(length).ok_or(MemoryReadError)?;
        let mut data = self
            .bytes
            .get(start..end)
            .map(|bytes| bytes.to_vec())
            .ok_or(MemoryReadError)?;
        if self.short_read_once && !data.is_empty() {
            self.short_read_once = false;
            data.pop();
        }
        Ok(data)
    }
}
