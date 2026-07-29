use crate::connection::{DbError, DbResult};
use domain::{CaseId, DataSourceId};
use rusqlite::{params, Connection};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BitLockerRestoreIntent {
    pub data_source_id: DataSourceId,
    pub partition_index: u32,
    pub metadata_fingerprint: String,
    pub enabled: bool,
    pub last_restore_status: BitLockerRestoreStatus,
    pub last_error_code: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BitLockerRestoreStatus {
    Pending,
    Restored,
    Failed,
    Disabled,
}

impl BitLockerRestoreStatus {
    fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Restored => "restored",
            Self::Failed => "failed",
            Self::Disabled => "disabled",
        }
    }

    fn parse(value: String) -> DbResult<Self> {
        match value.as_str() {
            "pending" => Ok(Self::Pending),
            "restored" => Ok(Self::Restored),
            "failed" => Ok(Self::Failed),
            "disabled" => Ok(Self::Disabled),
            _ => Err(DbError::System(format!(
                "unknown BitLocker restore status '{value}'"
            ))),
        }
    }
}

pub struct BitLockerRestoreIntentRepo<'a> {
    conn: &'a Connection,
}

impl<'a> BitLockerRestoreIntentRepo<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    pub fn upsert_enabled(
        &self,
        data_source_id: &DataSourceId,
        partition_index: u32,
        metadata_fingerprint: &str,
    ) -> DbResult<()> {
        validate_fingerprint(metadata_fingerprint)?;
        self.conn.execute(
            "INSERT INTO bitlocker_restore_intents (
                data_source_id, partition_index, metadata_fingerprint, enabled,
                last_restore_status, last_error_code, updated_at
             ) VALUES (?1, ?2, ?3, 1, 'pending', NULL, datetime('now'))
             ON CONFLICT(data_source_id, partition_index) DO UPDATE SET
                metadata_fingerprint = excluded.metadata_fingerprint,
                enabled = 1,
                last_restore_status = 'pending',
                last_error_code = NULL,
                updated_at = datetime('now')",
            params![
                data_source_id.0,
                i64::from(partition_index),
                metadata_fingerprint
            ],
        )?;
        Ok(())
    }

    pub fn list_enabled_for_case(&self, case_id: &CaseId) -> DbResult<Vec<BitLockerRestoreIntent>> {
        let mut statement = self.conn.prepare(
            "SELECT intent.data_source_id, intent.partition_index,
                    intent.metadata_fingerprint, intent.enabled,
                    intent.last_restore_status, intent.last_error_code
             FROM bitlocker_restore_intents AS intent
             INNER JOIN data_sources AS source ON source.id = intent.data_source_id
             WHERE source.case_id = ?1 AND intent.enabled = 1
             ORDER BY source.imported_at ASC, intent.data_source_id ASC, intent.partition_index ASC",
        )?;
        let rows = statement.query_map([&case_id.0], read_intent)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    pub fn mark_status(
        &self,
        data_source_id: &DataSourceId,
        partition_index: u32,
        status: BitLockerRestoreStatus,
        error_code: Option<&str>,
    ) -> DbResult<()> {
        if let Some(error_code) = error_code {
            validate_error_code(error_code)?;
        }
        let changed = self.conn.execute(
            "UPDATE bitlocker_restore_intents
             SET last_restore_status = ?1,
                 last_error_code = ?2,
                 updated_at = datetime('now')
             WHERE data_source_id = ?3 AND partition_index = ?4",
            params![
                status.as_str(),
                error_code,
                data_source_id.0,
                i64::from(partition_index),
            ],
        )?;
        if changed != 1 {
            return Err(DbError::System(
                "BitLocker restore intent disappeared before its status was updated".to_string(),
            ));
        }
        Ok(())
    }

    pub fn remove(&self, data_source_id: &DataSourceId, partition_index: u32) -> DbResult<bool> {
        Ok(self.conn.execute(
            "DELETE FROM bitlocker_restore_intents
             WHERE data_source_id = ?1 AND partition_index = ?2",
            params![data_source_id.0, i64::from(partition_index)],
        )? > 0)
    }
}

fn read_intent(row: &rusqlite::Row<'_>) -> rusqlite::Result<BitLockerRestoreIntent> {
    let partition_index: i64 = row.get(1)?;
    let partition_index = u32::try_from(partition_index).map_err(|_| {
        rusqlite::Error::FromSqlConversionFailure(
            1,
            rusqlite::types::Type::Integer,
            "BitLocker restore partition index exceeds u32".into(),
        )
    })?;
    let status = row.get::<_, String>(4)?;
    Ok(BitLockerRestoreIntent {
        data_source_id: DataSourceId(row.get(0)?),
        partition_index,
        metadata_fingerprint: row.get(2)?,
        enabled: row.get::<_, i64>(3)? == 1,
        last_restore_status: BitLockerRestoreStatus::parse(status).map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(
                4,
                rusqlite::types::Type::Text,
                Box::new(error),
            )
        })?,
        last_error_code: row.get(5)?,
    })
}

fn validate_fingerprint(value: &str) -> DbResult<()> {
    if value.len() == 32
        && value
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    {
        return Ok(());
    }
    Err(DbError::System(
        "BitLocker metadata fingerprint must be 32 lowercase hexadecimal characters".to_string(),
    ))
}

fn validate_error_code(value: &str) -> DbResult<()> {
    if !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit() || byte == b'_')
    {
        return Ok(());
    }
    Err(DbError::System(
        "BitLocker restore error code must be an uppercase stable code".to_string(),
    ))
}
