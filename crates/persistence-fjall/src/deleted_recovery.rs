use crate::deleted_recovery_types::{
    DeletedRecoveryAggregate, DeletedRecoveryHashAlgorithm, DeletedRecoveryPage,
    DeletedRecoveryRecord, RecoveryIssueRecord, RecoveryScanRecord,
};
use crate::{
    codec::{encode_key, KeyPart},
    database::{DatabaseConfig, PersistMode, SourceReadSnapshot, WriteAction},
    errors::{FjallStorageError, Result},
    keyspaces::KeyspaceName,
    scan::ScanDirection,
    source_store::SourceStore,
    SnapshotEntry,
};
use std::path::Path;

mod codec;
mod write;

use codec::{
    decode_recovery, decode_scan, recovery_key, scan_key, tombstone_key, validate_aggregate,
    HASH_FAMILY, ISSUE_FAMILY, PARTITION_FAMILY, RANGE_FAMILY, SOURCE_FAMILY,
};

#[derive(Clone)]
pub struct DeletedRecoveryRecordStore {
    source_store: SourceStore,
}

impl DeletedRecoveryRecordStore {
    pub fn open(path: impl AsRef<Path>, config: DatabaseConfig) -> Result<Self> {
        Ok(Self {
            source_store: SourceStore::open(path, config)?,
        })
    }
    pub fn from_source_store(source_store: SourceStore) -> Self {
        Self { source_store }
    }
    pub fn source_store(&self) -> &SourceStore {
        &self.source_store
    }
    pub fn read_snapshot(&self) -> SourceReadSnapshot {
        self.source_store.database().read_snapshot()
    }

    pub fn replace_scan(
        &self,
        aggregate: &DeletedRecoveryAggregate,
        mode: PersistMode,
    ) -> Result<()> {
        validate_aggregate(aggregate)?;
        let snapshot = self.read_snapshot();
        let old = self.find_scan(
            &snapshot,
            &aggregate.scan.data_source_id,
            aggregate.scan.partition_index,
        )?;
        let mut actions = Vec::new();
        if let Some(old_scan) = old {
            self.delete_scan_actions(&snapshot, &old_scan, &mut actions)?;
        }
        self.write_scan_actions(&mut actions, aggregate)?;
        self.source_store.database().write_batch(&actions, mode)?;
        Ok(())
    }

    pub fn upsert(&self, aggregate: &DeletedRecoveryAggregate, mode: PersistMode) -> Result<()> {
        self.replace_scan(aggregate, mode)
    }

    pub fn list_by_partition(
        &self,
        snapshot: &SourceReadSnapshot,
        source_id: &str,
        partition_index: u32,
    ) -> Result<Option<DeletedRecoveryAggregate>> {
        let Some(scan) = self.find_scan(snapshot, source_id, partition_index)? else {
            return Ok(None);
        };
        self.read_aggregate(snapshot, &scan).map(Some)
    }

    pub fn get_scan(
        &self,
        snapshot: &SourceReadSnapshot,
        source_id: &str,
        scan_id: &str,
    ) -> Result<Option<RecoveryScanRecord>> {
        let key = scan_key(source_id, scan_id)?;
        let Some(envelope) =
            self.source_store
                .read_envelope(snapshot, KeyspaceName::RecoveryRecords, &key)?
        else {
            return Ok(None);
        };
        decode_scan(envelope, source_id, scan_id).map(Some)
    }

    pub fn find_recovery(
        &self,
        snapshot: &SourceReadSnapshot,
        source_id: &str,
        recovery_id: &str,
    ) -> Result<Option<(RecoveryScanRecord, DeletedRecoveryRecord)>> {
        let key = recovery_key(source_id, recovery_id)?;
        let Some(envelope) =
            self.source_store
                .read_envelope(snapshot, KeyspaceName::RecoveryRecords, &key)?
        else {
            return Ok(None);
        };
        let recovery = decode_recovery(envelope, source_id, recovery_id)?;
        let Some(scan) = self.get_scan(snapshot, source_id, &recovery.scan_id)? else {
            return Ok(None);
        };
        let recovery = self.read_recovery_ranges(snapshot, source_id, recovery)?;
        Ok(Some((scan, recovery)))
    }

    pub fn list_page(
        &self,
        snapshot: &SourceReadSnapshot,
        source_id: &str,
        partition_index: u32,
        cursor: Option<&[u8]>,
        direction: ScanDirection,
        limit: usize,
    ) -> Result<Option<DeletedRecoveryPage>> {
        let Some(scan) = self.find_scan(snapshot, source_id, partition_index)? else {
            return Ok(None);
        };
        if limit == 0 {
            return Ok(Some(DeletedRecoveryPage {
                scan: scan.clone(),
                recoveries: Vec::new(),
                issues: self.list_issues(snapshot, source_id, &scan.id)?,
                next_cursor: None,
            }));
        }
        let prefix = encode_key(
            SOURCE_FAMILY,
            &[
                KeyPart::Text(source_id.to_owned()),
                KeyPart::Text(scan.id.clone()),
            ],
        )?;
        let entries = snapshot.keyset(
            KeyspaceName::RecoveryIndexes,
            &prefix,
            cursor,
            direction,
            limit.saturating_add(1),
        )?;
        let has_more = entries.len() > limit;
        let entries = entries.into_iter().take(limit).collect::<Vec<_>>();
        let mut recoveries = Vec::with_capacity(entries.len());
        for entry in &entries {
            let Some((_, recovery)) = self.find_recovery_by_key(snapshot, entry, source_id)? else {
                continue;
            };
            recoveries.push(recovery);
        }
        let next_cursor = has_more
            .then(|| entries.last().map(|entry| entry.key.clone()))
            .flatten();
        Ok(Some(DeletedRecoveryPage {
            scan: scan.clone(),
            recoveries,
            issues: self.list_issues(snapshot, source_id, &scan.id)?,
            next_cursor,
        }))
    }

    #[allow(clippy::too_many_arguments)]
    pub fn list_by_hash(
        &self,
        snapshot: &SourceReadSnapshot,
        source_id: &str,
        algorithm: DeletedRecoveryHashAlgorithm,
        hash: &str,
        cursor: Option<&[u8]>,
        direction: ScanDirection,
        limit: usize,
    ) -> Result<DeletedRecoveryPage> {
        let prefix = encode_key(
            HASH_FAMILY,
            &[
                KeyPart::Text(source_id.to_owned()),
                KeyPart::Text(algorithm.as_str().to_owned()),
                KeyPart::Text(hash.to_owned()),
            ],
        )?;
        let entries = snapshot.keyset(
            KeyspaceName::RecoveryIndexes,
            &prefix,
            cursor,
            direction,
            limit.saturating_add(1),
        )?;
        let has_more = entries.len() > limit;
        let entries = entries.into_iter().take(limit).collect::<Vec<_>>();
        let mut recoveries = Vec::new();
        let mut scan = None;
        for entry in &entries {
            if let Some((candidate_scan, recovery)) =
                self.find_recovery_by_key(snapshot, entry, source_id)?
            {
                scan = Some(candidate_scan);
                recoveries.push(recovery);
            }
        }
        let Some(scan) = scan else {
            return Err(FjallStorageError::InvalidEnvelope(
                "recovery hash index is stale".to_owned(),
            ));
        };
        Ok(DeletedRecoveryPage {
            issues: self.list_issues(snapshot, source_id, &scan.id)?,
            scan,
            recoveries,
            next_cursor: has_more
                .then(|| entries.last().map(|entry| entry.key.clone()))
                .flatten(),
        })
    }

    pub fn delete_scan(
        &self,
        source_id: &str,
        partition_index: u32,
        mode: PersistMode,
    ) -> Result<bool> {
        let snapshot = self.read_snapshot();
        let Some(scan) = self.find_scan(&snapshot, source_id, partition_index)? else {
            return Ok(false);
        };
        let mut actions = Vec::new();
        self.delete_scan_actions(&snapshot, &scan, &mut actions)?;
        actions.push(WriteAction::Put {
            keyspace: KeyspaceName::Tombstones,
            key: tombstone_key(source_id, &scan.id)?,
            value: scan_key(source_id, &scan.id)?,
        });
        self.source_store.database().write_batch(&actions, mode)?;
        Ok(true)
    }

    fn find_scan(
        &self,
        snapshot: &SourceReadSnapshot,
        source_id: &str,
        partition_index: u32,
    ) -> Result<Option<RecoveryScanRecord>> {
        let prefix = encode_key(
            PARTITION_FAMILY,
            &[
                KeyPart::Text(source_id.to_owned()),
                KeyPart::U32(partition_index),
            ],
        )?;
        let entries = snapshot.keyset(
            KeyspaceName::RecoveryIndexes,
            &prefix,
            None,
            ScanDirection::Reverse,
            1,
        )?;
        let Some(entry) = entries.first() else {
            return Ok(None);
        };
        let Some(envelope) = self.source_store.read_envelope(
            snapshot,
            KeyspaceName::RecoveryRecords,
            &entry.value,
        )?
        else {
            return Ok(None);
        };
        decode_scan(envelope, source_id, "").map(Some)
    }

    fn read_aggregate(
        &self,
        snapshot: &SourceReadSnapshot,
        scan: &RecoveryScanRecord,
    ) -> Result<DeletedRecoveryAggregate> {
        let prefix = encode_key(
            SOURCE_FAMILY,
            &[
                KeyPart::Text(scan.data_source_id.clone()),
                KeyPart::Text(scan.id.clone()),
            ],
        )?;
        let entries = snapshot.prefix(
            KeyspaceName::RecoveryIndexes,
            &prefix,
            ScanDirection::Forward,
        )?;
        let mut recoveries = Vec::new();
        for entry in entries {
            if let Some((_, recovery)) =
                self.find_recovery_by_key(snapshot, &entry, &scan.data_source_id)?
            {
                recoveries.push(recovery);
            }
        }
        Ok(DeletedRecoveryAggregate {
            scan: scan.clone(),
            recoveries,
            issues: self.list_issues(snapshot, &scan.data_source_id, &scan.id)?,
        })
    }

    fn find_recovery_by_key(
        &self,
        snapshot: &SourceReadSnapshot,
        entry: &SnapshotEntry,
        source_id: &str,
    ) -> Result<Option<(RecoveryScanRecord, DeletedRecoveryRecord)>> {
        let Some(envelope) = self.source_store.read_envelope(
            snapshot,
            KeyspaceName::RecoveryRecords,
            &entry.value,
        )?
        else {
            return Ok(None);
        };
        let recovery = decode_recovery(envelope, source_id, "")?;
        let Some(scan) = self.get_scan(snapshot, source_id, &recovery.scan_id)? else {
            return Ok(None);
        };
        Ok(Some((
            scan,
            self.read_recovery_ranges(snapshot, source_id, recovery)?,
        )))
    }

    fn read_recovery_ranges(
        &self,
        snapshot: &SourceReadSnapshot,
        source_id: &str,
        mut recovery: DeletedRecoveryRecord,
    ) -> Result<DeletedRecoveryRecord> {
        let prefix = encode_key(
            RANGE_FAMILY,
            &[
                KeyPart::Text(source_id.to_owned()),
                KeyPart::Text(recovery.id.clone()),
            ],
        )?;
        recovery.ranges = snapshot
            .prefix(
                KeyspaceName::RecoveryRecords,
                &prefix,
                ScanDirection::Forward,
            )?
            .into_iter()
            .map(|entry| self.decode_range(entry.value, source_id, &recovery.id))
            .collect::<Result<Vec<_>>>()?;
        Ok(recovery)
    }

    fn list_issues(
        &self,
        snapshot: &SourceReadSnapshot,
        source_id: &str,
        scan_id: &str,
    ) -> Result<Vec<RecoveryIssueRecord>> {
        let prefix = encode_key(
            ISSUE_FAMILY,
            &[
                KeyPart::Text(source_id.to_owned()),
                KeyPart::Text(scan_id.to_owned()),
            ],
        )?;
        snapshot
            .prefix(
                KeyspaceName::RecoveryRecords,
                &prefix,
                ScanDirection::Forward,
            )?
            .into_iter()
            .map(|entry| self.decode_issue(entry.value, source_id, scan_id))
            .collect()
    }
}
