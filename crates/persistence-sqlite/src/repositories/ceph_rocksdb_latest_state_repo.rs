use std::collections::{HashMap, HashSet};

use rusqlite::{params, Connection};

use crate::connection::{DbError, DbResult};

use super::ceph_rocksdb_repo::{CephRocksdbAggregate, CephRocksdbRepo};

const ROCKSDB_MAX_SEQUENCE_NUMBER: u64 = (1u64 << 56) - 1;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CephRocksdbLatestStateRecord {
    pub inventory_id: String,
    pub column_family_id: u32,
    pub column_family_name: String,
    pub schema_version: u32,
    pub sharding_sha256: String,
    pub point_mutation_count: u64,
    pub sst_point_mutation_count: u64,
    pub wal_point_mutation_count: u64,
    pub range_mutation_count: u64,
    pub sst_range_mutation_count: u64,
    pub wal_range_mutation_count: u64,
    pub latest_value_count: u64,
    pub deleted_key_count: u64,
    pub delete_decision_count: u64,
    pub single_delete_decision_count: u64,
    pub range_delete_decision_count: u64,
    pub merge_resolved_count: u64,
    pub merge_operand_count: u64,
    pub range_hidden_version_count: u64,
    pub smallest_sequence: Option<u64>,
    pub largest_sequence: Option<u64>,
    pub point_sha256: String,
    pub range_sha256: String,
    pub latest_state_sha256: String,
    pub scan_complete: bool,
}

pub struct CephRocksdbLatestStateRepo<'a> {
    conn: &'a Connection,
}

impl<'a> CephRocksdbLatestStateRepo<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    pub fn find(&self, inventory_id: &str) -> DbResult<Vec<CephRocksdbLatestStateRecord>> {
        let mut statement = self.conn.prepare(
            "SELECT inventory_id, column_family_id, column_family_name, schema_version,
                    sharding_sha256, point_mutation_count, sst_point_mutation_count,
                    wal_point_mutation_count, range_mutation_count,
                    sst_range_mutation_count, wal_range_mutation_count,
                    latest_value_count, deleted_key_count, delete_decision_count,
                    single_delete_decision_count, range_delete_decision_count,
                    merge_resolved_count, merge_operand_count, range_hidden_version_count,
                    smallest_sequence, largest_sequence, point_sha256, range_sha256,
                    latest_state_sha256, scan_complete
             FROM ceph_rocksdb_latest_state
             WHERE inventory_id = ?1
             ORDER BY column_family_id",
        )?;
        let rows = statement.query_map(params![inventory_id], map_record)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    pub fn replace_for_inventory(
        &self,
        inventory_id: &str,
        records: &[CephRocksdbLatestStateRecord],
    ) -> DbResult<()> {
        validate_target(inventory_id, records)?;
        let transaction = self.conn.unchecked_transaction()?;
        let rocksdb = CephRocksdbRepo::new(&transaction)
            .find_aggregate(inventory_id)?
            .ok_or_else(|| {
                DbError::System(
                    "latest-state replacement references an unknown RocksDB inventory".to_string(),
                )
            })?;
        validate_replacement(&rocksdb, records)?;
        ensure_no_semantic_snapshot(&transaction, inventory_id)?;
        replace_on(&transaction, inventory_id, records)?;
        transaction.commit()?;
        Ok(())
    }
}

pub(crate) fn replace_on(
    conn: &Connection,
    inventory_id: &str,
    records: &[CephRocksdbLatestStateRecord],
) -> DbResult<()> {
    validate_target(inventory_id, records)?;
    conn.execute(
        "DELETE FROM ceph_rocksdb_latest_state WHERE inventory_id = ?1",
        params![inventory_id],
    )?;
    insert_records(conn, records)
}

fn validate_target(inventory_id: &str, records: &[CephRocksdbLatestStateRecord]) -> DbResult<()> {
    if inventory_id.is_empty() || records.is_empty() {
        return latest_state_error("latest-state replacement cannot be empty");
    }
    if records
        .iter()
        .any(|record| record.inventory_id != inventory_id)
    {
        return latest_state_error("latest-state replacement crosses inventory boundaries");
    }
    Ok(())
}

fn ensure_no_semantic_snapshot(conn: &Connection, inventory_id: &str) -> DbResult<()> {
    let exists: bool = conn.query_row(
        "SELECT EXISTS(
            SELECT 1 FROM ceph_bluestore_semantic_scans WHERE inventory_id = ?1
         )",
        params![inventory_id],
        |row| row.get(0),
    )?;
    if exists {
        return latest_state_error(
            "latest-state replacement requires the atomic Ceph aggregate path once BlueStore semantics exist",
        );
    }
    Ok(())
}

pub fn validate_replacement(
    rocksdb: &CephRocksdbAggregate,
    records: &[CephRocksdbLatestStateRecord],
) -> DbResult<()> {
    let active = index_active_column_families(rocksdb)?;
    if active.is_empty() || records.len() != active.len() {
        return latest_state_error(
            "latest-state rows do not cover every active column family exactly once",
        );
    }
    let mut seen = HashSet::with_capacity(records.len());
    let mut sharding_sha256 = None;
    for record in records {
        let column_family = active.get(&record.column_family_id).ok_or_else(|| {
            DbError::System(
                "latest-state row references a dropped or unknown column family".to_string(),
            )
        })?;
        if !seen.insert(record.column_family_id) {
            return latest_state_error("latest-state column family is duplicated");
        }
        validate_record(
            rocksdb.manifest.inventory_id.as_str(),
            column_family.name.as_str(),
            record,
        )?;
        if sharding_sha256
            .replace(record.sharding_sha256.as_str())
            .is_some_and(|previous| previous != record.sharding_sha256)
        {
            return latest_state_error(
                "latest-state rows do not share one sharding definition digest",
            );
        }
    }
    if active.keys().any(|id| !seen.contains(id)) {
        return latest_state_error("latest-state rows omit an active column family");
    }
    Ok(())
}

fn index_active_column_families(
    rocksdb: &CephRocksdbAggregate,
) -> DbResult<HashMap<u32, &super::ceph_rocksdb_repo::CephRocksdbColumnFamilyRecord>> {
    let mut all = HashSet::with_capacity(rocksdb.column_families.len());
    let mut active = HashMap::new();
    for column_family in &rocksdb.column_families {
        if column_family.inventory_id != rocksdb.manifest.inventory_id
            || !all.insert(column_family.column_family_id)
        {
            return latest_state_error("RocksDB column family metadata is inconsistent");
        }
        if !column_family.dropped {
            active.insert(column_family.column_family_id, column_family);
        }
    }
    Ok(active)
}

fn validate_record(
    inventory_id: &str,
    column_family_name: &str,
    record: &CephRocksdbLatestStateRecord,
) -> DbResult<()> {
    if record.inventory_id != inventory_id
        || !valid_text(&record.inventory_id)
        || record.column_family_name != column_family_name
        || !valid_text(&record.column_family_name)
        || record.schema_version != 1
        || !valid_sha256(&record.sharding_sha256)
        || !valid_sha256(&record.point_sha256)
        || !valid_sha256(&record.range_sha256)
        || !valid_sha256(&record.latest_state_sha256)
        || !record.scan_complete
        || !counts_fit_sqlite(record)
        || !valid_count_closure(record)
        || !valid_sequence_range(record)
    {
        return latest_state_error("RocksDB latest-state summary is incomplete or inconsistent");
    }
    Ok(())
}

fn valid_count_closure(record: &CephRocksdbLatestStateRecord) -> bool {
    checked_sum(
        record.sst_point_mutation_count,
        record.wal_point_mutation_count,
    ) == Some(record.point_mutation_count)
        && checked_sum(
            record.sst_range_mutation_count,
            record.wal_range_mutation_count,
        ) == Some(record.range_mutation_count)
        && checked_sum3(
            record.delete_decision_count,
            record.single_delete_decision_count,
            record.range_delete_decision_count,
        ) == Some(record.deleted_key_count)
        && checked_sum(record.latest_value_count, record.deleted_key_count)
            .is_some_and(|count| count <= record.point_mutation_count)
        && record.merge_resolved_count <= record.latest_value_count
        && record.merge_resolved_count <= record.merge_operand_count
        && record.merge_operand_count <= record.point_mutation_count
        && record.range_hidden_version_count <= record.point_mutation_count
        && (record.range_mutation_count > 0
            || (record.range_delete_decision_count == 0 && record.range_hidden_version_count == 0))
}

fn valid_sequence_range(record: &CephRocksdbLatestStateRecord) -> bool {
    let total_mutations = checked_sum(record.point_mutation_count, record.range_mutation_count);
    match (
        total_mutations,
        record.smallest_sequence,
        record.largest_sequence,
    ) {
        (Some(0), None, None) => true,
        (Some(total), Some(smallest), Some(largest)) if total > 0 => {
            smallest <= largest && largest <= ROCKSDB_MAX_SEQUENCE_NUMBER
        }
        _ => false,
    }
}

fn counts_fit_sqlite(record: &CephRocksdbLatestStateRecord) -> bool {
    [
        record.point_mutation_count,
        record.sst_point_mutation_count,
        record.wal_point_mutation_count,
        record.range_mutation_count,
        record.sst_range_mutation_count,
        record.wal_range_mutation_count,
        record.latest_value_count,
        record.deleted_key_count,
        record.delete_decision_count,
        record.single_delete_decision_count,
        record.range_delete_decision_count,
        record.merge_resolved_count,
        record.merge_operand_count,
        record.range_hidden_version_count,
    ]
    .into_iter()
    .all(|value| value <= i64::MAX as u64)
}

fn valid_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn valid_text(value: &str) -> bool {
    !value.is_empty() && !value.contains('\0')
}

fn checked_sum(left: u64, right: u64) -> Option<u64> {
    left.checked_add(right)
}

fn checked_sum3(first: u64, second: u64, third: u64) -> Option<u64> {
    first.checked_add(second)?.checked_add(third)
}

fn insert_records(conn: &Connection, records: &[CephRocksdbLatestStateRecord]) -> DbResult<()> {
    let mut statement = conn.prepare_cached(
        "INSERT INTO ceph_rocksdb_latest_state (
            inventory_id, column_family_id, column_family_name, schema_version,
            sharding_sha256, point_mutation_count, sst_point_mutation_count,
            wal_point_mutation_count, range_mutation_count, sst_range_mutation_count,
            wal_range_mutation_count, latest_value_count, deleted_key_count,
            delete_decision_count, single_delete_decision_count,
            range_delete_decision_count, merge_resolved_count, merge_operand_count,
            range_hidden_version_count, smallest_sequence, largest_sequence,
            point_sha256, range_sha256, latest_state_sha256, scan_complete
         ) VALUES (
            ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13,
            ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24, ?25
         )",
    )?;
    for record in records {
        statement.execute(params![
            record.inventory_id,
            record.column_family_id,
            record.column_family_name,
            record.schema_version,
            record.sharding_sha256,
            record.point_mutation_count,
            record.sst_point_mutation_count,
            record.wal_point_mutation_count,
            record.range_mutation_count,
            record.sst_range_mutation_count,
            record.wal_range_mutation_count,
            record.latest_value_count,
            record.deleted_key_count,
            record.delete_decision_count,
            record.single_delete_decision_count,
            record.range_delete_decision_count,
            record.merge_resolved_count,
            record.merge_operand_count,
            record.range_hidden_version_count,
            record.smallest_sequence,
            record.largest_sequence,
            record.point_sha256,
            record.range_sha256,
            record.latest_state_sha256,
            record.scan_complete,
        ])?;
    }
    Ok(())
}

fn map_record(row: &rusqlite::Row<'_>) -> rusqlite::Result<CephRocksdbLatestStateRecord> {
    Ok(CephRocksdbLatestStateRecord {
        inventory_id: row.get(0)?,
        column_family_id: row.get(1)?,
        column_family_name: row.get(2)?,
        schema_version: row.get(3)?,
        sharding_sha256: row.get(4)?,
        point_mutation_count: row.get(5)?,
        sst_point_mutation_count: row.get(6)?,
        wal_point_mutation_count: row.get(7)?,
        range_mutation_count: row.get(8)?,
        sst_range_mutation_count: row.get(9)?,
        wal_range_mutation_count: row.get(10)?,
        latest_value_count: row.get(11)?,
        deleted_key_count: row.get(12)?,
        delete_decision_count: row.get(13)?,
        single_delete_decision_count: row.get(14)?,
        range_delete_decision_count: row.get(15)?,
        merge_resolved_count: row.get(16)?,
        merge_operand_count: row.get(17)?,
        range_hidden_version_count: row.get(18)?,
        smallest_sequence: row.get(19)?,
        largest_sequence: row.get(20)?,
        point_sha256: row.get(21)?,
        range_sha256: row.get(22)?,
        latest_state_sha256: row.get(23)?,
        scan_complete: row.get(24)?,
    })
}

fn latest_state_error<T>(message: &str) -> DbResult<T> {
    Err(DbError::System(message.to_string()))
}
