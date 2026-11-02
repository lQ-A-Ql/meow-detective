use super::codec::{
    recovery_key, scan_key, HASH_FAMILY, ISSUE_FAMILY, ISSUE_KIND, PARTITION_FAMILY, RANGE_FAMILY,
    RECOVERY_KIND, SCAN_KIND, SCHEMA_VERSION, SOURCE_FAMILY,
};
use super::DeletedRecoveryRecordStore;
use crate::deleted_recovery_types::{
    DeletedRecoveryAggregate, DeletedRecoveryHashAlgorithm, DeletedRecoveryRecord,
    RecoveryIssueRecord, RecoveryScanRecord,
};
use crate::{
    codec::{encode_key, KeyPart, VersionedEnvelope},
    database::{SourceReadSnapshot, WriteAction},
    errors::{FjallStorageError, Result},
    keyspaces::KeyspaceName,
};

impl DeletedRecoveryRecordStore {
    pub(super) fn write_scan_actions(
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
                super::codec::RANGE_KIND,
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

    pub(super) fn delete_scan_actions(
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
}
