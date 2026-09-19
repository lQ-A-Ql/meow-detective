use crate::connection::{DbError, DbResult};
use chrono::Utc;
use domain::{ForensicLedgerEvent, LedgerScope, LEDGER_GENESIS_HASH};
use rusqlite::{params, Connection, OptionalExtension, Row, Transaction, TransactionBehavior};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LedgerEntry {
    pub id: String,
    pub scope_key: String,
    pub case_id: Option<String>,
    pub audit_id: String,
    pub sequence: u64,
    pub actor_id: String,
    pub action: String,
    pub resource_type: String,
    pub resource_id: Option<String>,
    pub details: String,
    pub previous_hash: String,
    pub entry_hash: String,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LedgerVerification {
    pub valid: bool,
    pub entry_count: u64,
    pub head_hash: Option<String>,
    pub first_error: Option<String>,
}

pub struct LedgerEventInput<'a> {
    pub case_id: Option<&'a str>,
    pub audit_id: &'a str,
    pub actor_id: &'a str,
    pub action: &'a str,
    pub resource_type: &'a str,
    pub resource_id: Option<&'a str>,
    pub details: &'a str,
}

pub struct LedgerRepo<'a> {
    conn: &'a Connection,
}

impl<'a> LedgerRepo<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    pub fn is_available(&self) -> DbResult<bool> {
        table_exists(self.conn)
    }

    pub fn append_audit_event(&self, input: LedgerEventInput<'_>) -> DbResult<Option<LedgerEntry>> {
        if !self.is_available()? {
            return Ok(None);
        }
        let tx = Transaction::new_unchecked(self.conn, TransactionBehavior::Immediate)?;
        let entry = append_in_transaction(&tx, input)?;
        tx.commit()?;
        Ok(Some(entry))
    }

    pub fn verify(&self, case_id: Option<&str>) -> DbResult<LedgerVerification> {
        if !self.is_available()? {
            return Ok(LedgerVerification {
                valid: false,
                entry_count: 0,
                head_hash: None,
                first_error: Some("forensic ledger migration is not applied".to_string()),
            });
        }
        let scope_key = LedgerScope::Case.key(case_id);
        let mut statement = self.conn.prepare(
            "SELECT id, scope_key, case_id, audit_id, sequence, actor_id, action,
                    resource_type, resource_id, details, previous_hash, entry_hash, created_at
             FROM forensic_ledger WHERE scope_key = ?1 ORDER BY sequence ASC",
        )?;
        let rows = statement.query_map([scope_key], row_to_entry)?;
        let mut previous_hash = LEDGER_GENESIS_HASH.to_string();
        let mut expected_sequence = 1u64;
        let mut entry_count = 0u64;
        let mut head_hash = None;
        for row in rows {
            let entry = row?;
            let event = event_from_entry(&entry);
            if entry.sequence != expected_sequence {
                return Ok(invalid_verification(
                    entry_count,
                    head_hash,
                    format!("sequence discontinuity at {}", entry.sequence),
                ));
            }
            if entry.previous_hash != previous_hash {
                return Ok(invalid_verification(
                    entry_count,
                    head_hash,
                    format!("previous hash mismatch at sequence {}", entry.sequence),
                ));
            }
            let calculated = event.entry_hash();
            if entry.entry_hash != calculated {
                return Ok(invalid_verification(
                    entry_count,
                    head_hash,
                    format!("entry hash mismatch at sequence {}", entry.sequence),
                ));
            }
            previous_hash = entry.entry_hash.clone();
            head_hash = Some(previous_hash.clone());
            entry_count += 1;
            expected_sequence = expected_sequence.saturating_add(1);
        }
        Ok(LedgerVerification {
            valid: true,
            entry_count,
            head_hash,
            first_error: None,
        })
    }
}

pub fn append_in_transaction(
    tx: &Transaction<'_>,
    input: LedgerEventInput<'_>,
) -> DbResult<LedgerEntry> {
    let LedgerEventInput {
        case_id,
        audit_id,
        actor_id,
        action,
        resource_type,
        resource_id,
        details,
    } = input;
    let scope_key = LedgerScope::Case.key(case_id);
    let previous = tx
        .query_row(
            "SELECT sequence, entry_hash FROM forensic_ledger
             WHERE scope_key = ?1 ORDER BY sequence DESC LIMIT 1",
            [&scope_key],
            |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?)),
        )
        .optional()?;
    let (sequence, previous_hash) = match previous {
        Some((sequence, hash)) => (
            u64::try_from(sequence)
                .map_err(|_| DbError::System("ledger sequence is negative".to_string()))?
                .checked_add(1)
                .ok_or_else(|| DbError::System("ledger sequence overflow".to_string()))?,
            hash,
        ),
        None => (1, LEDGER_GENESIS_HASH.to_string()),
    };
    let event = ForensicLedgerEvent {
        scope_key: scope_key.clone(),
        case_id: case_id.map(str::to_string),
        audit_id: audit_id.to_string(),
        sequence,
        actor_id: actor_id.to_string(),
        action: action.to_string(),
        resource_type: resource_type.to_string(),
        resource_id: resource_id.map(str::to_string),
        details: details.to_string(),
        previous_hash: previous_hash.clone(),
        created_at: Utc::now().to_rfc3339(),
    };
    let entry = LedgerEntry {
        id: Uuid::new_v4().to_string(),
        scope_key,
        case_id: event.case_id.clone(),
        audit_id: event.audit_id.clone(),
        sequence,
        actor_id: event.actor_id.clone(),
        action: event.action.clone(),
        resource_type: event.resource_type.clone(),
        resource_id: event.resource_id.clone(),
        details: event.details.clone(),
        previous_hash,
        entry_hash: event.entry_hash(),
        created_at: event.created_at.clone(),
    };
    let sequence = i64::try_from(entry.sequence)
        .map_err(|_| DbError::System("ledger sequence exceeds SQLite range".to_string()))?;
    tx.execute(
        "INSERT INTO forensic_ledger (
            id, scope_key, case_id, audit_id, sequence, actor_id, action,
            resource_type, resource_id, details, previous_hash, entry_hash, created_at
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
        params![
            entry.id,
            entry.scope_key,
            entry.case_id,
            entry.audit_id,
            sequence,
            entry.actor_id,
            entry.action,
            entry.resource_type,
            entry.resource_id,
            entry.details,
            entry.previous_hash,
            entry.entry_hash,
            entry.created_at,
        ],
    )?;
    Ok(entry)
}

fn event_from_entry(entry: &LedgerEntry) -> ForensicLedgerEvent {
    ForensicLedgerEvent {
        scope_key: entry.scope_key.clone(),
        case_id: entry.case_id.clone(),
        audit_id: entry.audit_id.clone(),
        sequence: entry.sequence,
        actor_id: entry.actor_id.clone(),
        action: entry.action.clone(),
        resource_type: entry.resource_type.clone(),
        resource_id: entry.resource_id.clone(),
        details: entry.details.clone(),
        previous_hash: entry.previous_hash.clone(),
        created_at: entry.created_at.clone(),
    }
}

fn invalid_verification(
    entry_count: u64,
    head_hash: Option<String>,
    first_error: String,
) -> LedgerVerification {
    LedgerVerification {
        valid: false,
        entry_count,
        head_hash,
        first_error: Some(first_error),
    }
}

fn row_to_entry(row: &Row<'_>) -> rusqlite::Result<LedgerEntry> {
    let sequence = row.get::<_, i64>(4)?;
    let sequence = u64::try_from(sequence)
        .map_err(|_| rusqlite::Error::IntegralValueOutOfRange(4, sequence))?;
    Ok(LedgerEntry {
        id: row.get(0)?,
        scope_key: row.get(1)?,
        case_id: row.get(2)?,
        audit_id: row.get(3)?,
        sequence,
        actor_id: row.get(5)?,
        action: row.get(6)?,
        resource_type: row.get(7)?,
        resource_id: row.get(8)?,
        details: row.get(9)?,
        previous_hash: row.get(10)?,
        entry_hash: row.get(11)?,
        created_at: row.get(12)?,
    })
}

fn table_exists(conn: &Connection) -> DbResult<bool> {
    Ok(conn.query_row(
        "SELECT COUNT(*) > 0 FROM sqlite_master WHERE type = 'table' AND name = 'forensic_ledger'",
        [],
        |row| row.get(0),
    )?)
}

#[cfg(test)]
#[path = "../../tests/unit/repositories/ledger_repo.rs"]
mod tests;
