use super::DeletedRecoveryRecordStore;
use crate::deleted_recovery_types::{
    DeletedRecoveryAggregate, DeletedRecoveryHashAlgorithm, DeletedRecoveryRecord,
    RecoveryIssueRecord, RecoveryRangeRecord, RecoveryScanRecord,
};
use crate::{
    codec::{encode_key, KeyPart, VersionedEnvelope},
    errors::{FjallStorageError, Result},
};

pub(super) const SCHEMA_VERSION: u16 = 1;
pub(super) const SCAN_KIND: &str = "filesystem_recovery_scan";
pub(super) const RECOVERY_KIND: &str = "deleted_file_recovery";
pub(super) const RANGE_KIND: &str = "deleted_file_recovery_range";
pub(super) const ISSUE_KIND: &str = "filesystem_recovery_issue";
pub(super) const SCAN_FAMILY: &str = "recovery.scan";
pub(super) const PARTITION_FAMILY: &str = "recovery.partition";
pub(super) const RECOVERY_FAMILY: &str = "recovery.record";
pub(super) const SOURCE_FAMILY: &str = "recovery.source";
pub(super) const HASH_FAMILY: &str = "recovery.hash";
pub(super) const RANGE_FAMILY: &str = "recovery.range";
pub(super) const ISSUE_FAMILY: &str = "recovery.issue";
pub(super) const TOMBSTONE_FAMILY: &str = "recovery.tombstone";

impl DeletedRecoveryHashAlgorithm {
    pub(super) fn as_str(self) -> &'static str {
        match self {
            Self::Md5 => "md5",
            Self::Sha1 => "sha1",
            Self::Sha256 => "sha256",
        }
    }

    pub(super) fn value(self, record: &DeletedRecoveryRecord) -> Option<&str> {
        match self {
            Self::Md5 => record.content_md5.as_deref(),
            Self::Sha1 => record.content_sha1.as_deref(),
            Self::Sha256 => record.content_sha256.as_deref(),
        }
    }
}

impl DeletedRecoveryRecordStore {
    pub(super) fn decode_range(
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

    pub(super) fn decode_issue(
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

pub(super) fn validate_aggregate(aggregate: &DeletedRecoveryAggregate) -> Result<()> {
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

pub(super) fn decode_scan(
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

pub(super) fn decode_recovery(
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

pub(super) fn scan_key(source_id: &str, scan_id: &str) -> Result<Vec<u8>> {
    encode_key(
        SCAN_FAMILY,
        &[
            KeyPart::Text(source_id.to_owned()),
            KeyPart::Text(scan_id.to_owned()),
        ],
    )
}

pub(super) fn recovery_key(source_id: &str, recovery_id: &str) -> Result<Vec<u8>> {
    encode_key(
        RECOVERY_FAMILY,
        &[
            KeyPart::Text(source_id.to_owned()),
            KeyPart::Text(recovery_id.to_owned()),
        ],
    )
}

pub(super) fn tombstone_key(source_id: &str, scan_id: &str) -> Result<Vec<u8>> {
    encode_key(
        TOMBSTONE_FAMILY,
        &[
            KeyPart::Text(source_id.to_owned()),
            KeyPart::Text(scan_id.to_owned()),
        ],
    )
}
