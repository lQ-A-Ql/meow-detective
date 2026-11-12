use persistence_sqlite::repositories::ceph_rocksdb_latest_state_repo::CephRocksdbLatestStateRecord;
use rocksdb_wire::LatestStateRef;
use transport::CommandError;

use crate::import_pipeline::ceph_rocksdb_digest::RecoveryDigests;
use crate::import_pipeline::ceph_rocksdb_spool::{
    SpoolPoint, SpoolPointRef, SpoolRange, SpoolSourceKind,
};

use super::latest_state_error;

const SUMMARY_SCHEMA_VERSION: u32 = 1;
const LATEST_ORIGIN_VALUE: u8 = 1;
const LATEST_ORIGIN_MERGE: u8 = 2;
const LATEST_DELETE: u8 = 0;
const LATEST_SINGLE_DELETE: u8 = 7;
const LATEST_RANGE_DELETE: u8 = 15;

pub(super) struct ColumnFamilySummary {
    pub(super) column_family_id: u32,
    pub(super) column_family_name: String,
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
    pub(super) fn new(column_family_id: u32, column_family_name: String) -> Self {
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

    pub(super) fn observe_point(&mut self, point: &SpoolPoint) -> Result<(), CommandError> {
        self.observe_point_ref(SpoolPointRef {
            column_family_id: point.column_family_id,
            user_key: &point.user_key,
            sequence: point.sequence,
            value_type: point.value_type,
            value: &point.value,
            provenance: point.provenance,
        })
    }

    pub(super) fn observe_point_ref(
        &mut self,
        point: SpoolPointRef<'_>,
    ) -> Result<(), CommandError> {
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

    pub(super) fn observe_range(&mut self, range: &SpoolRange) -> Result<(), CommandError> {
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

    pub(super) fn observe_hidden_versions(
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

    pub(super) fn observe_latest(
        &mut self,
        user_key: &[u8],
        state: Option<&LatestStateRef<'_>>,
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
                    *sequence,
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
                    .observe_deleted(user_key, *sequence, LATEST_DELETE);
            }
            LatestStateRef::SingleDelete { sequence } => {
                checked_increment(&mut self.deleted_key_count, "deleted key count")?;
                checked_increment(
                    &mut self.single_delete_decision_count,
                    "single-delete decision count",
                )?;
                self.digests
                    .observe_deleted(user_key, *sequence, LATEST_SINGLE_DELETE);
            }
            LatestStateRef::RangeDelete { sequence } => {
                checked_increment(&mut self.deleted_key_count, "deleted key count")?;
                checked_increment(
                    &mut self.range_delete_decision_count,
                    "range-delete decision count",
                )?;
                self.digests
                    .observe_deleted(user_key, *sequence, LATEST_RANGE_DELETE);
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

    pub(super) fn finish(
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

pub(super) fn is_hidden(point_sequence: u64, range_sequence: Option<u64>) -> bool {
    range_sequence.is_some_and(|sequence| sequence > point_sequence)
}

fn checked_increment(value: &mut u64, field: &'static str) -> Result<(), CommandError> {
    *value = value
        .checked_add(1)
        .ok_or_else(|| latest_state_error(format!("{field} overflow")))?;
    Ok(())
}
