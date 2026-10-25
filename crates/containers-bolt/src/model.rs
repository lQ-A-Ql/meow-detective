#[derive(Debug, Clone)]
pub struct BoltReadOptions {
    pub prefix: Vec<Vec<u8>>,
    pub record_keys: Vec<Vec<u8>>,
    pub value_keys: Vec<Vec<u8>>,
    pub parent_depth: Option<usize>,
    pub bucket_parent_depth: Option<usize>,
    pub parent_suffix: Vec<Vec<u8>>,
    pub include_buckets: bool,
    pub max_file_bytes: u64,
    pub max_entries: usize,
    pub max_depth: usize,
    pub max_output_bytes: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BoltMetadata {
    pub page_size: u32,
    pub txid: u64,
    pub metadata_checksum_verified: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BoltEntry {
    pub bucket_path: Vec<Vec<u8>>,
    pub key: Vec<u8>,
    pub value_length: Option<u64>,
    pub value: Option<Vec<u8>>,
    pub is_bucket: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BoltReadReport {
    pub metadata: BoltMetadata,
    pub entries: Vec<BoltEntry>,
    pub visited_entries: u64,
    pub visited_buckets: u64,
    pub missing_bucket: Option<usize>,
}
