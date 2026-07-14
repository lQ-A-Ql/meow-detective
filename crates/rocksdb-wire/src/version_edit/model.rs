#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NewFileFormat {
    NewFile,
    NewFile2,
    NewFile3,
    NewFile4,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InternalKeyMetadata {
    pub encoded_length: u32,
    pub user_key_length: u32,
    pub sequence_number: u64,
    pub value_type: u8,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NewFileMetadata {
    pub marked_for_compaction: bool,
    pub oldest_blob_file_number: Option<u64>,
    pub oldest_ancestor_time: Option<u64>,
    pub file_creation_time: Option<u64>,
    pub epoch_number: Option<u64>,
    pub temperature: Option<u8>,
    pub compensated_range_deletion_size: Option<u64>,
    pub file_checksum_length: Option<u32>,
    pub file_checksum_function_length: Option<u32>,
    pub unique_id_length: Option<u32>,
    pub min_timestamp_length: Option<u32>,
    pub max_timestamp_length: Option<u32>,
    pub skipped_safe_custom_fields: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewFile {
    pub format: NewFileFormat,
    pub level: u32,
    pub file_number: u64,
    pub path_id: u32,
    pub file_size: u64,
    pub smallest: InternalKeyMetadata,
    pub largest: InternalKeyMetadata,
    pub smallest_sequence_number: u64,
    pub largest_sequence_number: u64,
    pub metadata: NewFileMetadata,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeletedFile {
    pub level: u32,
    pub file_number: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompactCursor {
    pub level: u32,
    pub key: InternalKeyMetadata,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ColumnFamilyAction {
    Add { name: Vec<u8> },
    Drop,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IgnoredField {
    pub tag: u32,
    pub encoded_length: u32,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct VersionEdit {
    pub decoded_tag_count: u32,
    pub comparator: Option<Vec<u8>>,
    pub log_number: Option<u64>,
    pub previous_log_number: Option<u64>,
    pub next_file_number: Option<u64>,
    pub last_sequence: Option<u64>,
    pub min_log_number_to_keep: Option<u64>,
    pub max_column_family_id: Option<u32>,
    pub column_family_id: u32,
    pub column_family_action: Option<ColumnFamilyAction>,
    pub atomic_group_remaining: Option<u32>,
    pub compact_cursors: Vec<CompactCursor>,
    pub deleted_files: Vec<DeletedFile>,
    pub new_files: Vec<NewFile>,
    pub ignored_fields: Vec<IgnoredField>,
}
