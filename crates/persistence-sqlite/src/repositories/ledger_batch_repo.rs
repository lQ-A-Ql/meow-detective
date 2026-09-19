use super::ledger_repo::{
    row_to_entry, LedgerBatch, LedgerBatchVerification, LedgerEntry, LedgerProof, LedgerRepo,
};
use crate::connection::{DbError, DbResult};
use chrono::Utc;
use domain::{merkle_proof, merkle_root, LedgerScope};
use rusqlite::{params, Connection, OptionalExtension, Row, Transaction, TransactionBehavior};
use uuid::Uuid;

const LEDGER_BATCH_ENTRY_LIMIT: u64 = 1_000;

impl<'a> LedgerRepo<'a> {
    pub fn list_entries(
        &self,
        case_id: Option<&str>,
        limit: u32,
        offset: u64,
    ) -> DbResult<Vec<LedgerEntry>> {
        let scope_key = LedgerScope::Case.key(case_id);
        let mut statement = self.conn.prepare(
            "SELECT id, scope_key, case_id, audit_id, sequence, actor_id, action,
                    resource_type, resource_id, details, previous_hash, entry_hash, created_at
             FROM forensic_ledger WHERE scope_key = ?1 ORDER BY sequence ASC
             LIMIT ?2 OFFSET ?3",
        )?;
        let offset = i64::try_from(offset)
            .map_err(|_| DbError::System("ledger offset exceeds SQLite range".to_string()))?;
        let rows =
            statement.query_map(params![scope_key, i64::from(limit), offset], row_to_entry)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    pub fn list_batches(&self, case_id: Option<&str>) -> DbResult<Vec<LedgerBatch>> {
        ensure_batches_table(self.conn)?;
        let scope_key = LedgerScope::Case.key(case_id);
        let mut statement = self.conn.prepare(
            "SELECT id, scope_key, case_id, start_sequence, end_sequence, entry_count,
                    merkle_root, head_hash, created_at
             FROM forensic_ledger_batches WHERE scope_key = ?1 ORDER BY start_sequence ASC",
        )?;
        let rows = statement.query_map([scope_key], row_to_batch)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    pub fn seal_next_batch(&self, case_id: Option<&str>) -> DbResult<Option<LedgerBatch>> {
        ensure_batches_table(self.conn)?;
        let tx = Transaction::new_unchecked(self.conn, TransactionBehavior::Immediate)?;
        let scope_key = LedgerScope::Case.key(case_id);
        let chain = LedgerRepo::new(&tx).verify(case_id)?;
        if !chain.valid {
            return Err(DbError::System(format!(
                "cannot seal an invalid ledger: {}",
                chain
                    .first_error
                    .unwrap_or_else(|| "integrity verification failed".to_string())
            )));
        }
        let start_sequence = next_batch_start(&tx, &scope_key)?;
        let entries = load_entries_from(&tx, &scope_key, start_sequence, LEDGER_BATCH_ENTRY_LIMIT)?;
        if entries.is_empty() {
            tx.commit()?;
            return Ok(None);
        }
        validate_contiguous_entries(&entries, start_sequence)?;
        let hashes = entries
            .iter()
            .map(|entry| entry.entry_hash.clone())
            .collect::<Vec<_>>();
        let last = entries.last().expect("non-empty entries");
        let batch = LedgerBatch {
            id: Uuid::new_v4().to_string(),
            scope_key,
            case_id: case_id.map(str::to_string),
            start_sequence,
            end_sequence: last.sequence,
            entry_count: entries.len() as u64,
            merkle_root: merkle_root(&hashes).map_err(ledger_merkle_error)?,
            head_hash: last.entry_hash.clone(),
            created_at: Utc::now().to_rfc3339(),
        };
        insert_batch(&tx, &batch)?;
        tx.commit()?;
        Ok(Some(batch))
    }

    pub fn verify_batches(&self, case_id: Option<&str>) -> DbResult<LedgerBatchVerification> {
        ensure_batches_table(self.conn)?;
        let scope_key = LedgerScope::Case.key(case_id);
        let batches = self.list_batches(case_id)?;
        let mut expected_start = 1u64;
        for batch in &batches {
            if batch.start_sequence != expected_start {
                return Ok(invalid_batch_verification(
                    batches.len() as u64,
                    format!(
                        "batch starts at unexpected sequence {}",
                        batch.start_sequence
                    ),
                ));
            }
            if batch.entry_count == 0 || batch.entry_count > LEDGER_BATCH_ENTRY_LIMIT {
                return Ok(invalid_batch_verification(
                    batches.len() as u64,
                    format!("batch {} exceeds the supported entry limit", batch.id),
                ));
            }
            let entries = load_entries_range(
                self.conn,
                &scope_key,
                batch.start_sequence,
                batch.end_sequence,
            )?;
            if entries.len() as u64 != batch.entry_count {
                return Ok(invalid_batch_verification(
                    batches.len() as u64,
                    format!("batch {} entry count mismatch", batch.id),
                ));
            }
            validate_contiguous_entries(&entries, batch.start_sequence)?;
            let hashes = entries
                .iter()
                .map(|entry| entry.entry_hash.clone())
                .collect::<Vec<_>>();
            let root = merkle_root(&hashes).map_err(ledger_merkle_error)?;
            if root != batch.merkle_root {
                return Ok(invalid_batch_verification(
                    batches.len() as u64,
                    format!("batch {} merkle root mismatch", batch.id),
                ));
            }
            if entries.last().map(|entry| entry.entry_hash.as_str())
                != Some(batch.head_hash.as_str())
            {
                return Ok(invalid_batch_verification(
                    batches.len() as u64,
                    format!("batch {} head hash mismatch", batch.id),
                ));
            }
            expected_start = batch.end_sequence.saturating_add(1);
        }
        Ok(LedgerBatchVerification {
            valid: true,
            batch_count: batches.len() as u64,
            first_error: None,
        })
    }

    pub fn proof(
        &self,
        case_id: Option<&str>,
        batch_id: &str,
        sequence: u64,
    ) -> DbResult<Option<LedgerProof>> {
        ensure_batches_table(self.conn)?;
        let scope_key = LedgerScope::Case.key(case_id);
        let batch = self
            .conn
            .query_row(
                "SELECT id, scope_key, case_id, start_sequence, end_sequence, entry_count,
                        merkle_root, head_hash, created_at
                 FROM forensic_ledger_batches WHERE id = ?1 AND scope_key = ?2",
                params![batch_id, scope_key],
                row_to_batch,
            )
            .optional()?;
        let Some(batch) = batch else {
            return Ok(None);
        };
        let chain = self.verify(case_id)?;
        if !chain.valid {
            return Err(DbError::System(format!(
                "cannot generate a proof for an invalid ledger: {}",
                chain
                    .first_error
                    .unwrap_or_else(|| "integrity verification failed".to_string())
            )));
        }
        let batches = self.verify_batches(case_id)?;
        if !batches.valid {
            return Err(DbError::System(format!(
                "cannot generate a proof for invalid batches: {}",
                batches
                    .first_error
                    .unwrap_or_else(|| "batch verification failed".to_string())
            )));
        }
        if batch.entry_count > LEDGER_BATCH_ENTRY_LIMIT {
            return Err(DbError::System(
                "ledger batch exceeds the supported entry limit".to_string(),
            ));
        }
        if sequence < batch.start_sequence || sequence > batch.end_sequence {
            return Ok(None);
        }
        let entries = load_entries_range(
            self.conn,
            &batch.scope_key,
            batch.start_sequence,
            batch.end_sequence,
        )?;
        validate_contiguous_entries(&entries, batch.start_sequence)?;
        let index = usize::try_from(sequence - batch.start_sequence)
            .map_err(|_| DbError::System("ledger proof index overflow".to_string()))?;
        let hashes = entries
            .iter()
            .map(|entry| entry.entry_hash.clone())
            .collect::<Vec<_>>();
        let proof = merkle_proof(&hashes, index).map_err(ledger_merkle_error)?;
        Ok(Some(LedgerProof {
            batch,
            sequence,
            entry_hash: entries[index].entry_hash.clone(),
            proof,
        }))
    }
}

fn row_to_batch(row: &Row<'_>) -> rusqlite::Result<LedgerBatch> {
    let start_sequence = row.get::<_, i64>(3)?;
    let end_sequence = row.get::<_, i64>(4)?;
    let entry_count = row.get::<_, i64>(5)?;
    Ok(LedgerBatch {
        id: row.get(0)?,
        scope_key: row.get(1)?,
        case_id: row.get(2)?,
        start_sequence: u64::try_from(start_sequence)
            .map_err(|_| rusqlite::Error::IntegralValueOutOfRange(3, start_sequence))?,
        end_sequence: u64::try_from(end_sequence)
            .map_err(|_| rusqlite::Error::IntegralValueOutOfRange(4, end_sequence))?,
        entry_count: u64::try_from(entry_count)
            .map_err(|_| rusqlite::Error::IntegralValueOutOfRange(5, entry_count))?,
        merkle_root: row.get(6)?,
        head_hash: row.get(7)?,
        created_at: row.get(8)?,
    })
}

fn load_entries_from(
    conn: &Connection,
    scope_key: &str,
    start: u64,
    limit: u64,
) -> DbResult<Vec<LedgerEntry>> {
    let mut statement = conn.prepare(
        "SELECT id, scope_key, case_id, audit_id, sequence, actor_id, action,
                resource_type, resource_id, details, previous_hash, entry_hash, created_at
         FROM forensic_ledger WHERE scope_key = ?1 AND sequence >= ?2 ORDER BY sequence ASC
         LIMIT ?3",
    )?;
    let rows = statement.query_map(
        params![
            scope_key,
            i64::try_from(start).map_err(|_| {
                DbError::System("ledger sequence exceeds SQLite range".to_string())
            })?,
            i64::try_from(limit).map_err(|_| DbError::System(
                "ledger batch limit exceeds SQLite range".to_string()
            ))?
        ],
        row_to_entry,
    )?;
    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

fn load_entries_range(
    conn: &Connection,
    scope_key: &str,
    start: u64,
    end: u64,
) -> DbResult<Vec<LedgerEntry>> {
    let mut statement = conn.prepare(
        "SELECT id, scope_key, case_id, audit_id, sequence, actor_id, action,
                resource_type, resource_id, details, previous_hash, entry_hash, created_at
         FROM forensic_ledger WHERE scope_key = ?1 AND sequence BETWEEN ?2 AND ?3 ORDER BY sequence ASC",
    )?;
    let start = i64::try_from(start)
        .map_err(|_| DbError::System("ledger sequence exceeds SQLite range".to_string()))?;
    let end = i64::try_from(end)
        .map_err(|_| DbError::System("ledger sequence exceeds SQLite range".to_string()))?;
    let rows = statement.query_map(params![scope_key, start, end], row_to_entry)?;
    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

fn next_batch_start(conn: &Connection, scope_key: &str) -> DbResult<u64> {
    let end = conn
        .query_row(
            "SELECT end_sequence FROM forensic_ledger_batches WHERE scope_key = ?1
             ORDER BY end_sequence DESC LIMIT 1",
            [scope_key],
            |row| row.get::<_, i64>(0),
        )
        .optional()?;
    match end {
        Some(end) => u64::try_from(end)
            .map_err(|_| DbError::System("ledger batch sequence is negative".to_string()))?
            .checked_add(1)
            .ok_or_else(|| DbError::System("ledger batch sequence overflow".to_string())),
        None => Ok(1),
    }
}

fn validate_contiguous_entries(entries: &[LedgerEntry], expected_start: u64) -> DbResult<()> {
    for (index, entry) in entries.iter().enumerate() {
        let expected = expected_start
            .checked_add(index as u64)
            .ok_or_else(|| DbError::System("ledger sequence overflow".to_string()))?;
        if entry.sequence != expected {
            return Err(DbError::System(format!(
                "ledger sequence discontinuity at {}",
                entry.sequence
            )));
        }
    }
    Ok(())
}

fn insert_batch(conn: &Connection, batch: &LedgerBatch) -> DbResult<()> {
    conn.execute(
        "INSERT INTO forensic_ledger_batches (
            id, scope_key, case_id, start_sequence, end_sequence, entry_count,
            merkle_root, head_hash, created_at
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![
            batch.id,
            batch.scope_key,
            batch.case_id,
            i64::try_from(batch.start_sequence)
                .map_err(|_| DbError::System("ledger sequence exceeds SQLite range".to_string()))?,
            i64::try_from(batch.end_sequence)
                .map_err(|_| DbError::System("ledger sequence exceeds SQLite range".to_string()))?,
            i64::try_from(batch.entry_count).map_err(|_| DbError::System(
                "ledger entry count exceeds SQLite range".to_string()
            ))?,
            batch.merkle_root,
            batch.head_hash,
            batch.created_at,
        ],
    )?;
    Ok(())
}

fn ensure_batches_table(conn: &Connection) -> DbResult<()> {
    if batches_table_exists(conn)? {
        Ok(())
    } else {
        Err(DbError::System(
            "forensic ledger batch migration is not applied".to_string(),
        ))
    }
}

fn batches_table_exists(conn: &Connection) -> DbResult<bool> {
    Ok(conn.query_row(
        "SELECT COUNT(*) > 0 FROM sqlite_master WHERE type = 'table' AND name = 'forensic_ledger_batches'",
        [],
        |row| row.get(0),
    )?)
}

fn invalid_batch_verification(batch_count: u64, first_error: String) -> LedgerBatchVerification {
    LedgerBatchVerification {
        valid: false,
        batch_count,
        first_error: Some(first_error),
    }
}

fn ledger_merkle_error(error: domain::MerkleError) -> DbError {
    DbError::System(format!("ledger merkle error: {error}"))
}
