use std::collections::{HashMap, HashSet};

use rusqlite::{params, Connection, OptionalExtension};

use crate::connection::{DbError, DbResult};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CephRocksdbManifestRecord {
    pub inventory_id: String,
    pub data_source_id: String,
    pub active_manifest_path: String,
    pub identity_uuid: Option<String>,
    pub manifest_file_number: u64,
    pub manifest_file_size: u64,
    pub logical_edit_count: u32,
    pub comparator_name: String,
    pub last_sequence: u64,
    pub next_file_number: u64,
    pub log_number: u64,
    pub prev_log_number: u64,
    pub max_column_family_id: u32,
    pub min_log_number_to_keep: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CephRocksdbColumnFamilyRecord {
    pub inventory_id: String,
    pub column_family_id: u32,
    pub name: String,
    pub comparator_name: String,
    pub dropped: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CephRocksdbLiveSstRecord {
    pub inventory_id: String,
    pub column_family_id: u32,
    pub level: u32,
    pub file_number: u64,
    pub path_id: u32,
    pub format: String,
    pub file_size: u64,
    pub smallest_sequence: Option<u64>,
    pub largest_sequence: Option<u64>,
    pub smallest_internal_key_length: u32,
    pub largest_internal_key_length: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CephRocksdbAggregate {
    pub manifest: CephRocksdbManifestRecord,
    pub column_families: Vec<CephRocksdbColumnFamilyRecord>,
    pub live_ssts: Vec<CephRocksdbLiveSstRecord>,
}

pub struct CephRocksdbRepo<'a> {
    conn: &'a Connection,
}

impl<'a> CephRocksdbRepo<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    pub fn find_manifest(&self, inventory_id: &str) -> DbResult<Option<CephRocksdbManifestRecord>> {
        self.conn
            .query_row(
                "SELECT inventory_id, data_source_id, active_manifest_path, identity_uuid,
                        manifest_file_number, manifest_file_size, logical_edit_count,
                        comparator_name, last_sequence, next_file_number, log_number,
                        prev_log_number, max_column_family_id, min_log_number_to_keep
                 FROM ceph_rocksdb_manifests
                 WHERE inventory_id = ?1",
                params![inventory_id],
                map_manifest,
            )
            .optional()
            .map_err(Into::into)
    }

    pub fn find_by_data_source(
        &self,
        data_source_id: &str,
    ) -> DbResult<Vec<CephRocksdbManifestRecord>> {
        let mut statement = self.conn.prepare(
            "SELECT inventory_id, data_source_id, active_manifest_path, identity_uuid,
                    manifest_file_number, manifest_file_size, logical_edit_count,
                    comparator_name, last_sequence, next_file_number, log_number,
                    prev_log_number, max_column_family_id, min_log_number_to_keep
             FROM ceph_rocksdb_manifests
             WHERE data_source_id = ?1
             ORDER BY inventory_id",
        )?;
        let rows = statement.query_map(params![data_source_id], map_manifest)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    pub fn find_column_families(
        &self,
        inventory_id: &str,
    ) -> DbResult<Vec<CephRocksdbColumnFamilyRecord>> {
        let mut statement = self.conn.prepare(
            "SELECT inventory_id, column_family_id, name, comparator_name, dropped
             FROM ceph_rocksdb_column_families
             WHERE inventory_id = ?1
             ORDER BY column_family_id",
        )?;
        let rows = statement.query_map(params![inventory_id], map_column_family)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    pub fn find_live_ssts(&self, inventory_id: &str) -> DbResult<Vec<CephRocksdbLiveSstRecord>> {
        let mut statement = self.conn.prepare(
            "SELECT inventory_id, column_family_id, level, file_number, path_id,
                    format, file_size, smallest_sequence, largest_sequence,
                    smallest_internal_key_length, largest_internal_key_length
             FROM ceph_rocksdb_live_files
             WHERE inventory_id = ?1
             ORDER BY column_family_id, level, file_number",
        )?;
        let rows = statement.query_map(params![inventory_id], map_live_sst)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    pub fn find_aggregate(&self, inventory_id: &str) -> DbResult<Option<CephRocksdbAggregate>> {
        let Some(manifest) = self.find_manifest(inventory_id)? else {
            return Ok(None);
        };
        Ok(Some(CephRocksdbAggregate {
            manifest,
            column_families: self.find_column_families(inventory_id)?,
            live_ssts: self.find_live_ssts(inventory_id)?,
        }))
    }
}

pub(super) fn replace_for_inventory_on(
    conn: &Connection,
    records: &CephRocksdbAggregate,
) -> DbResult<()> {
    conn.execute(
        "DELETE FROM ceph_rocksdb_manifests WHERE inventory_id = ?1",
        params![records.manifest.inventory_id],
    )?;
    insert_manifest(conn, &records.manifest)?;
    insert_column_families(conn, &records.column_families)?;
    insert_live_ssts(conn, &records.live_ssts)
}

pub(super) fn validate_replacement(records: &CephRocksdbAggregate) -> DbResult<()> {
    validate_manifest(&records.manifest)?;
    let inventory_id = records.manifest.inventory_id.as_str();
    let mut column_family_states = HashMap::new();
    for record in &records.column_families {
        validate_column_family(inventory_id, record)?;
        if column_family_states
            .insert(record.column_family_id, record.dropped)
            .is_some()
        {
            return Err(DbError::System(
                "RocksDB column family id is duplicated".to_string(),
            ));
        }
    }
    if column_family_states.is_empty()
        || column_family_states.get(&0) != Some(&false)
        || column_family_states
            .iter()
            .any(|(id, _)| *id > records.manifest.max_column_family_id)
    {
        return Err(DbError::System(
            "RocksDB column family inventory is incomplete or out of range".to_string(),
        ));
    }
    let default_comparator = records
        .column_families
        .iter()
        .find(|record| record.column_family_id == 0)
        .map(|record| record.comparator_name.as_str());
    if default_comparator != Some(records.manifest.comparator_name.as_str()) {
        return Err(DbError::System(
            "RocksDB manifest comparator does not match the default column family".to_string(),
        ));
    }
    validate_live_ssts(records, &column_family_states)
}

fn validate_manifest(record: &CephRocksdbManifestRecord) -> DbResult<()> {
    if record.inventory_id.is_empty()
        || record.data_source_id.is_empty()
        || !is_relative_bluefs_path(&record.active_manifest_path)
        || record.identity_uuid.as_ref().is_some_and(|value| {
            uuid::Uuid::parse_str(value)
                .map(|uuid| uuid.to_string() != *value)
                .unwrap_or(true)
        })
        || parse_manifest_number(&record.active_manifest_path) != Some(record.manifest_file_number)
        || record.manifest_file_number == 0
        || record.manifest_file_size == 0
        || record.logical_edit_count == 0
        || record.comparator_name.is_empty()
        || record.comparator_name.contains('\0')
        || record.next_file_number == 0
        || record.manifest_file_number >= record.next_file_number
        || record.log_number >= record.next_file_number
        || record.prev_log_number >= record.next_file_number
        || record
            .min_log_number_to_keep
            .is_some_and(|value| value >= record.next_file_number)
        || record.last_sequence > ((1u64 << 56) - 1)
    {
        return Err(DbError::System(
            "RocksDB manifest inventory is incomplete or inconsistent".to_string(),
        ));
    }
    Ok(())
}

fn validate_column_family(
    inventory_id: &str,
    record: &CephRocksdbColumnFamilyRecord,
) -> DbResult<()> {
    if record.inventory_id != inventory_id || record.name.is_empty() || record.name.contains('\0') {
        return Err(DbError::System(
            "RocksDB column family metadata is invalid".to_string(),
        ));
    }
    if record.comparator_name.is_empty() || record.comparator_name.contains('\0') {
        return Err(DbError::System(
            "RocksDB column family comparator is invalid".to_string(),
        ));
    }
    Ok(())
}

fn validate_live_ssts(
    records: &CephRocksdbAggregate,
    column_family_states: &HashMap<u32, bool>,
) -> DbResult<()> {
    let inventory_id = records.manifest.inventory_id.as_str();
    let mut file_numbers = HashSet::new();
    for record in &records.live_ssts {
        if record.inventory_id != inventory_id
            || column_family_states.get(&record.column_family_id) != Some(&false)
            || record.file_number == 0
            || record.path_id > 3
            || !matches!(
                record.format.as_str(),
                "newFile" | "newFile2" | "newFile3" | "newFile4"
            )
            || record.file_size == 0
            || record.file_number >= records.manifest.next_file_number
            || !valid_sequence_range(
                &record.format,
                record.smallest_sequence,
                record.largest_sequence,
                records.manifest.last_sequence,
            )
            || record.smallest_internal_key_length < 8
            || record.largest_internal_key_length < 8
            || !file_numbers.insert(record.file_number)
        {
            return Err(DbError::System(
                "RocksDB live SST inventory is invalid or duplicated".to_string(),
            ));
        }
    }
    Ok(())
}

fn valid_sequence_range(
    format: &str,
    smallest: Option<u64>,
    largest: Option<u64>,
    manifest: u64,
) -> bool {
    match (format, smallest, largest) {
        ("newFile", None, None) => true,
        ("newFile2" | "newFile3" | "newFile4", Some(smallest), Some(largest)) => {
            smallest <= largest && largest <= manifest
        }
        _ => false,
    }
}

fn is_relative_bluefs_path(value: &str) -> bool {
    !value.is_empty()
        && !value.contains('\0')
        && !value.contains('\\')
        && !value.starts_with('/')
        && !value.split('/').any(|component| component == "..")
}

fn parse_manifest_number(value: &str) -> Option<u64> {
    let digits = value.strip_prefix("db/MANIFEST-")?;
    (!digits.is_empty() && digits.bytes().all(|byte| byte.is_ascii_digit()))
        .then(|| digits.parse().ok())
        .flatten()
}

fn insert_manifest(conn: &Connection, record: &CephRocksdbManifestRecord) -> DbResult<()> {
    conn.execute(
        "INSERT INTO ceph_rocksdb_manifests (
            inventory_id, data_source_id, active_manifest_path, identity_uuid,
            manifest_file_number, manifest_file_size, logical_edit_count,
            comparator_name, last_sequence, next_file_number, log_number,
            prev_log_number, max_column_family_id, min_log_number_to_keep
         ) VALUES (
            ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14
         )",
        params![
            record.inventory_id,
            record.data_source_id,
            record.active_manifest_path,
            record.identity_uuid,
            record.manifest_file_number,
            record.manifest_file_size,
            record.logical_edit_count,
            record.comparator_name,
            record.last_sequence,
            record.next_file_number,
            record.log_number,
            record.prev_log_number,
            record.max_column_family_id,
            record.min_log_number_to_keep,
        ],
    )?;
    Ok(())
}

fn insert_column_families(
    conn: &Connection,
    records: &[CephRocksdbColumnFamilyRecord],
) -> DbResult<()> {
    let mut statement = conn.prepare_cached(
        "INSERT INTO ceph_rocksdb_column_families (
            inventory_id, column_family_id, name, comparator_name, dropped
         ) VALUES (?1, ?2, ?3, ?4, ?5)",
    )?;
    for record in records {
        statement.execute(params![
            record.inventory_id,
            record.column_family_id,
            record.name,
            record.comparator_name,
            record.dropped,
        ])?;
    }
    Ok(())
}

fn insert_live_ssts(conn: &Connection, records: &[CephRocksdbLiveSstRecord]) -> DbResult<()> {
    let mut statement = conn.prepare_cached(
        "INSERT INTO ceph_rocksdb_live_files (
            inventory_id, column_family_id, level, file_number, path_id,
            format, file_size, smallest_sequence, largest_sequence,
            smallest_internal_key_length, largest_internal_key_length
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
    )?;
    for record in records {
        statement.execute(params![
            record.inventory_id,
            record.column_family_id,
            record.level,
            record.file_number,
            record.path_id,
            record.format,
            record.file_size,
            record.smallest_sequence,
            record.largest_sequence,
            record.smallest_internal_key_length,
            record.largest_internal_key_length,
        ])?;
    }
    Ok(())
}

fn map_manifest(row: &rusqlite::Row<'_>) -> rusqlite::Result<CephRocksdbManifestRecord> {
    Ok(CephRocksdbManifestRecord {
        inventory_id: row.get(0)?,
        data_source_id: row.get(1)?,
        active_manifest_path: row.get(2)?,
        identity_uuid: row.get(3)?,
        manifest_file_number: row.get(4)?,
        manifest_file_size: row.get(5)?,
        logical_edit_count: row.get(6)?,
        comparator_name: row.get(7)?,
        last_sequence: row.get(8)?,
        next_file_number: row.get(9)?,
        log_number: row.get(10)?,
        prev_log_number: row.get(11)?,
        max_column_family_id: row.get(12)?,
        min_log_number_to_keep: row.get(13)?,
    })
}

fn map_column_family(row: &rusqlite::Row<'_>) -> rusqlite::Result<CephRocksdbColumnFamilyRecord> {
    Ok(CephRocksdbColumnFamilyRecord {
        inventory_id: row.get(0)?,
        column_family_id: row.get(1)?,
        name: row.get(2)?,
        comparator_name: row.get(3)?,
        dropped: row.get(4)?,
    })
}

fn map_live_sst(row: &rusqlite::Row<'_>) -> rusqlite::Result<CephRocksdbLiveSstRecord> {
    Ok(CephRocksdbLiveSstRecord {
        inventory_id: row.get(0)?,
        column_family_id: row.get(1)?,
        level: row.get(2)?,
        file_number: row.get(3)?,
        path_id: row.get(4)?,
        format: row.get(5)?,
        file_size: row.get(6)?,
        smallest_sequence: row.get(7)?,
        largest_sequence: row.get(8)?,
        smallest_internal_key_length: row.get(9)?,
        largest_internal_key_length: row.get(10)?,
    })
}
