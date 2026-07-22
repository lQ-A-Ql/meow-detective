use std::collections::HashMap;

use rusqlite::{params, Connection, OptionalExtension, Row};

use crate::connection::{DbError, DbResult};

use super::validation::{record_u64, validate_aggregate, validate_recovery, validate_scan};
use super::{
    DeletedRecoveryAggregate, DeletedRecoveryPageRecord, DeletedRecoveryRecord,
    DeletedRecoveryRepo, RecoveryIssueRecord, RecoveryRangeRecord, RecoveryScanRecord,
};

impl DeletedRecoveryRepo<'_> {
    pub fn find_recovery(
        &self,
        data_source_id: &str,
        recovery_id: &str,
    ) -> DbResult<Option<(RecoveryScanRecord, DeletedRecoveryRecord)>> {
        let Some(scan) = find_scan_for_recovery(self.conn, data_source_id, recovery_id)? else {
            return Ok(None);
        };
        let ranges = list_ranges_for_recovery(self.conn, &scan.id, recovery_id)?;
        let Some(recovery) = find_recovery_record(self.conn, &scan.id, recovery_id, ranges)? else {
            return Ok(None);
        };
        validate_scan(&scan)?;
        validate_recovery(&recovery)?;
        Ok(Some((scan, recovery)))
    }

    pub fn list_by_partition(
        &self,
        data_source_id: &str,
        partition_index: u32,
    ) -> DbResult<Option<DeletedRecoveryAggregate>> {
        let Some(scan) = find_scan(self.conn, data_source_id, partition_index)? else {
            return Ok(None);
        };
        let recoveries = list_recoveries(self.conn, &scan.id)?;
        let issues = list_issues(self.conn, &scan.id)?;
        let aggregate = DeletedRecoveryAggregate {
            scan,
            recoveries,
            issues,
        };
        validate_aggregate(&aggregate)?;
        Ok(Some(aggregate))
    }

    pub fn list_page(
        &self,
        data_source_id: &str,
        partition_index: u32,
        offset: u64,
        limit: u32,
    ) -> DbResult<Option<DeletedRecoveryPageRecord>> {
        if limit == 0 || limit > 10_000 {
            return Err(DbError::System(
                "recovery page limit must be between 1 and 10000".to_string(),
            ));
        }
        let Some(scan) = find_scan(self.conn, data_source_id, partition_index)? else {
            return Ok(None);
        };
        let total = scan.candidate_count;
        let recoveries = list_recoveries_page(self.conn, &scan.id, offset, limit)?;
        let issues = list_issues(self.conn, &scan.id)?;
        Ok(Some(DeletedRecoveryPageRecord {
            scan,
            recoveries,
            issues,
            offset,
            limit,
            total,
        }))
    }
}

fn find_scan_for_recovery(
    conn: &Connection,
    data_source_id: &str,
    recovery_id: &str,
) -> DbResult<Option<RecoveryScanRecord>> {
    conn.query_row(
        "SELECT scan.id, scan.data_source_id, scan.partition_index, scan.filesystem_type,
                scan.filesystem_uuid, scan.parser_version, scan.log_kind,
                scan.snapshot_identity_sha256, scan.state, scan.transaction_count,
                scan.candidate_count, scan.warnings_json, scan.started_at, scan.completed_at
         FROM filesystem_recovery_scans AS scan
         INNER JOIN deleted_file_recoveries AS recovery ON recovery.scan_id = scan.id
         WHERE scan.data_source_id = ?1 AND recovery.id = ?2",
        params![data_source_id, recovery_id],
        scan_from_row,
    )
    .optional()
    .map_err(Into::into)
}

fn find_recovery_record(
    conn: &Connection,
    scan_id: &str,
    recovery_id: &str,
    ranges: Vec<RecoveryRangeRecord>,
) -> DbResult<Option<DeletedRecoveryRecord>> {
    let mut statement = conn.prepare(
        "SELECT id, inode, original_path, entry_type, mode, mft_sequence, deleted_at_unix,
                declared_size, recoverable_bytes, completeness, recovery_method,
                confidence, allocation_state, transaction_id, log_sequence,
                log_cycle, content_sha256, warnings_json
         FROM deleted_file_recoveries WHERE scan_id = ?1 AND id = ?2",
    )?;
    let mut rows = statement.query(params![scan_id, recovery_id])?;
    let Some(row) = rows.next()? else {
        return Ok(None);
    };
    Ok(Some(recovery_from_row(row, ranges)?))
}

fn find_scan(
    conn: &Connection,
    data_source_id: &str,
    partition_index: u32,
) -> DbResult<Option<RecoveryScanRecord>> {
    let mut statement = conn.prepare(
        "SELECT id, data_source_id, partition_index, filesystem_type, filesystem_uuid,
                parser_version, log_kind, snapshot_identity_sha256, state,
                transaction_count, candidate_count, warnings_json, started_at, completed_at
         FROM filesystem_recovery_scans
         WHERE data_source_id = ?1 AND partition_index = ?2
         ORDER BY completed_at DESC, id DESC
         LIMIT 1",
    )?;
    let mut rows = statement.query(params![data_source_id, partition_index])?;
    let Some(row) = rows.next()? else {
        return Ok(None);
    };
    Ok(Some(scan_from_row(row)?))
}

fn scan_from_row(row: &Row<'_>) -> rusqlite::Result<RecoveryScanRecord> {
    Ok(RecoveryScanRecord {
        id: row.get(0)?,
        data_source_id: row.get(1)?,
        partition_index: row.get(2)?,
        filesystem_type: row.get(3)?,
        filesystem_uuid: row.get(4)?,
        parser_version: row.get(5)?,
        log_kind: row.get(6)?,
        snapshot_identity_sha256: row.get(7)?,
        state: row.get(8)?,
        transaction_count: record_u64("transaction count", row.get(9)?).map_err(db_to_sqlite)?,
        candidate_count: record_u64("candidate count", row.get(10)?).map_err(db_to_sqlite)?,
        warnings: decode_warnings(row.get(11)?).map_err(db_to_sqlite)?,
        started_at: row.get(12)?,
        completed_at: row.get(13)?,
    })
}

fn list_recoveries(conn: &Connection, scan_id: &str) -> DbResult<Vec<DeletedRecoveryRecord>> {
    list_recoveries_query(conn, scan_id, None)
}

fn list_recoveries_page(
    conn: &Connection,
    scan_id: &str,
    offset: u64,
    limit: u32,
) -> DbResult<Vec<DeletedRecoveryRecord>> {
    list_recoveries_query(conn, scan_id, Some((offset, limit)))
}

fn list_recoveries_query(
    conn: &Connection,
    scan_id: &str,
    page: Option<(u64, u32)>,
) -> DbResult<Vec<DeletedRecoveryRecord>> {
    let ranges = list_ranges(conn, scan_id, page)?;
    let page_clause = if page.is_some() {
        " LIMIT ?2 OFFSET ?3"
    } else {
        ""
    };
    let sql = format!(
        "SELECT id, inode, original_path, entry_type, mode, mft_sequence, deleted_at_unix,
                declared_size, recoverable_bytes,
                completeness, recovery_method, confidence, allocation_state,
                transaction_id, log_sequence, log_cycle, content_sha256, warnings_json
         FROM deleted_file_recoveries
         WHERE scan_id = ?1
         ORDER BY CAST(inode AS INTEGER), id{page_clause}"
    );
    let mut statement = conn.prepare(&sql)?;
    let mut rows = match page {
        Some((offset, limit)) => statement.query(params![scan_id, limit, offset])?,
        None => statement.query(params![scan_id])?,
    };
    let mut result = Vec::new();
    while let Some(row) = rows.next()? {
        let values = RecoveryRowValues::from_row(row)?;
        let recovery_ranges = ranges.get(&values.id).cloned().unwrap_or_default();
        result.push(values.into_record(recovery_ranges)?);
    }
    Ok(result)
}

fn recovery_from_row(
    row: &Row<'_>,
    ranges: Vec<RecoveryRangeRecord>,
) -> DbResult<DeletedRecoveryRecord> {
    RecoveryRowValues::from_row(row)?.into_record(ranges)
}

struct RecoveryRowValues {
    id: String,
    inode: String,
    original_path: Option<String>,
    entry_type: Option<String>,
    mode: Option<u16>,
    mft_sequence: Option<u16>,
    deleted_at_unix: Option<i64>,
    declared_size: i64,
    recoverable_bytes: i64,
    completeness: String,
    recovery_method: String,
    confidence: f64,
    allocation_state: String,
    transaction_id: Option<String>,
    log_sequence: Option<i64>,
    log_cycle: Option<i64>,
    content_sha256: Option<String>,
    warnings_json: String,
}

impl RecoveryRowValues {
    fn from_row(row: &Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            id: row.get(0)?,
            inode: row.get(1)?,
            original_path: row.get(2)?,
            entry_type: row.get(3)?,
            mode: row.get(4)?,
            mft_sequence: row.get(5)?,
            deleted_at_unix: row.get(6)?,
            declared_size: row.get(7)?,
            recoverable_bytes: row.get(8)?,
            completeness: row.get(9)?,
            recovery_method: row.get(10)?,
            confidence: row.get(11)?,
            allocation_state: row.get(12)?,
            transaction_id: row.get(13)?,
            log_sequence: row.get(14)?,
            log_cycle: row.get(15)?,
            content_sha256: row.get(16)?,
            warnings_json: row.get(17)?,
        })
    }

    fn into_record(self, ranges: Vec<RecoveryRangeRecord>) -> DbResult<DeletedRecoveryRecord> {
        Ok(DeletedRecoveryRecord {
            id: self.id,
            inode: self.inode,
            original_path: self.original_path,
            entry_type: self.entry_type,
            mode: self.mode,
            mft_sequence: self.mft_sequence,
            deleted_at_unix: self
                .deleted_at_unix
                .map(|value| record_u64("deletion timestamp", value))
                .transpose()?,
            declared_size: record_u64("declared size", self.declared_size)?,
            recoverable_bytes: record_u64("recoverable bytes", self.recoverable_bytes)?,
            completeness: self.completeness,
            recovery_method: self.recovery_method,
            confidence: self.confidence,
            allocation_state: self.allocation_state,
            transaction_id: self.transaction_id,
            log_sequence: self
                .log_sequence
                .map(|value| record_u64("log sequence", value))
                .transpose()?,
            log_cycle: self
                .log_cycle
                .map(|value| record_u64("log cycle", value))
                .transpose()?,
            content_sha256: self.content_sha256,
            warnings: decode_warnings(self.warnings_json)?,
            ranges,
        })
    }
}

fn list_ranges_for_recovery(
    conn: &Connection,
    scan_id: &str,
    recovery_id: &str,
) -> DbResult<Vec<RecoveryRangeRecord>> {
    let mut statement = conn.prepare(
        "SELECT range.ordinal, range.range_role, range.source_kind, range.logical_offset,
                range.source_offset, range.physical_offset, range.length,
                range.allocation_state, range.sha256
         FROM deleted_file_recovery_ranges AS range
         INNER JOIN deleted_file_recoveries AS recovery ON recovery.id = range.recovery_id
         WHERE recovery.scan_id = ?1 AND range.recovery_id = ?2
         ORDER BY range.ordinal",
    )?;
    let mut rows = statement.query(params![scan_id, recovery_id])?;
    let mut ranges = Vec::new();
    while let Some(row) = rows.next()? {
        ranges.push(RecoveryRangeRecord {
            ordinal: row.get(0)?,
            range_role: row.get(1)?,
            source_kind: row.get(2)?,
            logical_offset: record_u64("logical offset", row.get(3)?)?,
            source_offset: record_u64("source offset", row.get(4)?)?,
            physical_offset: row
                .get::<_, Option<i64>>(5)?
                .map(|value| record_u64("physical offset", value))
                .transpose()?,
            length: record_u64("range length", row.get(6)?)?,
            allocation_state: row.get(7)?,
            sha256: row.get(8)?,
        });
    }
    Ok(ranges)
}

fn list_ranges(
    conn: &Connection,
    scan_id: &str,
    page: Option<(u64, u32)>,
) -> DbResult<HashMap<String, Vec<RecoveryRangeRecord>>> {
    let page_clause = if page.is_some() {
        " LIMIT ?2 OFFSET ?3"
    } else {
        ""
    };
    let sql = format!(
        "SELECT range.recovery_id, range.ordinal, range.range_role, range.source_kind,
                range.logical_offset, range.source_offset, range.physical_offset,
                range.length, range.allocation_state, range.sha256
         FROM deleted_file_recovery_ranges AS range
         WHERE range.recovery_id IN (
             SELECT id FROM deleted_file_recoveries
             WHERE scan_id = ?1
             ORDER BY CAST(inode AS INTEGER), id{page_clause}
         )
         ORDER BY range.recovery_id, range.ordinal"
    );
    let mut statement = conn.prepare(&sql)?;
    let mut rows = match page {
        Some((offset, limit)) => statement.query(params![scan_id, limit, offset])?,
        None => statement.query(params![scan_id])?,
    };
    let mut ranges = HashMap::<String, Vec<RecoveryRangeRecord>>::new();
    while let Some(row) = rows.next()? {
        let (
            id,
            ordinal,
            role,
            source,
            logical,
            source_offset,
            physical,
            length,
            allocation,
            sha256,
        ) = (
            row.get::<_, String>(0)?,
            row.get::<_, u32>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, i64>(4)?,
            row.get::<_, i64>(5)?,
            row.get::<_, Option<i64>>(6)?,
            row.get::<_, i64>(7)?,
            row.get::<_, String>(8)?,
            row.get::<_, Option<String>>(9)?,
        );
        ranges.entry(id).or_default().push(RecoveryRangeRecord {
            ordinal,
            range_role: role,
            source_kind: source,
            logical_offset: record_u64("logical offset", logical)?,
            source_offset: record_u64("source offset", source_offset)?,
            physical_offset: physical
                .map(|value| record_u64("physical offset", value))
                .transpose()?,
            length: record_u64("range length", length)?,
            allocation_state: allocation,
            sha256,
        });
    }
    Ok(ranges)
}

fn list_issues(conn: &Connection, scan_id: &str) -> DbResult<Vec<RecoveryIssueRecord>> {
    let mut statement = conn.prepare(
        "SELECT ordinal, severity, code, message, log_offset, sequence
         FROM filesystem_recovery_issues
         WHERE scan_id = ?1
         ORDER BY ordinal",
    )?;
    let rows = statement.query_map([scan_id], |row| {
        Ok((
            row.get::<_, u32>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, Option<i64>>(4)?,
            row.get::<_, Option<i64>>(5)?,
        ))
    })?;
    let mut result = Vec::new();
    for row in rows {
        let (ordinal, severity, code, message, log_offset, sequence) = row?;
        result.push(RecoveryIssueRecord {
            ordinal,
            severity,
            code,
            message,
            log_offset: log_offset
                .map(|value| record_u64("issue log offset", value))
                .transpose()?,
            sequence: sequence
                .map(|value| record_u64("issue sequence", value))
                .transpose()?,
        });
    }
    Ok(result)
}

fn decode_warnings(value: String) -> DbResult<Vec<String>> {
    serde_json::from_str(&value)
        .map_err(|error| DbError::System(format!("decode recovery warnings: {error}")))
}

fn db_to_sqlite(error: DbError) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(error))
}
