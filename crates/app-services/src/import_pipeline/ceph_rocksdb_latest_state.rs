use std::collections::BTreeMap;

use persistence_sqlite::repositories::{
    ceph_rocksdb_latest_state_repo::CephRocksdbLatestStateRecord,
    ceph_rocksdb_repo::CephRocksdbAggregate,
};
use rocksdb_wire::{
    KeyVersion, LatestStateError, LatestStateLimits, LatestStateRef, MergeOperator,
    RocksDbWireError,
};
use transport::CommandError;

mod point_only;

use super::ceph_rocksdb_digest::RecoveryDigests;
use super::ceph_rocksdb_range_state::RangeCoverage;
use super::ceph_rocksdb_sharding::RocksdbShardingDefinition;
use super::ceph_rocksdb_spool::{
    RocksdbRecoverySpool, SpoolPoint, SpoolPointRef, SpoolRange, SpoolSourceKind,
};

const SUMMARY_SCHEMA_VERSION: u32 = 1;
const LATEST_ORIGIN_VALUE: u8 = 1;
const LATEST_ORIGIN_MERGE: u8 = 2;
const LATEST_DELETE: u8 = 0;
const LATEST_SINGLE_DELETE: u8 = 7;
const LATEST_RANGE_DELETE: u8 = 15;

pub(super) fn recover_latest_state(
    rocksdb: &CephRocksdbAggregate,
    sharding: &RocksdbShardingDefinition,
    spool: &RocksdbRecoverySpool,
) -> Result<Vec<CephRocksdbLatestStateRecord>, CommandError> {
    let inventory_id = rocksdb.manifest.inventory_id.as_str();
    let mut summaries = active_column_families(rocksdb)?;
    if spool.range_count() == 0 && spool.merge_count() == 0 {
        return point_only::recover_point_only_state(
            spool,
            summaries,
            inventory_id,
            sharding.digest_sha256(),
        );
    }
    let mut ranges_by_column = BTreeMap::<u32, Vec<SpoolRange>>::new();
    for range in spool.load_ranges()? {
        let summary = summaries
            .get_mut(&range.column_family_id)
            .ok_or_else(|| latest_state_error("range references an inactive column family"))?;
        summary.observe_range(&range)?;
        ranges_by_column
            .entry(range.column_family_id)
            .or_default()
            .push(range);
    }
    let mut coverage = summaries
        .keys()
        .map(|column_family_id| {
            let ranges = ranges_by_column
                .remove(column_family_id)
                .unwrap_or_default();
            (*column_family_id, RangeCoverage::new(ranges))
        })
        .collect::<BTreeMap<_, _>>();

    spool.visit_point_groups(|group| {
        reduce_point_group(group, sharding, &mut coverage, &mut summaries)
    })?;

    finish_summaries(summaries, inventory_id, sharding.digest_sha256())
}

fn finish_summaries(
    summaries: BTreeMap<u32, ColumnFamilySummary>,
    inventory_id: &str,
    sharding_sha256: &str,
) -> Result<Vec<CephRocksdbLatestStateRecord>, CommandError> {
    summaries
        .into_values()
        .map(|summary| summary.finish(inventory_id, sharding_sha256))
        .collect()
}

fn active_column_families(
    rocksdb: &CephRocksdbAggregate,
) -> Result<BTreeMap<u32, ColumnFamilySummary>, CommandError> {
    let mut summaries = BTreeMap::new();
    for column_family in &rocksdb.column_families {
        if column_family.dropped {
            continue;
        }
        let summary =
            ColumnFamilySummary::new(column_family.column_family_id, column_family.name.clone());
        if summaries
            .insert(column_family.column_family_id, summary)
            .is_some()
        {
            return Err(latest_state_error(
                "active column family metadata contains a duplicate id",
            ));
        }
    }
    if summaries.is_empty() {
        return Err(latest_state_error(
            "RocksDB latest-state recovery has no active column family",
        ));
    }
    Ok(summaries)
}

fn reduce_point_group(
    group: &[SpoolPoint],
    sharding: &RocksdbShardingDefinition,
    coverage: &mut BTreeMap<u32, RangeCoverage>,
    summaries: &mut BTreeMap<u32, ColumnFamilySummary>,
) -> Result<(), CommandError> {
    let first = group
        .first()
        .ok_or_else(|| latest_state_error("recovery spool yielded an empty point group"))?;
    if group.iter().any(|point| {
        point.column_family_id != first.column_family_id || point.user_key != first.user_key
    }) {
        return Err(latest_state_error(
            "recovery spool yielded a mixed point group",
        ));
    }
    let range_sequence = coverage
        .get_mut(&first.column_family_id)
        .ok_or_else(|| latest_state_error("point references an inactive column family"))?
        .covering_sequence(&first.user_key);
    let merged = group
        .iter()
        .find(|point| !is_hidden(point.sequence, range_sequence))
        .is_some_and(|point| point.value_type == 2);
    {
        let summary = summaries
            .get_mut(&first.column_family_id)
            .ok_or_else(|| latest_state_error("point summary column family is missing"))?;
        for point in group {
            summary.observe_point(point)?;
        }
        summary.observe_hidden_versions(group, range_sequence)?;
    }

    let history = key_versions(group)?;
    let state = {
        let column_family_name = summaries
            .get(&first.column_family_id)
            .ok_or_else(|| latest_state_error("point summary column family is missing"))?
            .column_family_name
            .as_str();
        let mut merge_operator = CephMergeAdapter {
            sharding,
            column_family_name,
        };
        rocksdb_wire::reduce_latest_state_ref(
            &first.user_key,
            &history,
            range_sequence,
            LatestStateLimits::default(),
            &mut merge_operator,
        )
        .map_err(map_reducer_error)?
    };
    summaries
        .get_mut(&first.column_family_id)
        .ok_or_else(|| latest_state_error("point summary column family is missing"))?
        .observe_latest(&first.user_key, state, merged)
}

fn key_versions(group: &[SpoolPoint]) -> Result<Vec<KeyVersion<'_>>, CommandError> {
    group
        .iter()
        .map(|point| match point.value_type {
            0 => Ok(KeyVersion::delete(point.sequence)),
            1 => Ok(KeyVersion::value(point.sequence, &point.value)),
            2 => Ok(KeyVersion::merge(point.sequence, &point.value)),
            7 => Ok(KeyVersion::single_delete(point.sequence)),
            value_type => Err(CommandError::unsupported(format!(
                "RocksDB latest-state recovery does not support value type {value_type:#04x}"
            ))),
        })
        .collect()
}

struct CephMergeAdapter<'a> {
    sharding: &'a RocksdbShardingDefinition,
    column_family_name: &'a str,
}

impl MergeOperator for CephMergeAdapter<'_> {
    type Error = CommandError;

    fn full_merge(
        &mut self,
        user_key: &[u8],
        existing_value: Option<&[u8]>,
        operands_oldest_to_newest: &[&[u8]],
        max_output_bytes: usize,
    ) -> Result<Vec<u8>, Self::Error> {
        let value = super::ceph_rocksdb_merge::full_merge(
            self.sharding,
            self.column_family_name,
            user_key,
            existing_value,
            operands_oldest_to_newest,
        )?;
        if value.len() > max_output_bytes {
            return Err(CommandError::unsupported(format!(
                "Ceph RocksDB merge output exceeds the {max_output_bytes} byte recovery limit"
            )));
        }
        Ok(value)
    }
}

struct ColumnFamilySummary {
    column_family_id: u32,
    column_family_name: String,
    point_mutation_count: u64,
    sst_point_mutation_count: u64,
    wal_point_mutation_count: u64,
    range_mutation_count: u64,
    sst_range_mutation_count: u64,
    wal_range_mutation_count: u64,
    latest_value_count: u64,
    deleted_key_count: u64,
    delete_decision_count: u64,
    single_delete_decision_count: u64,
    range_delete_decision_count: u64,
    merge_resolved_count: u64,
    merge_operand_count: u64,
    range_hidden_version_count: u64,
    smallest_sequence: Option<u64>,
    largest_sequence: Option<u64>,
    digests: RecoveryDigests,
}

impl ColumnFamilySummary {
    fn new(column_family_id: u32, column_family_name: String) -> Self {
        Self {
            column_family_id,
            digests: RecoveryDigests::new(column_family_id, &column_family_name),
            column_family_name,
            point_mutation_count: 0,
            sst_point_mutation_count: 0,
            wal_point_mutation_count: 0,
            range_mutation_count: 0,
            sst_range_mutation_count: 0,
            wal_range_mutation_count: 0,
            latest_value_count: 0,
            deleted_key_count: 0,
            delete_decision_count: 0,
            single_delete_decision_count: 0,
            range_delete_decision_count: 0,
            merge_resolved_count: 0,
            merge_operand_count: 0,
            range_hidden_version_count: 0,
            smallest_sequence: None,
            largest_sequence: None,
        }
    }

    fn observe_point(&mut self, point: &SpoolPoint) -> Result<(), CommandError> {
        self.observe_point_ref(SpoolPointRef {
            column_family_id: point.column_family_id,
            user_key: &point.user_key,
            sequence: point.sequence,
            value_type: point.value_type,
            value: &point.value,
            provenance: point.provenance,
        })
    }

    fn observe_point_ref(&mut self, point: SpoolPointRef<'_>) -> Result<(), CommandError> {
        checked_increment(&mut self.point_mutation_count, "point mutation count")?;
        match point.provenance.source_kind {
            SpoolSourceKind::Sst => checked_increment(
                &mut self.sst_point_mutation_count,
                "SST point mutation count",
            )?,
            SpoolSourceKind::Wal => checked_increment(
                &mut self.wal_point_mutation_count,
                "WAL point mutation count",
            )?,
        }
        if point.value_type == 2 {
            checked_increment(&mut self.merge_operand_count, "merge operand count")?;
        }
        self.observe_sequence(point.sequence);
        self.digests.observe_point_ref(point);
        Ok(())
    }

    fn observe_range(&mut self, range: &SpoolRange) -> Result<(), CommandError> {
        checked_increment(&mut self.range_mutation_count, "range mutation count")?;
        match range.provenance.source_kind {
            SpoolSourceKind::Sst => checked_increment(
                &mut self.sst_range_mutation_count,
                "SST range mutation count",
            )?,
            SpoolSourceKind::Wal => checked_increment(
                &mut self.wal_range_mutation_count,
                "WAL range mutation count",
            )?,
        }
        self.observe_sequence(range.sequence);
        self.digests.observe_range(range);
        Ok(())
    }

    fn observe_hidden_versions(
        &mut self,
        group: &[SpoolPoint],
        range_sequence: Option<u64>,
    ) -> Result<(), CommandError> {
        let hidden = group
            .iter()
            .filter(|point| is_hidden(point.sequence, range_sequence))
            .count();
        let hidden = u64::try_from(hidden)
            .map_err(|_| latest_state_error("range-hidden version count exceeds u64"))?;
        self.range_hidden_version_count = self
            .range_hidden_version_count
            .checked_add(hidden)
            .ok_or_else(|| latest_state_error("range-hidden version count overflow"))?;
        Ok(())
    }

    fn observe_latest(
        &mut self,
        user_key: &[u8],
        state: Option<LatestStateRef<'_>>,
        merged: bool,
    ) -> Result<(), CommandError> {
        let Some(state) = state else {
            return Ok(());
        };
        match state {
            LatestStateRef::Value { sequence, value } => {
                checked_increment(&mut self.latest_value_count, "latest value count")?;
                if merged {
                    checked_increment(&mut self.merge_resolved_count, "merge-resolved count")?;
                }
                self.digests.observe_live(
                    user_key,
                    sequence,
                    if merged {
                        LATEST_ORIGIN_MERGE
                    } else {
                        LATEST_ORIGIN_VALUE
                    },
                    value.as_ref(),
                );
            }
            LatestStateRef::Delete { sequence } => {
                checked_increment(&mut self.deleted_key_count, "deleted key count")?;
                checked_increment(&mut self.delete_decision_count, "delete decision count")?;
                self.digests
                    .observe_deleted(user_key, sequence, LATEST_DELETE);
            }
            LatestStateRef::SingleDelete { sequence } => {
                checked_increment(&mut self.deleted_key_count, "deleted key count")?;
                checked_increment(
                    &mut self.single_delete_decision_count,
                    "single-delete decision count",
                )?;
                self.digests
                    .observe_deleted(user_key, sequence, LATEST_SINGLE_DELETE);
            }
            LatestStateRef::RangeDelete { sequence } => {
                checked_increment(&mut self.deleted_key_count, "deleted key count")?;
                checked_increment(
                    &mut self.range_delete_decision_count,
                    "range-delete decision count",
                )?;
                self.digests
                    .observe_deleted(user_key, sequence, LATEST_RANGE_DELETE);
            }
        }
        Ok(())
    }

    fn observe_sequence(&mut self, sequence: u64) {
        self.smallest_sequence = Some(
            self.smallest_sequence
                .map_or(sequence, |current| current.min(sequence)),
        );
        self.largest_sequence = Some(
            self.largest_sequence
                .map_or(sequence, |current| current.max(sequence)),
        );
    }

    fn finish(
        self,
        inventory_id: &str,
        sharding_sha256: &str,
    ) -> Result<CephRocksdbLatestStateRecord, CommandError> {
        let digests = self.digests.finish();
        Ok(CephRocksdbLatestStateRecord {
            inventory_id: inventory_id.to_string(),
            column_family_id: self.column_family_id,
            column_family_name: self.column_family_name,
            schema_version: SUMMARY_SCHEMA_VERSION,
            sharding_sha256: sharding_sha256.to_string(),
            point_mutation_count: self.point_mutation_count,
            sst_point_mutation_count: self.sst_point_mutation_count,
            wal_point_mutation_count: self.wal_point_mutation_count,
            range_mutation_count: self.range_mutation_count,
            sst_range_mutation_count: self.sst_range_mutation_count,
            wal_range_mutation_count: self.wal_range_mutation_count,
            latest_value_count: self.latest_value_count,
            deleted_key_count: self.deleted_key_count,
            delete_decision_count: self.delete_decision_count,
            single_delete_decision_count: self.single_delete_decision_count,
            range_delete_decision_count: self.range_delete_decision_count,
            merge_resolved_count: self.merge_resolved_count,
            merge_operand_count: self.merge_operand_count,
            range_hidden_version_count: self.range_hidden_version_count,
            smallest_sequence: self.smallest_sequence,
            largest_sequence: self.largest_sequence,
            point_sha256: digests.point_sha256,
            range_sha256: digests.range_sha256,
            latest_state_sha256: digests.latest_state_sha256,
            scan_complete: true,
        })
    }
}

fn is_hidden(point_sequence: u64, range_sequence: Option<u64>) -> bool {
    range_sequence.is_some_and(|sequence| sequence > point_sequence)
}

fn checked_increment(value: &mut u64, field: &'static str) -> Result<(), CommandError> {
    *value = value
        .checked_add(1)
        .ok_or_else(|| latest_state_error(format!("{field} overflow")))?;
    Ok(())
}

fn map_reducer_error(error: LatestStateError<CommandError>) -> CommandError {
    match error {
        LatestStateError::MergeOperator(error) => error,
        LatestStateError::Wire(error) => {
            let message = format!("RocksDB latest-state reduction failed: {error}");
            if matches!(
                error,
                RocksDbWireError::LatestStateVersionLimit { .. }
                    | RocksDbWireError::LatestStateHistoryBytesLimit { .. }
                    | RocksDbWireError::LatestStateMergeOperandLimit { .. }
                    | RocksDbWireError::LatestStateResolvedValueLimit { .. }
            ) {
                CommandError::unsupported(message)
            } else {
                CommandError::parser(message)
            }
        }
    }
}

fn latest_state_error(message: impl Into<String>) -> CommandError {
    CommandError::parser(format!(
        "RocksDB latest-state recovery failed: {}",
        message.into()
    ))
}

#[cfg(test)]
#[path = "../../tests/unit/import_pipeline/ceph_rocksdb_latest_state.rs"]
mod tests;
