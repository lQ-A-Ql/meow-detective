const MIB: usize = 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LogDecodeLimits {
    pub max_file_bytes: usize,
    pub max_logical_record_bytes: usize,
    pub max_logical_records: usize,
}

impl Default for LogDecodeLimits {
    fn default() -> Self {
        Self {
            max_file_bytes: 64 * MIB,
            max_logical_record_bytes: 16 * MIB,
            max_logical_records: 1_000_000,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WriteBatchLimits {
    pub max_batch_bytes: usize,
    pub max_mutations: usize,
    pub max_auxiliary_records: usize,
    pub max_key_bytes: usize,
    pub max_value_bytes: usize,
}

impl Default for WriteBatchLimits {
    fn default() -> Self {
        Self {
            max_batch_bytes: 16 * MIB,
            max_mutations: 1_000_000,
            max_auxiliary_records: 100_000,
            max_key_bytes: MIB,
            max_value_bytes: 64 * MIB,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VersionEditLimits {
    pub max_edit_bytes: usize,
    pub max_tags: usize,
    pub max_string_bytes: usize,
    pub max_internal_key_bytes: usize,
    pub max_custom_fields_per_file: usize,
    pub max_custom_field_bytes: usize,
    pub max_file_mutations: usize,
    pub max_compact_cursors: usize,
    pub max_level: u32,
}

impl Default for VersionEditLimits {
    fn default() -> Self {
        Self {
            max_edit_bytes: 16 * MIB,
            max_tags: 100_000,
            max_string_bytes: MIB,
            max_internal_key_bytes: MIB,
            max_custom_fields_per_file: 256,
            max_custom_field_bytes: MIB,
            max_file_mutations: 100_000,
            max_compact_cursors: 10_000,
            max_level: 63,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReplayLimits {
    pub max_column_families: usize,
    pub max_live_files: usize,
}

impl Default for ReplayLimits {
    fn default() -> Self {
        Self {
            max_column_families: 65_536,
            max_live_files: 1_000_000,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ManifestDecodeLimits {
    pub log: LogDecodeLimits,
    pub version_edit: VersionEditLimits,
    pub replay: ReplayLimits,
}
