use std::collections::HashSet;

use rusqlite::{params, Connection};

use crate::connection::{DbError, DbResult};

use super::ceph_rocksdb_repo::CephRocksdbAggregate;

pub const ROCKSDB_BLOCK_BASED_TABLE_MAGIC_HEX: &str = "88e241b785f4cff7";
const SUMMARY_FIELDS: [&str; 5] = [
    "version",
    "complete",
    "scannedEntries",
    "scannedDecompressedBytes",
    "buckets",
];
const BUCKET_FIELDS: [&str; 4] = ["name", "count", "minKeyLength", "maxKeyLength"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CephRocksdbSstRecord {
    pub inventory_id: String,
    pub file_number: u64,
    pub column_family_id: u32,
    pub level: u32,
    pub bluefs_path: String,
    pub file_size: u64,
    pub table_magic_hex: String,
    pub format_version: u32,
    pub checksum_type: String,
    pub metaindex_offset: u64,
    pub metaindex_size: u64,
    pub index_offset: u64,
    pub index_size: u64,
    pub data_block_count: u64,
    pub entry_count: u64,
    pub deletion_count: u64,
    pub merge_operand_count: u64,
    pub range_deletion_count: u64,
    pub raw_key_size: u64,
    pub raw_value_size: u64,
    pub data_size: u64,
    pub properties_index_size: u64,
    pub filter_size: u64,
    pub compression_name: String,
    pub comparator_name: String,
    pub column_family_name: String,
    pub original_file_number: u64,
    pub db_identity: Option<String>,
    pub db_session_identity: Option<String>,
    pub key_space_summary_version: u32,
    pub key_space_summary_json: String,
    pub scan_complete: bool,
}

pub struct CephRocksdbSstRepo<'a> {
    conn: &'a Connection,
}

impl<'a> CephRocksdbSstRepo<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    pub fn find_for_inventory(&self, inventory_id: &str) -> DbResult<Vec<CephRocksdbSstRecord>> {
        let mut statement = self.conn.prepare(
            "SELECT inventory_id, file_number, column_family_id, level, bluefs_path,
                    file_size, table_magic_hex, format_version, checksum_type,
                    metaindex_offset, metaindex_size, index_offset, index_size,
                    data_block_count, entry_count, deletion_count, merge_operand_count,
                    range_deletion_count, raw_key_size, raw_value_size, data_size,
                    properties_index_size, filter_size, compression_name, comparator_name,
                    column_family_name, original_file_number, db_identity,
                    db_session_identity, key_space_summary_version,
                    key_space_summary_json, scan_complete
             FROM ceph_rocksdb_sst_inventory
             WHERE inventory_id = ?1
             ORDER BY column_family_id, level, file_number",
        )?;
        let rows = statement.query_map(params![inventory_id], map_record)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }
}

pub(super) fn replace_for_inventory_on(
    conn: &Connection,
    inventory_id: &str,
    records: &[CephRocksdbSstRecord],
) -> DbResult<()> {
    conn.execute(
        "DELETE FROM ceph_rocksdb_sst_inventory WHERE inventory_id = ?1",
        params![inventory_id],
    )?;
    let mut statement = conn.prepare_cached(
        "INSERT INTO ceph_rocksdb_sst_inventory (
            inventory_id, file_number, column_family_id, level, bluefs_path,
            file_size, table_magic_hex, format_version, checksum_type,
            metaindex_offset, metaindex_size, index_offset, index_size,
            data_block_count, entry_count, deletion_count, merge_operand_count,
            range_deletion_count, raw_key_size, raw_value_size, data_size,
            properties_index_size, filter_size, compression_name, comparator_name,
            column_family_name, original_file_number, db_identity,
            db_session_identity, key_space_summary_version,
            key_space_summary_json, scan_complete
         ) VALUES (
            ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14,
            ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24, ?25, ?26,
            ?27, ?28, ?29, ?30, ?31, ?32
         )",
    )?;
    for record in records {
        statement.execute(params![
            record.inventory_id,
            record.file_number,
            record.column_family_id,
            record.level,
            record.bluefs_path,
            record.file_size,
            record.table_magic_hex,
            record.format_version,
            record.checksum_type,
            record.metaindex_offset,
            record.metaindex_size,
            record.index_offset,
            record.index_size,
            record.data_block_count,
            record.entry_count,
            record.deletion_count,
            record.merge_operand_count,
            record.range_deletion_count,
            record.raw_key_size,
            record.raw_value_size,
            record.data_size,
            record.properties_index_size,
            record.filter_size,
            record.compression_name,
            record.comparator_name,
            record.column_family_name,
            record.original_file_number,
            record.db_identity,
            record.db_session_identity,
            record.key_space_summary_version,
            record.key_space_summary_json,
            record.scan_complete,
        ])?;
    }
    Ok(())
}

pub(super) fn validate_replacement(
    rocksdb: &CephRocksdbAggregate,
    records: &[CephRocksdbSstRecord],
) -> DbResult<()> {
    let manifest = &rocksdb.manifest;
    let live_files = rocksdb
        .live_ssts
        .iter()
        .map(|file| (file.file_number, file))
        .collect::<std::collections::HashMap<_, _>>();
    let column_families = rocksdb
        .column_families
        .iter()
        .map(|column_family| (column_family.column_family_id, column_family))
        .collect::<std::collections::HashMap<_, _>>();
    let mut file_numbers = HashSet::new();
    let mut paths = HashSet::new();
    for record in records {
        let live_file = live_files.get(&record.file_number).ok_or_else(|| {
            DbError::System("SST inventory references a non-live RocksDB file".to_string())
        })?;
        let column_family = column_families
            .get(&record.column_family_id)
            .ok_or_else(|| {
                DbError::System("SST inventory references an unknown column family".to_string())
            })?;
        if record.inventory_id != manifest.inventory_id
            || live_file.column_family_id != record.column_family_id
            || live_file.level != record.level
            || live_file.file_size != record.file_size
            || column_family.dropped
            || column_family.name != record.column_family_name
            || column_family.comparator_name != record.comparator_name
            || record.bluefs_path != format!("db/{:06}.sst", record.file_number)
            || record.table_magic_hex != ROCKSDB_BLOCK_BASED_TABLE_MAGIC_HEX
            || record.format_version != 5
            || record.original_file_number != record.file_number
            || record.checksum_type != "xxh3"
            || manifest
                .identity_uuid
                .as_deref()
                .is_some_and(|identity| record.db_identity.as_deref() != Some(identity))
            || !valid_record(record)
            || !file_numbers.insert(record.file_number)
            || !paths.insert(record.bluefs_path.as_str())
        {
            return Err(DbError::System(
                "RocksDB SST inventory is incomplete or inconsistent".to_string(),
            ));
        }
    }
    if records.len() != rocksdb.live_ssts.len() {
        return Err(DbError::System(
            "RocksDB SST inventory does not cover the complete live set".to_string(),
        ));
    }
    Ok(())
}

fn valid_record(record: &CephRocksdbSstRecord) -> bool {
    let metaindex_end = record
        .metaindex_offset
        .checked_add(record.metaindex_size)
        .and_then(|end| end.checked_add(5));
    let index_end = record
        .index_offset
        .checked_add(record.index_size)
        .and_then(|end| end.checked_add(5));
    record.file_number > 0
        && record.file_size > 0
        && record.metaindex_size > 0
        && record.index_size > 0
        && metaindex_end.is_some_and(|end| end <= record.file_size)
        && index_end.is_some_and(|end| end <= record.file_size)
        && record.deletion_count <= record.entry_count
        && record.merge_operand_count <= record.entry_count
        && record.range_deletion_count <= record.entry_count
        && record.data_block_count > 0
        && record.entry_count > 0
        && record.data_size > 0
        && record.data_size <= record.metaindex_offset
        && record.data_size <= record.index_offset
        && record.properties_index_size > 0
        && record.key_space_summary_version == 1
        && record.scan_complete
        && valid_text(&record.checksum_type)
        && matches!(
            record.compression_name.as_str(),
            "NoCompression" | "LZ4" | "LZ4HC"
        )
        && valid_text(&record.comparator_name)
        && valid_text(&record.column_family_name)
        && valid_optional_text(record.db_identity.as_deref())
        && valid_optional_text(record.db_session_identity.as_deref())
        && serde_json::from_str::<serde_json::Value>(&record.key_space_summary_json).is_ok_and(
            |value| {
                record
                    .entry_count
                    .checked_sub(record.range_deletion_count)
                    .is_some_and(|expected| valid_key_space_summary(value, expected))
            },
        )
}

fn valid_key_space_summary(value: serde_json::Value, expected_entries: u64) -> bool {
    let Some(object) = value.as_object() else {
        return false;
    };
    exact_fields(object, &SUMMARY_FIELDS)
        && object.get("version").and_then(|value| value.as_u64()) == Some(1)
        && object.get("complete").and_then(|value| value.as_bool()) == Some(true)
        && object
            .get("scannedEntries")
            .and_then(|value| value.as_u64())
            == Some(expected_entries)
        && object
            .get("scannedDecompressedBytes")
            .and_then(|value| value.as_u64())
            .is_some()
        && object
            .get("buckets")
            .and_then(|value| value.as_array())
            .is_some_and(|buckets| valid_key_space_buckets(object, buckets))
}

fn valid_key_space_buckets(
    summary: &serde_json::Map<String, serde_json::Value>,
    buckets: &[serde_json::Value],
) -> bool {
    let mut names = HashSet::new();
    let mut total = 0u64;
    for bucket in buckets {
        let Some(bucket) = bucket.as_object() else {
            return false;
        };
        if !exact_fields(bucket, &BUCKET_FIELDS) {
            return false;
        }
        let Some(name) = bucket.get("name").and_then(|value| value.as_str()) else {
            return false;
        };
        let Some(count) = bucket.get("count").and_then(|value| value.as_u64()) else {
            return false;
        };
        let Some(min_length) = bucket.get("minKeyLength").and_then(|value| value.as_u64()) else {
            return false;
        };
        let Some(max_length) = bucket.get("maxKeyLength").and_then(|value| value.as_u64()) else {
            return false;
        };
        if !valid_summary_name(name)
            || !names.insert(name)
            || count == 0
            || min_length > max_length
            || total.checked_add(count).is_none()
        {
            return false;
        }
        total += count;
    }
    summary
        .get("scannedEntries")
        .and_then(|value| value.as_u64())
        == Some(total)
}

fn exact_fields(object: &serde_json::Map<String, serde_json::Value>, allowed: &[&str]) -> bool {
    object.len() == allowed.len() && object.keys().all(|key| allowed.contains(&key.as_str()))
}

fn valid_summary_name(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
}

fn valid_text(value: &str) -> bool {
    !value.is_empty() && !value.contains('\0')
}

fn valid_optional_text(value: Option<&str>) -> bool {
    value.is_none_or(valid_text)
}

fn map_record(row: &rusqlite::Row<'_>) -> rusqlite::Result<CephRocksdbSstRecord> {
    Ok(CephRocksdbSstRecord {
        inventory_id: row.get(0)?,
        file_number: row.get(1)?,
        column_family_id: row.get(2)?,
        level: row.get(3)?,
        bluefs_path: row.get(4)?,
        file_size: row.get(5)?,
        table_magic_hex: row.get(6)?,
        format_version: row.get(7)?,
        checksum_type: row.get(8)?,
        metaindex_offset: row.get(9)?,
        metaindex_size: row.get(10)?,
        index_offset: row.get(11)?,
        index_size: row.get(12)?,
        data_block_count: row.get(13)?,
        entry_count: row.get(14)?,
        deletion_count: row.get(15)?,
        merge_operand_count: row.get(16)?,
        range_deletion_count: row.get(17)?,
        raw_key_size: row.get(18)?,
        raw_value_size: row.get(19)?,
        data_size: row.get(20)?,
        properties_index_size: row.get(21)?,
        filter_size: row.get(22)?,
        compression_name: row.get(23)?,
        comparator_name: row.get(24)?,
        column_family_name: row.get(25)?,
        original_file_number: row.get(26)?,
        db_identity: row.get(27)?,
        db_session_identity: row.get(28)?,
        key_space_summary_version: row.get(29)?,
        key_space_summary_json: row.get(30)?,
        scan_complete: row.get(31)?,
    })
}
