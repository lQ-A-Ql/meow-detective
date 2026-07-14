use super::ceph_bluefs_replay_repo::CephBluefsFileRecord;
use super::ceph_bluefs_repo::CephBluefsAggregate;
use super::ceph_rocksdb_repo::CephRocksdbAggregate;
use crate::connection::{DbError, DbResult};
use rusqlite::{params, Connection};
use std::collections::{BTreeMap, HashMap};

mod rows;
mod selection;

use rows::{map_file, map_record};
use selection::{expected_wal_numbers, expected_wal_root, parse_wal_path};

const ROCKSDB_MAX_SEQUENCE_NUMBER: u64 = (1u64 << 56) - 1;
const WRITE_BATCH_HEADER_SIZE: u64 = 12;
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CephRocksdbWalFileRecord {
    pub inventory_id: String,
    pub wal_number: u64,
    pub bluefs_path: String,
    pub post_manifest: bool,
    pub file_size: u64,
    pub logical_record_count: u32,
    pub empty_batch_count: u32,
    pub mutation_count: u64,
    pub auxiliary_record_count: u64,
    pub logical_payload_bytes: u64,
    pub fragment_count: u64,
    pub first_sequence: Option<u64>,
    pub last_sequence: Option<u64>,
    pub first_record_offset: Option<u64>,
    pub last_record_offset: Option<u64>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CephRocksdbWalRecord {
    pub inventory_id: String,
    pub wal_number: u64,
    pub record_ordinal: u64,
    pub physical_offset: u64,
    pub fragment_count: u32,
    pub recyclable_log_number: Option<u32>,
    pub batch_sequence: u64,
    pub mutation_count: u32,
    pub auxiliary_record_count: u32,
    pub first_mutation_sequence: Option<u64>,
    pub last_mutation_sequence: Option<u64>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CephRocksdbWalAggregate {
    pub files: Vec<CephRocksdbWalFileRecord>,
    pub records: Vec<CephRocksdbWalRecord>,
}
pub struct CephRocksdbWalRepo<'a> {
    conn: &'a Connection,
}

impl<'a> CephRocksdbWalRepo<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    pub fn find_files_for_inventory(
        &self,
        inventory_id: &str,
    ) -> DbResult<Vec<CephRocksdbWalFileRecord>> {
        let mut statement = self.conn.prepare(
            "SELECT inventory_id, wal_number, bluefs_path, post_manifest, file_size,
                    logical_record_count, empty_batch_count, mutation_count,
                    auxiliary_record_count, logical_payload_bytes, fragment_count,
                    first_sequence, last_sequence, first_record_offset, last_record_offset
             FROM ceph_rocksdb_wal_files
             WHERE inventory_id = ?1
             ORDER BY wal_number",
        )?;
        let rows = statement.query_map(params![inventory_id], map_file)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    pub fn find_records_for_inventory(
        &self,
        inventory_id: &str,
    ) -> DbResult<Vec<CephRocksdbWalRecord>> {
        let mut statement = self.conn.prepare(
            "SELECT inventory_id, wal_number, record_ordinal, physical_offset,
                    fragment_count, recyclable_log_number, batch_sequence,
                    mutation_count, auxiliary_record_count,
                    first_mutation_sequence, last_mutation_sequence
             FROM ceph_rocksdb_wal_records
             WHERE inventory_id = ?1
             ORDER BY wal_number, record_ordinal",
        )?;
        let rows = statement.query_map(params![inventory_id], map_record)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    pub fn find_aggregate(&self, inventory_id: &str) -> DbResult<CephRocksdbWalAggregate> {
        Ok(CephRocksdbWalAggregate {
            files: self.find_files_for_inventory(inventory_id)?,
            records: self.find_records_for_inventory(inventory_id)?,
        })
    }
}

pub(super) fn replace_for_inventory_on(
    conn: &Connection,
    inventory_id: &str,
    aggregate: &CephRocksdbWalAggregate,
) -> DbResult<()> {
    conn.execute(
        "DELETE FROM ceph_rocksdb_wal_files WHERE inventory_id = ?1",
        params![inventory_id],
    )?;
    insert_files(conn, &aggregate.files)?;
    insert_records(conn, &aggregate.records)
}

pub(super) fn validate_replacement(
    bluefs: &CephBluefsAggregate,
    rocksdb: &CephRocksdbAggregate,
    aggregate: &CephRocksdbWalAggregate,
) -> DbResult<()> {
    let inventory_id = rocksdb.manifest.inventory_id.as_str();
    let replay_files = index_replay_files(bluefs, inventory_id)?;
    let recovery_lower_bound = recovery_log_boundary(rocksdb)?;
    let expected_root = expected_wal_root(bluefs)?;
    let expected_wal_numbers =
        expected_wal_numbers(&replay_files, expected_root, recovery_lower_bound)?;
    let mut files = HashMap::with_capacity(aggregate.files.len());
    for file in &aggregate.files {
        validate_file_binding(
            inventory_id,
            file,
            &replay_files,
            recovery_lower_bound,
            rocksdb.manifest.next_file_number,
            expected_root,
        )?;
        if files.insert(file.wal_number, file).is_some() {
            return wal_error("WAL file number is duplicated");
        }
    }
    if files.len() != expected_wal_numbers.len()
        || files
            .keys()
            .any(|wal_number| !expected_wal_numbers.contains(wal_number))
    {
        return wal_error("WAL inventory does not cover the complete selected file set");
    }

    let records = index_records(inventory_id, &files, &aggregate.records)?;
    validate_file_summaries(&files, &records)?;
    validate_global_sequences(&records)
}

fn index_replay_files<'a>(
    bluefs: &'a CephBluefsAggregate,
    inventory_id: &str,
) -> DbResult<HashMap<&'a str, &'a CephBluefsFileRecord>> {
    if bluefs.superblock.inventory_id != inventory_id {
        return wal_error("BlueFS and RocksDB inventory identities do not match");
    }
    let mut files = HashMap::with_capacity(bluefs.replay.files.len());
    for file in &bluefs.replay.files {
        if file.inventory_id != bluefs.superblock.inventory_id
            || files.insert(file.path.as_str(), file).is_some()
        {
            return wal_error("BlueFS replay file metadata is inconsistent or duplicated");
        }
    }
    Ok(files)
}

fn recovery_log_boundary(rocksdb: &CephRocksdbAggregate) -> DbResult<u64> {
    let column_family_minimum = rocksdb
        .column_families
        .iter()
        .filter(|column_family| !column_family.dropped)
        .map(|column_family| column_family.log_number.unwrap_or_default())
        .min()
        .ok_or_else(|| DbError::System("RocksDB has no active column family".to_string()))?;
    Ok(column_family_minimum.max(rocksdb.manifest.min_log_number_to_keep.unwrap_or_default()))
}

fn validate_file_binding(
    inventory_id: &str,
    file: &CephRocksdbWalFileRecord,
    replay_files: &HashMap<&str, &CephBluefsFileRecord>,
    recovery_lower_bound: u64,
    next_file_number: u64,
    expected_root: &str,
) -> DbResult<()> {
    let (root, parsed_number) = parse_wal_path(&file.bluefs_path)
        .ok_or_else(|| DbError::System("WAL inventory path is not canonical".to_string()))?;
    if root != expected_root {
        return wal_error("WAL inventory does not use the selected BlueFS WAL root");
    }
    let expected = replay_files.get(file.bluefs_path.as_str()).ok_or_else(|| {
        DbError::System("WAL inventory is not bound to a BlueFS replay file".to_string())
    })?;
    if file.inventory_id != inventory_id
        || parsed_number != file.wal_number
        || file.wal_number < recovery_lower_bound
        || file.post_manifest != (file.wal_number >= next_file_number)
        || file.file_size != expected.size
        || expected.encoding != 0
        || !fits_sqlite(file.wal_number)
        || !fits_sqlite(file.file_size)
        || !fits_sqlite(file.mutation_count)
        || !fits_sqlite(file.auxiliary_record_count)
        || !fits_sqlite(file.logical_payload_bytes)
        || !fits_sqlite(file.fragment_count)
    {
        return wal_error("WAL file summary is not bound to the active BlueFS file");
    }
    Ok(())
}

fn index_records<'a>(
    inventory_id: &str,
    files: &HashMap<u64, &'a CephRocksdbWalFileRecord>,
    records: &'a [CephRocksdbWalRecord],
) -> DbResult<BTreeMap<(u64, u64), &'a CephRocksdbWalRecord>> {
    let mut indexed = BTreeMap::new();
    for record in records {
        if record.inventory_id != inventory_id
            || !files.contains_key(&record.wal_number)
            || record.fragment_count == 0
            || record.batch_sequence > ROCKSDB_MAX_SEQUENCE_NUMBER
            || !fits_sqlite(record.wal_number)
            || !fits_sqlite(record.record_ordinal)
            || !fits_sqlite(record.physical_offset)
            || !valid_record_sequence(record)
            || indexed
                .insert((record.wal_number, record.record_ordinal), record)
                .is_some()
        {
            return wal_error("WAL logical record metadata is invalid or duplicated");
        }
    }
    Ok(indexed)
}

fn validate_file_summaries(
    files: &HashMap<u64, &CephRocksdbWalFileRecord>,
    records: &BTreeMap<(u64, u64), &CephRocksdbWalRecord>,
) -> DbResult<()> {
    for (&wal_number, file) in files {
        let file_records = records
            .range((wal_number, 0)..=(wal_number, u64::MAX))
            .map(|(_, record)| *record)
            .collect::<Vec<_>>();
        validate_file_summary(file, &file_records)?;
    }
    Ok(())
}

fn validate_file_summary(
    file: &CephRocksdbWalFileRecord,
    records: &[&CephRocksdbWalRecord],
) -> DbResult<()> {
    if records.is_empty() {
        return validate_empty_file(file);
    }
    let expected_count = u32::try_from(records.len())
        .map_err(|_| DbError::System("WAL logical record count exceeds u32".to_string()))?;
    let mut empty_batches = 0u32;
    let mut mutations = 0u64;
    let mut auxiliary = 0u64;
    let mut fragments = 0u64;
    let mut previous_offset = None;
    let mut recyclable = None;
    for (ordinal, record) in records.iter().enumerate() {
        validate_record_order(
            file,
            record,
            ordinal as u64,
            previous_offset,
            &mut recyclable,
        )?;
        previous_offset = Some(record.physical_offset);
        empty_batches += u32::from(record.mutation_count == 0);
        mutations = checked_add(mutations, u64::from(record.mutation_count), "mutation")?;
        auxiliary = checked_add(
            auxiliary,
            u64::from(record.auxiliary_record_count),
            "auxiliary record",
        )?;
        fragments = checked_add(fragments, u64::from(record.fragment_count), "fragment")?;
    }
    let first_sequence = records.first().map(|record| record.batch_sequence);
    let last_sequence = records.last().map(|record| {
        record
            .last_mutation_sequence
            .unwrap_or(record.batch_sequence)
    });
    let minimum_payload = u64::from(expected_count) * WRITE_BATCH_HEADER_SIZE;
    if file.logical_record_count != expected_count
        || file.empty_batch_count != empty_batches
        || file.mutation_count != mutations
        || file.auxiliary_record_count != auxiliary
        || file.fragment_count != fragments
        || file.logical_payload_bytes < minimum_payload
        || file.logical_payload_bytes > file.file_size
        || file.first_sequence != first_sequence
        || file.last_sequence != last_sequence
        || file.first_record_offset != records.first().map(|record| record.physical_offset)
        || file.last_record_offset != records.last().map(|record| record.physical_offset)
    {
        return wal_error("WAL file summary does not match its logical records");
    }
    Ok(())
}

fn validate_record_order(
    file: &CephRocksdbWalFileRecord,
    record: &CephRocksdbWalRecord,
    expected_ordinal: u64,
    previous_offset: Option<u64>,
    recyclable: &mut Option<Option<u32>>,
) -> DbResult<()> {
    let expected_recyclable = file.wal_number as u32;
    if record.record_ordinal != expected_ordinal
        || record.physical_offset >= file.file_size
        || previous_offset.is_some_and(|offset| record.physical_offset <= offset)
        || record
            .recyclable_log_number
            .is_some_and(|value| value != expected_recyclable)
        || recyclable.is_some_and(|value| value != record.recyclable_log_number)
    {
        return wal_error("WAL record order, offset, or recyclable identity is invalid");
    }
    if recyclable.is_none() {
        *recyclable = Some(record.recyclable_log_number);
    }
    Ok(())
}

fn validate_empty_file(file: &CephRocksdbWalFileRecord) -> DbResult<()> {
    if file.logical_record_count == 0
        && file.empty_batch_count == 0
        && file.mutation_count == 0
        && file.auxiliary_record_count == 0
        && file.logical_payload_bytes == 0
        && file.fragment_count == 0
        && file.first_sequence.is_none()
        && file.last_sequence.is_none()
        && file.first_record_offset.is_none()
        && file.last_record_offset.is_none()
    {
        Ok(())
    } else {
        wal_error("Empty WAL file summary contains non-empty metadata")
    }
}

fn validate_global_sequences(
    records: &BTreeMap<(u64, u64), &CephRocksdbWalRecord>,
) -> DbResult<()> {
    let mut previous_batch_sequence = None;
    let mut previous_mutation_end = None;
    for record in records.values() {
        if previous_batch_sequence.is_some_and(|previous| record.batch_sequence < previous) {
            return wal_error("WAL batch sequences are not monotonic");
        }
        if let (Some(previous), Some(first), Some(last)) = (
            previous_mutation_end,
            record.first_mutation_sequence,
            record.last_mutation_sequence,
        ) {
            if first <= previous {
                return wal_error("WAL mutation sequence ranges overlap");
            }
            previous_mutation_end = Some(last);
        } else if let Some(last) = record.last_mutation_sequence {
            previous_mutation_end = Some(last);
        }
        previous_batch_sequence = Some(record.batch_sequence);
    }
    Ok(())
}

fn valid_record_sequence(record: &CephRocksdbWalRecord) -> bool {
    if record.mutation_count == 0 {
        return record.first_mutation_sequence.is_none() && record.last_mutation_sequence.is_none();
    }
    record.first_mutation_sequence == Some(record.batch_sequence)
        && record.last_mutation_sequence
            == record
                .batch_sequence
                .checked_add(u64::from(record.mutation_count) - 1)
        && record
            .last_mutation_sequence
            .is_some_and(|value| value <= ROCKSDB_MAX_SEQUENCE_NUMBER)
}

fn checked_add(current: u64, value: u64, label: &str) -> DbResult<u64> {
    current
        .checked_add(value)
        .ok_or_else(|| DbError::System(format!("WAL {label} count overflow")))
}

fn fits_sqlite(value: u64) -> bool {
    value <= i64::MAX as u64
}

fn wal_error<T>(message: &str) -> DbResult<T> {
    Err(DbError::System(message.to_string()))
}

fn insert_files(conn: &Connection, files: &[CephRocksdbWalFileRecord]) -> DbResult<()> {
    let mut statement = conn.prepare_cached(
        "INSERT INTO ceph_rocksdb_wal_files (
            inventory_id, wal_number, bluefs_path, post_manifest, file_size, logical_record_count,
            empty_batch_count, mutation_count, auxiliary_record_count,
            logical_payload_bytes, fragment_count, first_sequence, last_sequence,
            first_record_offset, last_record_offset
         ) VALUES (
            ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15
         )",
    )?;
    for file in files {
        statement.execute(params![
            file.inventory_id,
            file.wal_number,
            file.bluefs_path,
            file.post_manifest,
            file.file_size,
            file.logical_record_count,
            file.empty_batch_count,
            file.mutation_count,
            file.auxiliary_record_count,
            file.logical_payload_bytes,
            file.fragment_count,
            file.first_sequence,
            file.last_sequence,
            file.first_record_offset,
            file.last_record_offset,
        ])?;
    }
    Ok(())
}

fn insert_records(conn: &Connection, records: &[CephRocksdbWalRecord]) -> DbResult<()> {
    let mut statement = conn.prepare_cached(
        "INSERT INTO ceph_rocksdb_wal_records (
            inventory_id, wal_number, record_ordinal, physical_offset,
            fragment_count, recyclable_log_number, batch_sequence,
            mutation_count, auxiliary_record_count,
            first_mutation_sequence, last_mutation_sequence
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
    )?;
    for record in records {
        statement.execute(params![
            record.inventory_id,
            record.wal_number,
            record.record_ordinal,
            record.physical_offset,
            record.fragment_count,
            record.recyclable_log_number,
            record.batch_sequence,
            record.mutation_count,
            record.auxiliary_record_count,
            record.first_mutation_sequence,
            record.last_mutation_sequence,
        ])?;
    }
    Ok(())
}
