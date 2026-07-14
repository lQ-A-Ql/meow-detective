use std::borrow::Cow;
use std::collections::BTreeMap;

use persistence_sqlite::repositories::ceph_rocksdb_latest_state_repo::CephRocksdbLatestStateRecord;
use rayon::prelude::*;
use rocksdb_wire::{LatestStateLimits, LatestStateRef};
use transport::CommandError;

use super::{latest_state_error, ColumnFamilySummary};
use crate::import_pipeline::ceph_rocksdb_spool::{RocksdbRecoverySpool, SpoolPointRef};

pub(super) fn recover_point_only_state(
    spool: &RocksdbRecoverySpool,
    summaries: BTreeMap<u32, ColumnFamilySummary>,
    inventory_id: &str,
    sharding_sha256: &str,
) -> Result<Vec<CephRocksdbLatestStateRecord>, CommandError> {
    for column_family_id in spool.point_column_family_ids()? {
        if !summaries.contains_key(&column_family_id) {
            return Err(latest_state_error(
                "point references an inactive column family",
            ));
        }
    }
    let spool_path = spool.path().to_path_buf();
    let mut records = summaries
        .into_values()
        .collect::<Vec<_>>()
        .into_par_iter()
        .map(|mut summary| {
            let column_family_id = summary.column_family_id;
            let mut recovery = PointOnlyRecovery::new(&mut summary);
            RocksdbRecoverySpool::visit_point_rows_for_column(
                &spool_path,
                column_family_id,
                |point| recovery.observe(point),
            )?;
            summary.finish(inventory_id, sharding_sha256)
        })
        .collect::<Result<Vec<_>, CommandError>>()?;
    records.sort_by_key(|record| record.column_family_id);
    Ok(records)
}

struct PointOnlyRecovery<'a> {
    summary: &'a mut ColumnFamilySummary,
    has_current_key: bool,
    current_user_key: Vec<u8>,
    previous_sequence: Option<u64>,
    version_count: usize,
    history_bytes: usize,
    limits: LatestStateLimits,
}

impl<'a> PointOnlyRecovery<'a> {
    fn new(summary: &'a mut ColumnFamilySummary) -> Self {
        Self {
            summary,
            has_current_key: false,
            current_user_key: Vec::new(),
            previous_sequence: None,
            version_count: 0,
            history_bytes: 0,
            limits: LatestStateLimits::default(),
        }
    }

    fn observe(&mut self, point: SpoolPointRef<'_>) -> Result<(), CommandError> {
        if point.column_family_id != self.summary.column_family_id || point.value_type == 2 {
            return Err(latest_state_error(
                "point-only recovery encountered a foreign column family or merge operand",
            ));
        }
        let first_for_key =
            !self.has_current_key || self.current_user_key.as_slice() != point.user_key;
        self.validate_history(point, first_for_key)?;
        self.summary.observe_point_ref(point)?;
        if first_for_key {
            self.summary.observe_latest(
                point.user_key,
                Some(point_only_latest_state(point, self.limits)?),
                false,
            )?;
        }
        Ok(())
    }

    fn validate_history(
        &mut self,
        point: SpoolPointRef<'_>,
        first_for_key: bool,
    ) -> Result<(), CommandError> {
        if first_for_key {
            self.has_current_key = true;
            self.current_user_key.clear();
            self.current_user_key.extend_from_slice(point.user_key);
            self.previous_sequence = None;
            self.version_count = 0;
            self.history_bytes = 0;
        }
        if self
            .previous_sequence
            .is_some_and(|previous| point.sequence >= previous)
        {
            return Err(latest_state_error(
                "point-only recovery history is not strictly descending",
            ));
        }
        self.previous_sequence = Some(point.sequence);
        self.version_count = self
            .version_count
            .checked_add(1)
            .ok_or_else(|| latest_state_error("point-only version count overflow"))?;
        if self.version_count > self.limits.max_versions {
            return Err(CommandError::unsupported(
                "RocksDB point-only history exceeds the per-key version limit",
            ));
        }
        let version_bytes = point
            .user_key
            .len()
            .checked_add(8)
            .and_then(|bytes| bytes.checked_add(point.value.len()))
            .ok_or_else(|| latest_state_error("point-only history byte count overflow"))?;
        self.history_bytes = self
            .history_bytes
            .checked_add(version_bytes)
            .ok_or_else(|| latest_state_error("point-only history byte count overflow"))?;
        if self.history_bytes > self.limits.max_key_history_bytes {
            return Err(CommandError::unsupported(
                "RocksDB point-only history exceeds the per-key byte limit",
            ));
        }
        Ok(())
    }
}

fn point_only_latest_state<'a>(
    point: SpoolPointRef<'a>,
    limits: LatestStateLimits,
) -> Result<LatestStateRef<'a>, CommandError> {
    match point.value_type {
        0 => Ok(LatestStateRef::Delete {
            sequence: point.sequence,
        }),
        1 if point.value.len() <= limits.max_resolved_value_bytes => Ok(LatestStateRef::Value {
            sequence: point.sequence,
            value: Cow::Borrowed(point.value),
        }),
        1 => Err(CommandError::unsupported(
            "RocksDB point-only value exceeds the resolved-value limit",
        )),
        7 => Ok(LatestStateRef::SingleDelete {
            sequence: point.sequence,
        }),
        value_type => Err(CommandError::unsupported(format!(
            "RocksDB point-only recovery does not support value type {value_type:#04x}"
        ))),
    }
}
