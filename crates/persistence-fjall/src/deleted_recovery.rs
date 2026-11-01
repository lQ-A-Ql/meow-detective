use crate::deleted_recovery_types::{
    DeletedRecoveryAggregate, DeletedRecoveryHashAlgorithm, DeletedRecoveryPage,
    DeletedRecoveryRecord, RecoveryIssueRecord, RecoveryRangeRecord, RecoveryScanRecord,
};
use crate::{
    codec::{encode_key, KeyPart, VersionedEnvelope},
    database::{DatabaseConfig, PersistMode, SourceReadSnapshot, WriteAction},
    errors::{FjallStorageError, Result},
    keyspaces::KeyspaceName,
    scan::ScanDirection,
    source_store::SourceStore,
    SnapshotEntry,
};
use std::path::Path;

const SCHEMA_VERSION: u16 = 1;
const SCAN_KIND: &str = "filesystem_recovery_scan";
const RECOVERY_KIND: &str = "deleted_file_recovery";
const RANGE_KIND: &str = "deleted_file_recovery_range";
const ISSUE_KIND: &str = "filesystem_recovery_issue";
const SCAN_FAMILY: &str = "recovery.scan";
const PARTITION_FAMILY: &str = "recovery.partition";
const RECOVERY_FAMILY: &str = "recovery.record";
const SOURCE_FAMILY: &str = "recovery.source";
const HASH_FAMILY: &str = "recovery.hash";
const RANGE_FAMILY: &str = "recovery.range";
const ISSUE_FAMILY: &str = "recovery.issue";
const TOMBSTONE_FAMILY: &str = "recovery.tombstone";

impl DeletedRecoveryHashAlgorithm {
    fn as_str(self) -> &'static str {
        match self {
            Self::Md5 => "md5",
            Self::Sha1 => "sha1",
            Self::Sha256 => "sha256",
        }
    }
    fn value(self, record: &DeletedRecoveryRecord) -> Option<&str> {
        match self {
            Self::Md5 => record.content_md5.as_deref(),
            Self::Sha1 => record.content_sha1.as_deref(),
            Self::Sha256 => record.content_sha256.as_deref(),
        }
    }
}

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

    fn write_scan_actions(
        &self,
        actions: &mut Vec<WriteAction>,
        aggregate: &DeletedRecoveryAggregate,
    ) -> Result<()> {
        let scan = &aggregate.scan;
        let key = scan_key(&scan.data_source_id, &scan.id)?;
        let envelope = VersionedEnvelope::from_serializable(
            SCAN_KIND,
            SCHEMA_VERSION,
            &scan.id,
            &scan.data_source_id,
            1,
            scan,
        )?;
        actions.push(WriteAction::Put {
            keyspace: KeyspaceName::RecoveryRecords,
            key: key.clone(),
            value: envelope.encode()?,
        });
        let partition_key = encode_key(
            PARTITION_FAMILY,
            &[
                KeyPart::Text(scan.data_source_id.clone()),
                KeyPart::U32(scan.partition_index),
                KeyPart::Text(scan.id.clone()),
            ],
        )?;
        actions.push(WriteAction::Put {
            keyspace: KeyspaceName::RecoveryIndexes,
            key: partition_key,
            value: key,
        });
        for recovery in &aggregate.recoveries {
            self.write_recovery_actions(actions, scan, recovery)?;
        }
        for issue in &aggregate.issues {
            self.write_issue_actions(actions, scan, issue)?;
        }
        Ok(())
    }

    fn write_recovery_actions(
        &self,
        actions: &mut Vec<WriteAction>,
        scan: &RecoveryScanRecord,
        recovery: &DeletedRecoveryRecord,
    ) -> Result<()> {
        if recovery.scan_id != scan.id {
            return Err(FjallStorageError::InvalidEnvelope(
                "recovery scan identity mismatch".to_owned(),
            ));
        }
        let key = recovery_key(&scan.data_source_id, &recovery.id)?;
        let envelope = VersionedEnvelope::from_serializable(
            RECOVERY_KIND,
            SCHEMA_VERSION,
            &recovery.id,
            &scan.data_source_id,
            1,
            recovery,
        )?;
        actions.push(WriteAction::Put {
            keyspace: KeyspaceName::RecoveryRecords,
            key: key.clone(),
            value: envelope.encode()?,
        });
        let source_key = encode_key(
            SOURCE_FAMILY,
            &[
                KeyPart::Text(scan.data_source_id.clone()),
                KeyPart::Text(scan.id.clone()),
                KeyPart::Text(recovery.id.clone()),
            ],
        )?;
        actions.push(WriteAction::Put {
            keyspace: KeyspaceName::RecoveryIndexes,
            key: source_key,
            value: key.clone(),
        });
        for algorithm in [
            DeletedRecoveryHashAlgorithm::Md5,
            DeletedRecoveryHashAlgorithm::Sha1,
            DeletedRecoveryHashAlgorithm::Sha256,
        ] {
            if let Some(hash) = algorithm.value(recovery).filter(|value| !value.is_empty()) {
                let hash_key = encode_key(
                    HASH_FAMILY,
                    &[
                        KeyPart::Text(scan.data_source_id.clone()),
                        KeyPart::Text(algorithm.as_str().to_owned()),
                        KeyPart::Text(hash.to_owned()),
                        KeyPart::Text(recovery.id.clone()),
                    ],
                )?;
                actions.push(WriteAction::Put {
                    keyspace: KeyspaceName::RecoveryIndexes,
                    key: hash_key,
                    value: key.clone(),
                });
            }
        }
        for range in &recovery.ranges {
            let range_key = encode_key(
                RANGE_FAMILY,
                &[
                    KeyPart::Text(scan.data_source_id.clone()),
                    KeyPart::Text(recovery.id.clone()),
                    KeyPart::U32(range.ordinal),
                ],
            )?;
            let envelope = VersionedEnvelope::from_serializable(
                RANGE_KIND,
                SCHEMA_VERSION,
                format!("{}:{}", recovery.id, range.ordinal),
                &scan.data_source_id,
                1,
                range,
            )?;
            actions.push(WriteAction::Put {
                keyspace: KeyspaceName::RecoveryRecords,
                key: range_key,
                value: envelope.encode()?,
            });
        }
        Ok(())
    }

    fn write_issue_actions(
        &self,
        actions: &mut Vec<WriteAction>,
        scan: &RecoveryScanRecord,
        issue: &RecoveryIssueRecord,
    ) -> Result<()> {
        let key = encode_key(
            ISSUE_FAMILY,
            &[
                KeyPart::Text(scan.data_source_id.clone()),
                KeyPart::Text(scan.id.clone()),
                KeyPart::U32(issue.ordinal),
            ],
        )?;
        let envelope = VersionedEnvelope::from_serializable(
            ISSUE_KIND,
            SCHEMA_VERSION,
            format!("{}:{}", scan.id, issue.ordinal),
            &scan.data_source_id,
            1,
            issue,
        )?;
        actions.push(WriteAction::Put {
            keyspace: KeyspaceName::RecoveryRecords,
            key,
            value: envelope.encode()?,
        });
        Ok(())
    }

    fn delete_scan_actions(
        &self,
        snapshot: &SourceReadSnapshot,
        scan: &RecoveryScanRecord,
        actions: &mut Vec<WriteAction>,
    ) -> Result<()> {
        actions.push(WriteAction::Delete {
            keyspace: KeyspaceName::RecoveryRecords,
            key: scan_key(&scan.data_source_id, &scan.id)?,
        });
        actions.push(WriteAction::Delete {
            keyspace: KeyspaceName::RecoveryIndexes,
            key: encode_key(
                PARTITION_FAMILY,
                &[
                    KeyPart::Text(scan.data_source_id.clone()),
                    KeyPart::U32(scan.partition_index),
                    KeyPart::Text(scan.id.clone()),
                ],
            )?,
        });
        let aggregate = self.read_aggregate(snapshot, scan)?;
        for recovery in aggregate.recoveries {
            let key = recovery_key(&scan.data_source_id, &recovery.id)?;
            actions.push(WriteAction::Delete {
                keyspace: KeyspaceName::RecoveryRecords,
                key,
            });
            actions.push(WriteAction::Delete {
                keyspace: KeyspaceName::RecoveryIndexes,
                key: encode_key(
                    SOURCE_FAMILY,
                    &[
                        KeyPart::Text(scan.data_source_id.clone()),
                        KeyPart::Text(scan.id.clone()),
                        KeyPart::Text(recovery.id.clone()),
                    ],
                )?,
            });
            for algorithm in [
                DeletedRecoveryHashAlgorithm::Md5,
                DeletedRecoveryHashAlgorithm::Sha1,
                DeletedRecoveryHashAlgorithm::Sha256,
            ] {
                if let Some(hash) = algorithm.value(&recovery).filter(|value| !value.is_empty()) {
                    actions.push(WriteAction::Delete {
                        keyspace: KeyspaceName::RecoveryIndexes,
                        key: encode_key(
                            HASH_FAMILY,
                            &[
                                KeyPart::Text(scan.data_source_id.clone()),
                                KeyPart::Text(algorithm.as_str().to_owned()),
                                KeyPart::Text(hash.to_owned()),
                                KeyPart::Text(recovery.id.clone()),
                            ],
                        )?,
                    });
                }
            }
            for range in recovery.ranges {
                actions.push(WriteAction::Delete {
                    keyspace: KeyspaceName::RecoveryRecords,
                    key: encode_key(
                        RANGE_FAMILY,
                        &[
                            KeyPart::Text(scan.data_source_id.clone()),
                            KeyPart::Text(recovery.id.clone()),
                            KeyPart::U32(range.ordinal),
                        ],
                    )?,
                });
            }
        }
        for issue in aggregate.issues {
            actions.push(WriteAction::Delete {
                keyspace: KeyspaceName::RecoveryRecords,
                key: encode_key(
                    ISSUE_FAMILY,
                    &[
                        KeyPart::Text(scan.data_source_id.clone()),
                        KeyPart::Text(scan.id.clone()),
                        KeyPart::U32(issue.ordinal),
                    ],
                )?,
            });
        }
        Ok(())
    }

    fn decode_range(
        &self,
        bytes: Vec<u8>,
        source_id: &str,
        recovery_id: &str,
    ) -> Result<RecoveryRangeRecord> {
        let envelope = VersionedEnvelope::decode(&bytes)?;
        if envelope.record_kind != RANGE_KIND || envelope.source_id != source_id {
            return Err(FjallStorageError::InvalidEnvelope(
                "recovery range identity mismatch".to_owned(),
            ));
        }
        let range: RecoveryRangeRecord = envelope.decode_payload()?;
        if envelope.record_id != format!("{}:{}", recovery_id, range.ordinal) {
            return Err(FjallStorageError::InvalidEnvelope(
                "recovery range identity mismatch".to_owned(),
            ));
        }
        Ok(range)
    }

    fn decode_issue(
        &self,
        bytes: Vec<u8>,
        source_id: &str,
        scan_id: &str,
    ) -> Result<RecoveryIssueRecord> {
        let envelope = VersionedEnvelope::decode(&bytes)?;
        if envelope.record_kind != ISSUE_KIND || envelope.source_id != source_id {
            return Err(FjallStorageError::InvalidEnvelope(
                "recovery issue identity mismatch".to_owned(),
            ));
        }
        let issue: RecoveryIssueRecord = envelope.decode_payload()?;
        if envelope.record_id != format!("{}:{}", scan_id, issue.ordinal) {
            return Err(FjallStorageError::InvalidEnvelope(
                "recovery issue identity mismatch".to_owned(),
            ));
        }
        Ok(issue)
    }
}

fn validate_aggregate(aggregate: &DeletedRecoveryAggregate) -> Result<()> {
    let scan = &aggregate.scan;
    if scan.id.is_empty()
        || scan.data_source_id.is_empty()
        || scan.filesystem_type.is_empty()
        || scan.parser_version.is_empty()
        || scan.snapshot_identity_sha256.is_empty()
    {
        return Err(FjallStorageError::InvalidEnvelope(
            "recovery scan identity fields are empty".to_owned(),
        ));
    }
    if aggregate.recoveries.iter().any(|record| {
        record.id.is_empty()
            || record.scan_id != scan.id
            || record.inode.is_empty()
            || !record.confidence.is_finite()
            || !(0.0..=1.0).contains(&record.confidence)
    }) {
        return Err(FjallStorageError::InvalidEnvelope(
            "invalid deleted recovery record".to_owned(),
        ));
    }
    Ok(())
}

fn decode_scan(
    envelope: VersionedEnvelope,
    source_id: &str,
    scan_id: &str,
) -> Result<RecoveryScanRecord> {
    if envelope.record_kind != SCAN_KIND
        || envelope.schema_version != SCHEMA_VERSION
        || envelope.source_id != source_id
        || (!scan_id.is_empty() && envelope.record_id != scan_id)
    {
        return Err(FjallStorageError::InvalidEnvelope(
            "recovery scan identity mismatch".to_owned(),
        ));
    }
    let scan: RecoveryScanRecord = envelope.decode_payload()?;
    if scan.id != envelope.record_id || scan.data_source_id != envelope.source_id {
        return Err(FjallStorageError::InvalidEnvelope(
            "recovery scan identity mismatch".to_owned(),
        ));
    }
    Ok(scan)
}

fn decode_recovery(
    envelope: VersionedEnvelope,
    source_id: &str,
    recovery_id: &str,
) -> Result<DeletedRecoveryRecord> {
    if envelope.record_kind != RECOVERY_KIND
        || envelope.schema_version != SCHEMA_VERSION
        || envelope.source_id != source_id
        || (!recovery_id.is_empty() && envelope.record_id != recovery_id)
    {
        return Err(FjallStorageError::InvalidEnvelope(
            "deleted recovery identity mismatch".to_owned(),
        ));
    }
    let mut recovery: DeletedRecoveryRecord = envelope.decode_payload()?;
    if recovery.id != envelope.record_id {
        return Err(FjallStorageError::InvalidEnvelope(
            "deleted recovery identity mismatch".to_owned(),
        ));
    }
    recovery.ranges.clear();
    Ok(recovery)
}

fn scan_key(source_id: &str, scan_id: &str) -> Result<Vec<u8>> {
    encode_key(
        SCAN_FAMILY,
        &[
            KeyPart::Text(source_id.to_owned()),
            KeyPart::Text(scan_id.to_owned()),
        ],
    )
}
fn recovery_key(source_id: &str, recovery_id: &str) -> Result<Vec<u8>> {
    encode_key(
        RECOVERY_FAMILY,
        &[
            KeyPart::Text(source_id.to_owned()),
            KeyPart::Text(recovery_id.to_owned()),
        ],
    )
}
fn tombstone_key(source_id: &str, scan_id: &str) -> Result<Vec<u8>> {
    encode_key(
        TOMBSTONE_FAMILY,
        &[
            KeyPart::Text(source_id.to_owned()),
            KeyPart::Text(scan_id.to_owned()),
        ],
    )
}
