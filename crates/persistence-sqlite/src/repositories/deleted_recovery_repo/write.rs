use rusqlite::{params, Connection};

use crate::connection::DbResult;

use super::validation::{sqlite_u64, validate_aggregate};
use super::{DeletedRecoveryAggregate, DeletedRecoveryRepo};

impl DeletedRecoveryRepo<'_> {
    pub fn replace_scan(&self, aggregate: &DeletedRecoveryAggregate) -> DbResult<()> {
        validate_aggregate(aggregate)?;
        let transaction = self.conn.unchecked_transaction()?;
        transaction.execute(
            "DELETE FROM filesystem_recovery_scans
             WHERE data_source_id = ?1 AND partition_index = ?2",
            params![
                aggregate.scan.data_source_id,
                aggregate.scan.partition_index,
            ],
        )?;
        insert_scan(&transaction, aggregate)?;
        insert_recoveries(&transaction, aggregate)?;
        insert_issues(&transaction, aggregate)?;
        transaction.commit()?;
        Ok(())
    }
}

fn insert_scan(conn: &Connection, aggregate: &DeletedRecoveryAggregate) -> DbResult<()> {
    let scan = &aggregate.scan;
    conn.execute(
        "INSERT INTO filesystem_recovery_scans (
            id, data_source_id, partition_index, filesystem_type, filesystem_uuid,
            parser_version, log_kind, snapshot_identity_sha256, state,
            transaction_count, candidate_count, warnings_json, started_at, completed_at
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
        params![
            scan.id,
            scan.data_source_id,
            scan.partition_index,
            scan.filesystem_type,
            scan.filesystem_uuid,
            scan.parser_version,
            scan.log_kind,
            scan.snapshot_identity_sha256,
            scan.state,
            sqlite_u64("transaction count", scan.transaction_count)?,
            sqlite_u64("candidate count", scan.candidate_count)?,
            serde_json::to_string(&scan.warnings).map_err(|error| {
                crate::connection::DbError::System(format!("encode recovery warnings: {error}"))
            })?,
            scan.started_at,
            scan.completed_at,
        ],
    )?;
    Ok(())
}

fn insert_recoveries(conn: &Connection, aggregate: &DeletedRecoveryAggregate) -> DbResult<()> {
    let mut recovery_statement = conn.prepare_cached(
        "INSERT INTO deleted_file_recoveries (
            id, scan_id, inode, original_path, entry_type, mode, mft_sequence, deleted_at_unix,
            declared_size, recoverable_bytes,
            completeness, recovery_method, confidence, allocation_state,
            transaction_id, log_sequence, log_cycle, content_md5, content_sha1,
            content_sha256, warnings_json
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21)",
    )?;
    let mut range_statement = conn.prepare_cached(
        "INSERT INTO deleted_file_recovery_ranges (
            recovery_id, ordinal, range_role, source_kind, logical_offset,
            source_offset, physical_offset, length, allocation_state, sha256
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
    )?;
    for recovery in &aggregate.recoveries {
        recovery_statement.execute(params![
            recovery.id,
            aggregate.scan.id,
            recovery.inode,
            recovery.original_path,
            recovery.entry_type,
            recovery.mode,
            recovery.mft_sequence,
            recovery
                .deleted_at_unix
                .map(|value| sqlite_u64("deletion timestamp", value))
                .transpose()?,
            sqlite_u64("declared size", recovery.declared_size)?,
            sqlite_u64("recoverable bytes", recovery.recoverable_bytes)?,
            recovery.completeness,
            recovery.recovery_method,
            recovery.confidence,
            recovery.allocation_state,
            recovery.transaction_id,
            recovery
                .log_sequence
                .map(|value| sqlite_u64("log sequence", value))
                .transpose()?,
            recovery
                .log_cycle
                .map(|value| sqlite_u64("log cycle", value))
                .transpose()?,
            recovery.content_md5,
            recovery.content_sha1,
            recovery.content_sha256,
            serde_json::to_string(&recovery.warnings).map_err(|error| {
                crate::connection::DbError::System(format!(
                    "encode recovery candidate warnings: {error}"
                ))
            })?,
        ])?;
        for range in &recovery.ranges {
            range_statement.execute(params![
                recovery.id,
                range.ordinal,
                range.range_role,
                range.source_kind,
                sqlite_u64("logical offset", range.logical_offset)?,
                sqlite_u64("source offset", range.source_offset)?,
                range
                    .physical_offset
                    .map(|value| sqlite_u64("physical offset", value))
                    .transpose()?,
                sqlite_u64("range length", range.length)?,
                range.allocation_state,
                range.sha256,
            ])?;
        }
    }
    Ok(())
}

fn insert_issues(conn: &Connection, aggregate: &DeletedRecoveryAggregate) -> DbResult<()> {
    let mut statement = conn.prepare_cached(
        "INSERT INTO filesystem_recovery_issues (
            scan_id, ordinal, severity, code, message, log_offset, sequence
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
    )?;
    for issue in &aggregate.issues {
        statement.execute(params![
            aggregate.scan.id,
            issue.ordinal,
            issue.severity,
            issue.code,
            issue.message,
            issue
                .log_offset
                .map(|value| sqlite_u64("issue log offset", value))
                .transpose()?,
            issue
                .sequence
                .map(|value| sqlite_u64("issue sequence", value))
                .transpose()?,
        ])?;
    }
    Ok(())
}
