use std::collections::HashSet;

use crate::cursor::WireCursor;
use crate::{Result, RocksDbWireError};

use super::restart::{visit_restart_block, ValueEncoding};
use super::{SstReadOptions, TableProperties};

const ORIGINAL_FILE_NUMBER: &[u8] = b"rocksdb.original.file.number";
const DATA_SIZE: &[u8] = b"rocksdb.data.size";
const INDEX_SIZE: &[u8] = b"rocksdb.index.size";
const INDEX_PARTITIONS: &[u8] = b"rocksdb.index.partitions";
const INDEX_KEY_IS_USER: &[u8] = b"rocksdb.index.key.is.user.key";
const INDEX_VALUE_IS_DELTA: &[u8] = b"rocksdb.index.value.is.delta.encoded";
const FILTER_SIZE: &[u8] = b"rocksdb.filter.size";
const RAW_KEY_SIZE: &[u8] = b"rocksdb.raw.key.size";
const RAW_VALUE_SIZE: &[u8] = b"rocksdb.raw.value.size";
const NUM_DATA_BLOCKS: &[u8] = b"rocksdb.num.data.blocks";
const NUM_ENTRIES: &[u8] = b"rocksdb.num.entries";
const DELETED_KEYS: &[u8] = b"rocksdb.deleted.keys";
const MERGE_OPERANDS: &[u8] = b"rocksdb.merge.operands";
const RANGE_DELETIONS: &[u8] = b"rocksdb.num.range-deletions";
const FORMAT_VERSION: &[u8] = b"rocksdb.format.version";
const COLUMN_FAMILY_ID: &[u8] = b"rocksdb.column.family.id";
const COLUMN_FAMILY_NAME: &[u8] = b"rocksdb.column.family.name";
const COMPARATOR: &[u8] = b"rocksdb.comparator";
const COMPRESSION: &[u8] = b"rocksdb.compression";
const DB_IDENTITY: &[u8] = b"rocksdb.creating.db.identity";
const DB_SESSION_IDENTITY: &[u8] = b"rocksdb.creating.session.identity";
const INDEX_TYPE: &[u8] = b"rocksdb.block.based.table.index.type";
const EXTERNAL_SST_VERSION: &[u8] = b"rocksdb.external_sst_file.version";
const EXTERNAL_SST_GLOBAL_SEQUENCE: &[u8] = b"rocksdb.external_sst_file.global_seqno";

pub(crate) fn parse_properties(block: &[u8], options: SstReadOptions) -> Result<TableProperties> {
    let options = SstReadOptions {
        max_entries_per_block: options.max_entries_per_block.min(options.max_properties),
        ..options
    };
    let mut builder = PropertyBuilder::default();
    let mut names = HashSet::new();
    let mut previous_name = Vec::new();
    let count = visit_restart_block(block, ValueEncoding::Full, options, |entry| {
        if !previous_name.is_empty() && previous_name.as_slice() >= entry.key {
            return Err(RocksDbWireError::InvalidSstProperty {
                context: "SST properties block",
                reason: "properties are not strictly ordered",
            });
        }
        previous_name.clear();
        previous_name.extend_from_slice(entry.key);
        let name = property_name(entry.key)?;
        if !names.insert(name) {
            return Err(RocksDbWireError::DuplicateSstProperty);
        }
        builder.add(entry.key, entry.value)
    })?;
    debug_assert!(count <= options.max_properties);
    builder.finish()
}

#[derive(Default)]
struct PropertyBuilder {
    num_data_blocks: Option<u64>,
    num_entries: Option<u64>,
    deleted_keys: Option<u64>,
    merge_operands: Option<u64>,
    num_range_deletions: Option<u64>,
    raw_key_size: Option<u64>,
    raw_value_size: Option<u64>,
    data_size: Option<u64>,
    index_size: Option<u64>,
    filter_size: Option<u64>,
    format_version: Option<u64>,
    compression_name: Option<String>,
    comparator_name: Option<String>,
    column_family_name: Option<String>,
    column_family_id: Option<u64>,
    original_file_number: Option<u64>,
    db_identity: Option<String>,
    db_session_identity: Option<String>,
    index_key_is_user: Option<u64>,
    index_value_is_delta: Option<u64>,
    index_partitions: Option<u64>,
    index_type: Option<u32>,
    ignored_user_property_count: u32,
}

impl PropertyBuilder {
    fn add(&mut self, name: &[u8], value: &[u8]) -> Result<()> {
        match name {
            NUM_DATA_BLOCKS => self.num_data_blocks = Some(numeric(name, value)?),
            NUM_ENTRIES => self.num_entries = Some(numeric(name, value)?),
            DELETED_KEYS => self.deleted_keys = Some(numeric(name, value)?),
            MERGE_OPERANDS => self.merge_operands = Some(numeric(name, value)?),
            RANGE_DELETIONS => self.num_range_deletions = Some(numeric(name, value)?),
            RAW_KEY_SIZE => self.raw_key_size = Some(numeric(name, value)?),
            RAW_VALUE_SIZE => self.raw_value_size = Some(numeric(name, value)?),
            DATA_SIZE => self.data_size = Some(numeric(name, value)?),
            INDEX_SIZE => self.index_size = Some(numeric(name, value)?),
            FILTER_SIZE => self.filter_size = Some(numeric(name, value)?),
            FORMAT_VERSION => self.format_version = Some(numeric(name, value)?),
            COLUMN_FAMILY_ID => self.column_family_id = Some(numeric(name, value)?),
            ORIGINAL_FILE_NUMBER => self.original_file_number = Some(numeric(name, value)?),
            INDEX_KEY_IS_USER => self.index_key_is_user = Some(numeric(name, value)?),
            INDEX_VALUE_IS_DELTA => self.index_value_is_delta = Some(numeric(name, value)?),
            INDEX_PARTITIONS => self.index_partitions = Some(numeric(name, value)?),
            COMPRESSION => self.compression_name = Some(text(name, value)?),
            COMPARATOR => self.comparator_name = Some(text(name, value)?),
            COLUMN_FAMILY_NAME => self.column_family_name = Some(text(name, value)?),
            DB_IDENTITY => self.db_identity = Some(text(name, value)?),
            DB_SESSION_IDENTITY => self.db_session_identity = Some(text(name, value)?),
            INDEX_TYPE => self.index_type = Some(fixed32(name, value)?),
            EXTERNAL_SST_VERSION | EXTERNAL_SST_GLOBAL_SEQUENCE => {
                return Err(RocksDbWireError::UnsupportedSstFeature {
                    feature: "external SST global sequence",
                    value: 1,
                });
            }
            _ if !name.starts_with(b"rocksdb.") => self.ignored_user_property_count += 1,
            _ => {}
        }
        Ok(())
    }

    fn finish(self) -> Result<TableProperties> {
        validate_features(&self)?;
        Ok(TableProperties {
            num_data_blocks: required(self.num_data_blocks, "rocksdb.num.data.blocks")?,
            num_entries: required(self.num_entries, "rocksdb.num.entries")?,
            deleted_keys: required(self.deleted_keys, "rocksdb.deleted.keys")?,
            merge_operands: required(self.merge_operands, "rocksdb.merge.operands")?,
            num_range_deletions: required(self.num_range_deletions, "rocksdb.num.range-deletions")?,
            raw_key_size: required(self.raw_key_size, "rocksdb.raw.key.size")?,
            raw_value_size: required(self.raw_value_size, "rocksdb.raw.value.size")?,
            data_size: required(self.data_size, "rocksdb.data.size")?,
            index_size: required(self.index_size, "rocksdb.index.size")?,
            filter_size: required(self.filter_size, "rocksdb.filter.size")?,
            properties_format_version: u32::try_from(required(
                self.format_version,
                "rocksdb.format.version",
            )?)
            .map_err(|_| invalid_known("format version property", "value exceeds u32"))?,
            index_key_is_user_key: required(
                self.index_key_is_user,
                "rocksdb.index.key.is.user.key",
            )? == 1,
            index_value_is_delta_encoded: required(
                self.index_value_is_delta,
                "rocksdb.index.value.is.delta.encoded",
            )? == 1,
            index_type: required(self.index_type, "rocksdb.block.based.table.index.type")?,
            index_partitions: self.index_partitions.unwrap_or(0),
            compression_name: required(self.compression_name, "rocksdb.compression")?,
            comparator_name: required(self.comparator_name, "rocksdb.comparator")?,
            column_family_name: required(self.column_family_name, "rocksdb.column.family.name")?,
            column_family_id: u32::try_from(required(
                self.column_family_id,
                "rocksdb.column.family.id",
            )?)
            .map_err(|_| invalid_known("column family ID property", "value exceeds u32"))?,
            original_file_number: required(
                self.original_file_number,
                "rocksdb.original.file.number",
            )?,
            db_identity: self.db_identity,
            db_session_identity: self.db_session_identity,
            ignored_user_property_count: self.ignored_user_property_count,
        })
    }
}

fn validate_features(builder: &PropertyBuilder) -> Result<()> {
    let format = required(builder.format_version, "rocksdb.format.version")?;
    ensure_feature("table properties format version", format, 0)?;
    ensure_feature("index partitions", builder.index_partitions.unwrap_or(0), 0)?;
    ensure_boolean(
        "index key is user key",
        required(builder.index_key_is_user, "rocksdb.index.key.is.user.key")?,
    )?;
    ensure_feature(
        "index value delta encoding",
        required(
            builder.index_value_is_delta,
            "rocksdb.index.value.is.delta.encoded",
        )?,
        1,
    )?;
    ensure_feature(
        "block-based table index type",
        u64::from(required(
            builder.index_type,
            "rocksdb.block.based.table.index.type",
        )?),
        0,
    )
}

fn ensure_boolean(feature: &'static str, value: u64) -> Result<()> {
    if value > 1 {
        return Err(RocksDbWireError::UnsupportedSstFeature { feature, value });
    }
    Ok(())
}

fn ensure_feature(feature: &'static str, value: u64, expected: u64) -> Result<()> {
    if value != expected {
        return Err(RocksDbWireError::UnsupportedSstFeature { feature, value });
    }
    Ok(())
}

fn numeric(_name: &[u8], value: &[u8]) -> Result<u64> {
    let mut cursor = WireCursor::new(value);
    let decoded = cursor.read_varint_u64("SST numeric property")?;
    if !cursor.is_empty() {
        return Err(invalid(
            "numeric SST property",
            "property has trailing bytes",
        ));
    }
    Ok(decoded)
}

fn fixed32(_name: &[u8], value: &[u8]) -> Result<u32> {
    if value.len() != 4 {
        return Err(invalid(
            "fixed32 SST property",
            "property has invalid length",
        ));
    }
    Ok(u32::from_le_bytes(value.try_into().map_err(|_| {
        invalid("fixed32 SST property", "property has invalid width")
    })?))
}

fn text(_name: &[u8], value: &[u8]) -> Result<String> {
    if value.is_empty() || value.len() > 4096 || value.contains(&0) {
        return Err(invalid(
            "string SST property",
            "property is empty, contains NUL, or is too long",
        ));
    }
    String::from_utf8(value.to_vec())
        .map_err(|_| invalid("string SST property", "property is not UTF-8"))
}

fn property_name(value: &[u8]) -> Result<String> {
    if value.is_empty() || value.len() > 4096 || value.contains(&0) {
        return Err(invalid("SST property name", "name is invalid"));
    }
    String::from_utf8(value.to_vec()).map_err(|_| invalid("SST property name", "name is not UTF-8"))
}

fn required<T>(value: Option<T>, name: &'static str) -> Result<T> {
    value.ok_or(RocksDbWireError::MissingSstProperty { name })
}

fn invalid(context: &'static str, reason: &'static str) -> RocksDbWireError {
    RocksDbWireError::InvalidSstProperty { context, reason }
}

fn invalid_known(context: &'static str, reason: &'static str) -> RocksDbWireError {
    invalid(context, reason)
}
