use super::BlockHandle;

const MIB: usize = 1024 * 1024;

pub const BLOCK_TRAILER_LENGTH: usize = 5;
pub const KEY_SPACE_SUMMARY_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChecksumType {
    Xxh3,
}

impl ChecksumType {
    pub(crate) const XXH3_ID: u8 = 0x04;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockCompression {
    None,
    Lz4,
    Lz4Hc,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SstReadOptions {
    pub max_stored_block_bytes: usize,
    pub max_decompressed_block_bytes: usize,
    pub max_compression_dictionary_bytes: usize,
    pub max_auxiliary_metadata_bytes: usize,
    pub max_entries_per_block: usize,
    pub max_key_bytes: usize,
    pub max_value_bytes: usize,
    pub max_properties: usize,
    pub max_metaindex_entries: usize,
    pub max_census_entries: u64,
    pub max_census_decompressed_bytes: u64,
}

impl Default for SstReadOptions {
    fn default() -> Self {
        Self {
            max_stored_block_bytes: 16 * MIB,
            max_decompressed_block_bytes: 64 * MIB,
            max_compression_dictionary_bytes: 16 * MIB,
            max_auxiliary_metadata_bytes: 256 * MIB,
            max_entries_per_block: 100_000,
            max_key_bytes: MIB,
            max_value_bytes: 64 * MIB,
            max_properties: 4096,
            max_metaindex_entries: 4096,
            max_census_entries: 2_000_000,
            max_census_decompressed_bytes: 1024 * MIB as u64,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EntryTypeCounts {
    pub entries: u64,
    pub values: u64,
    pub deletions: u64,
    pub merges: u64,
    pub range_deletions: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DataBlockStats {
    pub handle: BlockHandle,
    pub compression: BlockCompression,
    pub uncompressed_size: u64,
    pub counts: EntryTypeCounts,
    pub raw_key_size: u64,
    pub raw_value_size: u64,
    pub smallest_sequence: u64,
    pub largest_sequence: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IndexKeyKind {
    User,
    Internal,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexKeyMetadata {
    pub key_length: u32,
    pub kind: IndexKeyKind,
    pub sequence: Option<u64>,
    pub value_type: Option<u8>,
    pub xxh3_digest: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeySpaceBucket {
    pub name: String,
    pub entries: u64,
    pub min_user_key_length: u32,
    pub max_user_key_length: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeySpaceCensus {
    pub version: u32,
    pub scanned_entries: u64,
    pub scanned_decompressed_bytes: u64,
    pub complete: bool,
    pub buckets: Vec<KeySpaceBucket>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TableProperties {
    pub num_data_blocks: u64,
    pub num_entries: u64,
    pub deleted_keys: u64,
    pub merge_operands: u64,
    pub num_range_deletions: u64,
    pub raw_key_size: u64,
    pub raw_value_size: u64,
    pub data_size: u64,
    pub index_size: u64,
    pub filter_size: u64,
    pub properties_format_version: u32,
    pub index_key_is_user_key: bool,
    pub index_value_is_delta_encoded: bool,
    pub index_type: u32,
    pub index_partitions: u64,
    pub compression_name: String,
    pub comparator_name: String,
    pub column_family_name: String,
    pub column_family_id: u32,
    pub original_file_number: u64,
    pub db_identity: Option<String>,
    pub db_session_identity: Option<String>,
    pub ignored_user_property_count: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SstInspection {
    pub file_size: u64,
    pub footer: super::Footer,
    pub properties_handle: BlockHandle,
    pub filter_handle_count: u32,
    pub compression_dictionary_present: bool,
    pub range_deletion_block_present: bool,
    pub unknown_meta_block_count: u32,
    pub properties: TableProperties,
    pub data_blocks: Vec<DataBlockStats>,
    pub first_index_key: IndexKeyMetadata,
    pub last_index_key: IndexKeyMetadata,
    pub counts: EntryTypeCounts,
    pub raw_key_size: u64,
    pub raw_value_size: u64,
    pub smallest_sequence: u64,
    pub largest_sequence: u64,
    pub census: KeySpaceCensus,
}
