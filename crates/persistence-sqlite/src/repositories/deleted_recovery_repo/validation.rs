use std::collections::HashSet;

use crate::connection::{DbError, DbResult};

use super::{
    DeletedRecoveryAggregate, DeletedRecoveryRecord, RecoveryIssueRecord, RecoveryRangeRecord,
    RecoveryScanRecord,
};

const MAX_TEXT_BYTES: usize = 16 * 1024;
const MAX_WARNINGS: usize = 1_024;
const MAX_RECOVERIES: usize = 1_000_000;
const MAX_RANGES_PER_RECOVERY: usize = 1_000_000;
const MAX_ISSUES: usize = 100_000;

pub(super) fn validate_aggregate(aggregate: &DeletedRecoveryAggregate) -> DbResult<()> {
    validate_scan(&aggregate.scan)?;
    if aggregate.recoveries.len() > MAX_RECOVERIES {
        return invalid("recovery candidate count exceeds the persistence limit");
    }
    if aggregate.issues.len() > MAX_ISSUES {
        return invalid("recovery issue count exceeds the persistence limit");
    }
    if aggregate.scan.candidate_count != aggregate.recoveries.len() as u64 {
        return invalid("recovery scan candidate count does not match its candidates");
    }

    let mut recovery_ids = HashSet::with_capacity(aggregate.recoveries.len());
    for recovery in &aggregate.recoveries {
        if !recovery_ids.insert(recovery.id.as_str()) {
            return invalid("recovery candidate IDs must be unique within a scan");
        }
        validate_recovery(recovery)?;
    }
    validate_issues(&aggregate.issues)
}

pub(super) fn validate_scan(scan: &RecoveryScanRecord) -> DbResult<()> {
    validate_token("scan ID", &scan.id)?;
    validate_token("data-source ID", &scan.data_source_id)?;
    validate_text("parser version", &scan.parser_version, false)?;
    validate_optional_text("filesystem UUID", scan.filesystem_uuid.as_deref())?;
    if !matches!(scan.filesystem_type.as_str(), "ext4" | "xfs" | "ntfs") {
        return invalid("recovery filesystem type is unsupported");
    }
    if !matches!(scan.log_kind.as_str(), "internal_journal" | "internal_log") {
        return invalid("recovery log kind is invalid");
    }
    if !matches!(scan.state.as_str(), "complete" | "partial" | "failed") {
        return invalid("recovery scan state is invalid");
    }
    validate_sha256("snapshot identity", &scan.snapshot_identity_sha256)?;
    validate_text("scan start timestamp", &scan.started_at, false)?;
    validate_text("scan completion timestamp", &scan.completed_at, false)?;
    validate_warnings(&scan.warnings)
}

pub(super) fn validate_recovery(recovery: &DeletedRecoveryRecord) -> DbResult<()> {
    validate_token("recovery ID", &recovery.id)?;
    if recovery.inode.is_empty() || !recovery.inode.bytes().all(|byte| byte.is_ascii_digit()) {
        return invalid("recovery inode must be an unsigned decimal integer");
    }
    validate_optional_text("original path", recovery.original_path.as_deref())?;
    if recovery.mft_sequence.is_some_and(|value| value == 0) {
        return invalid("MFT sequence number must be non-zero when present");
    }
    if recovery
        .entry_type
        .as_deref()
        .is_some_and(|value| !matches!(value, "file" | "directory" | "symlink"))
    {
        return invalid("recovery entry type is invalid");
    }
    validate_text("recovery method", &recovery.recovery_method, false)?;
    if !recovery.confidence.is_finite() || !(0.0..=1.0).contains(&recovery.confidence) {
        return invalid("recovery confidence must be between zero and one");
    }
    if !matches!(
        recovery.completeness.as_str(),
        "metadata_only" | "partial" | "complete"
    ) {
        return invalid("recovery completeness is invalid");
    }
    validate_allocation_state(&recovery.allocation_state)?;
    validate_optional_text("transaction ID", recovery.transaction_id.as_deref())?;
    if let Some(hash) = recovery.content_md5.as_deref() {
        validate_hex_digest("content MD5", hash, 32)?;
    }
    if let Some(hash) = recovery.content_sha1.as_deref() {
        validate_hex_digest("content SHA-1", hash, 40)?;
    }
    if let Some(hash) = recovery.content_sha256.as_deref() {
        validate_sha256("content hash", hash)?;
    }
    if recovery.content_md5.is_some() != recovery.content_sha1.is_some() {
        return invalid("content MD5 and SHA-1 digests must be stored together");
    }
    validate_warnings(&recovery.warnings)?;
    validate_ranges(recovery)
}

fn validate_ranges(recovery: &DeletedRecoveryRecord) -> DbResult<()> {
    if recovery.ranges.len() > MAX_RANGES_PER_RECOVERY {
        return invalid("recovery range count exceeds the persistence limit");
    }
    let mut ordinals = HashSet::with_capacity(recovery.ranges.len());
    let mut content_bytes = 0u64;
    let mut content_ranges = Vec::new();
    for range in &recovery.ranges {
        if !ordinals.insert(range.ordinal) {
            return invalid("recovery range ordinals must be unique");
        }
        validate_range(range)?;
        if range.range_role == "content" {
            content_bytes = content_bytes
                .checked_add(range.length)
                .ok_or_else(|| DbError::System("recovery content length overflows".to_string()))?;
            content_ranges.push(range);
        }
    }
    if content_bytes != recovery.recoverable_bytes {
        return invalid("recoverable bytes must equal the persisted content-range length");
    }
    validate_content_claim(recovery, &mut content_ranges)
}

fn validate_content_claim(
    recovery: &DeletedRecoveryRecord,
    content_ranges: &mut Vec<&RecoveryRangeRecord>,
) -> DbResult<()> {
    if recovery.completeness == "metadata_only" {
        if !content_ranges.is_empty()
            || recovery.recoverable_bytes != 0
            || recovery.content_md5.is_some()
            || recovery.content_sha1.is_some()
            || recovery.content_sha256.is_some()
        {
            return invalid("metadata-only recovery cannot claim recovered content");
        }
        return Ok(());
    }

    content_ranges.sort_by_key(|range| (range.logical_offset, range.ordinal));
    let mut previous_end = None;
    for range in content_ranges.iter().copied() {
        if range.source_kind != "filesystem" {
            return invalid("recovered content must originate from the filesystem source");
        }
        if range.allocation_state != "free" {
            return invalid("recovered content ranges must be verified free");
        }
        if range.sha256.is_none() {
            return invalid("recovered content ranges must carry a SHA-256 digest");
        }
        let end = range
            .logical_offset
            .checked_add(range.length)
            .ok_or_else(|| DbError::System("recovery logical range overflows".to_string()))?;
        if end > recovery.declared_size {
            return invalid("recovered content range exceeds the declared file size");
        }
        if previous_end.is_some_and(|previous| previous > range.logical_offset) {
            return invalid("recovered content ranges must not overlap");
        }
        previous_end = Some(end);
    }

    match recovery.completeness.as_str() {
        "partial" => {
            if recovery.declared_size == 0
                || recovery.recoverable_bytes == 0
                || recovery.recoverable_bytes >= recovery.declared_size
            {
                return invalid("partial recovery must cover some but not all declared bytes");
            }
            if !matches!(
                recovery.allocation_state.as_str(),
                "free" | "partially_overwritten" | "unverified"
            ) {
                return invalid("partial recovery allocation state is inconsistent");
            }
            if recovery.content_md5.is_some()
                || recovery.content_sha1.is_some()
                || recovery.content_sha256.is_some()
            {
                return invalid("partial recovery cannot claim a complete-content digest");
            }
        }
        "complete" => validate_complete_content(recovery, content_ranges)?,
        _ => return invalid("recovery completeness is invalid"),
    }
    Ok(())
}

fn validate_complete_content(
    recovery: &DeletedRecoveryRecord,
    content_ranges: &[&RecoveryRangeRecord],
) -> DbResult<()> {
    if recovery.declared_size == 0 {
        if recovery.recoverable_bytes != 0 || !content_ranges.is_empty() {
            return invalid("complete empty recovery cannot contain content ranges");
        }
        if recovery.allocation_state != "free" {
            return invalid("complete empty recovery must be verified free");
        }
        if recovery.content_sha256.as_deref()
            != Some("e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855")
        {
            return invalid("complete empty recovery must carry the empty-content SHA-256 digest");
        }
        return Ok(());
    }
    if recovery.recoverable_bytes != recovery.declared_size {
        return invalid("complete recovery must cover every declared byte");
    }
    if recovery.allocation_state != "free" {
        return invalid("complete recovery must be verified free");
    }
    if recovery.content_sha256.is_none() {
        return invalid("complete recovery must carry a complete-content SHA-256 digest");
    }
    let mut expected_offset = 0u64;
    for range in content_ranges {
        if range.logical_offset != expected_offset {
            return invalid("complete recovery content ranges must be contiguous");
        }
        expected_offset = expected_offset
            .checked_add(range.length)
            .ok_or_else(|| DbError::System("recovery content coverage overflows".to_string()))?;
    }
    if expected_offset != recovery.declared_size {
        return invalid("complete recovery content ranges must cover the declared file size");
    }
    Ok(())
}

fn validate_range(range: &RecoveryRangeRecord) -> DbResult<()> {
    if !matches!(range.range_role.as_str(), "metadata" | "content") {
        return invalid("recovery range role is invalid");
    }
    if !matches!(range.source_kind.as_str(), "filesystem" | "journal" | "log") {
        return invalid("recovery range source kind is invalid");
    }
    if range.length == 0 {
        return invalid("recovery range length must be non-zero");
    }
    range
        .source_offset
        .checked_add(range.length)
        .ok_or_else(|| DbError::System("recovery source range overflows".to_string()))?;
    if let Some(physical_offset) = range.physical_offset {
        physical_offset
            .checked_add(range.length)
            .ok_or_else(|| DbError::System("recovery physical range overflows".to_string()))?;
    }
    range
        .logical_offset
        .checked_add(range.length)
        .ok_or_else(|| DbError::System("recovery logical range overflows".to_string()))?;
    validate_allocation_state(&range.allocation_state)?;
    if let Some(hash) = range.sha256.as_deref() {
        validate_sha256("recovery range hash", hash)?;
    }
    Ok(())
}

fn validate_issues(issues: &[RecoveryIssueRecord]) -> DbResult<()> {
    let mut ordinals = HashSet::with_capacity(issues.len());
    for issue in issues {
        if !ordinals.insert(issue.ordinal) {
            return invalid("recovery issue ordinals must be unique");
        }
        if !matches!(issue.severity.as_str(), "info" | "warning" | "error") {
            return invalid("recovery issue severity is invalid");
        }
        validate_token("recovery issue code", &issue.code)?;
        validate_text("recovery issue message", &issue.message, false)?;
    }
    Ok(())
}

fn validate_allocation_state(value: &str) -> DbResult<()> {
    if matches!(
        value,
        "unverified" | "free" | "allocated" | "partially_overwritten"
    ) {
        Ok(())
    } else {
        invalid("recovery allocation state is invalid")
    }
}

fn validate_warnings(warnings: &[String]) -> DbResult<()> {
    if warnings.len() > MAX_WARNINGS {
        return invalid("recovery warning count exceeds the persistence limit");
    }
    for warning in warnings {
        validate_text("recovery warning", warning, false)?;
    }
    Ok(())
}

fn validate_token(label: &str, value: &str) -> DbResult<()> {
    if value.is_empty()
        || value.len() > 256
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':'))
    {
        return invalid(format!("{label} is invalid"));
    }
    Ok(())
}

fn validate_optional_text(label: &str, value: Option<&str>) -> DbResult<()> {
    match value {
        Some(value) => validate_text(label, value, true),
        None => Ok(()),
    }
}

fn validate_text(label: &str, value: &str, allow_empty: bool) -> DbResult<()> {
    if (!allow_empty && value.is_empty()) || value.len() > MAX_TEXT_BYTES || value.contains('\0') {
        return invalid(format!("{label} is invalid"));
    }
    Ok(())
}

fn validate_sha256(label: &str, value: &str) -> DbResult<()> {
    validate_hex_digest(label, value, 64)
}

fn validate_hex_digest(label: &str, value: &str, expected_len: usize) -> DbResult<()> {
    if value.len() != expected_len
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
    {
        return invalid(format!("{label} must be a lowercase hexadecimal digest"));
    }
    Ok(())
}

fn invalid<T>(message: impl Into<String>) -> DbResult<T> {
    Err(DbError::System(message.into()))
}

pub(super) fn sqlite_u64(label: &str, value: u64) -> DbResult<i64> {
    i64::try_from(value)
        .map_err(|_| DbError::System(format!("{label} exceeds SQLite INTEGER range")))
}

pub(super) fn record_u64(label: &str, value: i64) -> DbResult<u64> {
    u64::try_from(value).map_err(|_| DbError::System(format!("stored {label} is negative")))
}
