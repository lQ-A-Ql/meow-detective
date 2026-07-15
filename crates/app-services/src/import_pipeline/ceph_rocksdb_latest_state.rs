use std::collections::BTreeMap;

use persistence_sqlite::repositories::{
    ceph_bluestore_semantic_repo::CephBluestoreSemanticAggregate,
    ceph_rocksdb_latest_state_repo::CephRocksdbLatestStateRecord,
    ceph_rocksdb_repo::CephRocksdbAggregate,
};
use rocksdb_wire::{
    KeyVersion, LatestStateError, LatestStateLimits, LatestStateRef, MergeOperator,
    RocksDbWireError,
};
use transport::CommandError;

mod point_only;
mod summary;

use super::ceph_rocksdb_range_state::RangeCoverage;
use super::ceph_rocksdb_sharding::RocksdbShardingDefinition;
use super::ceph_rocksdb_spool::{RocksdbRecoverySpool, SpoolPoint, SpoolRange};
use summary::{is_hidden, ColumnFamilySummary};

pub(super) struct RecoveredLatestState {
    pub(super) summaries: Vec<CephRocksdbLatestStateRecord>,
    pub(super) semantic: CephBluestoreSemanticAggregate,
}

pub(super) fn recover_latest_state(
    rocksdb: &CephRocksdbAggregate,
    sharding: &RocksdbShardingDefinition,
    spool: &RocksdbRecoverySpool,
    device_size: u64,
) -> Result<RecoveredLatestState, CommandError> {
    let inventory_id = rocksdb.manifest.inventory_id.as_str();
    let mut summaries = active_column_families(rocksdb)?;
    if spool.range_count() == 0 && spool.merge_count() == 0 {
        return point_only::recover_point_only_state(
            spool,
            summaries,
            inventory_id,
            sharding,
            device_size,
        );
    }
    let mut semantics = semantic_fragments(&summaries);
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
        reduce_point_group(
            group,
            sharding,
            &mut coverage,
            &mut summaries,
            &mut semantics,
        )
    })?;

    finish_recovery(
        summaries,
        semantics,
        inventory_id,
        sharding.digest_sha256(),
        device_size,
    )
}

fn finish_recovery(
    summaries: BTreeMap<u32, ColumnFamilySummary>,
    semantics: BTreeMap<u32, super::ceph_bluestore_semantic::BlueStoreSemanticFragment>,
    inventory_id: &str,
    sharding_sha256: &str,
    device_size: u64,
) -> Result<RecoveredLatestState, CommandError> {
    let summaries = summaries
        .into_values()
        .map(|summary| summary.finish(inventory_id, sharding_sha256))
        .collect::<Result<Vec<_>, _>>()?;
    finish_recovery_parts(
        summaries,
        semantics.into_values().collect(),
        inventory_id,
        sharding_sha256,
        device_size,
    )
}

pub(super) fn finish_recovery_parts(
    summaries: Vec<CephRocksdbLatestStateRecord>,
    fragments: Vec<super::ceph_bluestore_semantic::BlueStoreSemanticFragment>,
    inventory_id: &str,
    sharding_sha256: &str,
    device_size: u64,
) -> Result<RecoveredLatestState, CommandError> {
    let mut semantic = super::ceph_bluestore_semantic::BlueStoreSemanticFragment::default();
    for fragment in fragments {
        semantic.merge(fragment)?;
    }
    let semantic = semantic.finish(inventory_id, sharding_sha256, device_size, &summaries)?;
    Ok(RecoveredLatestState {
        summaries,
        semantic,
    })
}

fn semantic_fragments(
    summaries: &BTreeMap<u32, ColumnFamilySummary>,
) -> BTreeMap<u32, super::ceph_bluestore_semantic::BlueStoreSemanticFragment> {
    summaries
        .keys()
        .map(|column_family_id| {
            (
                *column_family_id,
                super::ceph_bluestore_semantic::BlueStoreSemanticFragment::default(),
            )
        })
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
    semantics: &mut BTreeMap<u32, super::ceph_bluestore_semantic::BlueStoreSemanticFragment>,
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
    let state = state.as_ref();
    let summary = summaries
        .get_mut(&first.column_family_id)
        .ok_or_else(|| latest_state_error("point summary column family is missing"))?;
    if let Some(LatestStateRef::Value { value, .. }) = state {
        semantics
            .get_mut(&first.column_family_id)
            .ok_or_else(|| latest_state_error("semantic column family is missing"))?
            .observe_latest_value(
                sharding,
                &summary.column_family_name,
                &first.user_key,
                value.as_ref(),
            )?;
    }
    summary.observe_latest(&first.user_key, state, merged)
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
