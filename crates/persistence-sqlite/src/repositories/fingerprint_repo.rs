use crate::connection::DbResult;
use domain::{normalize_content_sha256, ForensicFingerprint, ForensicObjectType};
use rusqlite::{params, Connection, Error as SqliteError, ErrorCode, Row};

pub struct ForensicFingerprintRepo<'a> {
    conn: &'a Connection,
}

impl<'a> ForensicFingerprintRepo<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    pub fn upsert(&self, fingerprint: &ForensicFingerprint) -> DbResult<()> {
        upsert_record(self.conn, fingerprint)
    }

    pub fn upsert_in_transaction(&self, fingerprint: &ForensicFingerprint) -> DbResult<()> {
        upsert_record(self.conn, fingerprint)
    }

    pub fn is_available(&self) -> DbResult<bool> {
        table_exists(self.conn)
    }

    /// Older staging/fixture databases can legitimately predate the FMD
    /// migration. Their primary record must remain writable; the FMD write is
    /// therefore skipped until the owning migration has been applied.
    pub fn upsert_if_available(&self, fingerprint: &ForensicFingerprint) -> DbResult<()> {
        if !table_exists(self.conn)? {
            return Ok(());
        }
        self.upsert(fingerprint)
    }

    pub fn upsert_batch(&self, fingerprints: &[ForensicFingerprint]) -> DbResult<()> {
        if fingerprints.is_empty() {
            return Ok(());
        }
        let tx = self.conn.unchecked_transaction()?;
        for fingerprint in fingerprints {
            upsert_record(&tx, fingerprint)?;
        }
        tx.commit()?;
        Ok(())
    }

    pub fn find(
        &self,
        object_type: ForensicObjectType,
        source_id: Option<&str>,
        object_id: &str,
    ) -> DbResult<Option<ForensicFingerprint>> {
        let key = object_key_parts(object_type, source_id, object_id);
        self.conn
            .query_row(
                "SELECT object_id, case_id, source_id, object_type, parent_object_id,
                        source_locator, byte_length, content_sha256, metadata_sha256,
                        parser_id, parser_version, created_at
                 FROM forensic_fingerprints WHERE object_key = ?1",
                [key],
                row_to_fingerprint,
            )
            .optional()
            .map_err(Into::into)
    }

    pub fn list_by_source(&self, source_id: &str) -> DbResult<Vec<ForensicFingerprint>> {
        let mut statement = self.conn.prepare(
            "SELECT object_id, case_id, source_id, object_type, parent_object_id,
                    source_locator, byte_length, content_sha256, metadata_sha256,
                    parser_id, parser_version, created_at
             FROM forensic_fingerprints
             WHERE source_id = ?1
             ORDER BY object_type ASC, object_id ASC",
        )?;
        let rows = statement.query_map([source_id], row_to_fingerprint)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    /// Update the content digest of an already registered object without
    /// rebuilding its metadata digest. Hash jobs complete after the source
    /// row is inserted, so this keeps the FMD record current for that path.
    pub fn update_content_sha256(
        &self,
        object_type: ForensicObjectType,
        source_id: Option<&str>,
        object_id: &str,
        content_sha256: Option<&str>,
    ) -> DbResult<()> {
        if !self.is_available()? {
            return Ok(());
        }
        self.conn.execute(
            "UPDATE forensic_fingerprints
             SET content_sha256 = ?1
             WHERE object_key = ?2",
            params![
                normalize_content_sha256(content_sha256),
                object_key_parts(object_type, source_id, object_id),
            ],
        )?;
        Ok(())
    }
}

fn upsert_record(conn: &Connection, fingerprint: &ForensicFingerprint) -> DbResult<()> {
    conn.execute(
        "INSERT INTO forensic_fingerprints (
                object_key, object_id, case_id, source_id, object_type,
                parent_object_id, source_locator, byte_length, content_sha256,
                metadata_sha256, parser_id, parser_version, created_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
            ON CONFLICT(object_key) DO UPDATE SET
                object_id = excluded.object_id,
                case_id = excluded.case_id,
                source_id = excluded.source_id,
                object_type = excluded.object_type,
                parent_object_id = excluded.parent_object_id,
                source_locator = excluded.source_locator,
                byte_length = excluded.byte_length,
                content_sha256 = excluded.content_sha256,
                metadata_sha256 = excluded.metadata_sha256,
                parser_id = excluded.parser_id,
                parser_version = excluded.parser_version",
        params![
            object_key(fingerprint),
            fingerprint.object_id,
            fingerprint.case_id,
            fingerprint.source_id,
            fingerprint.object_type.as_str(),
            fingerprint.parent_object_id,
            fingerprint.source_locator,
            fingerprint.byte_length.map(|value| value as i64),
            fingerprint.content_sha256,
            fingerprint.metadata_sha256,
            fingerprint.parser_id,
            fingerprint.parser_version,
            fingerprint.created_at.to_rfc3339(),
        ],
    )?;
    Ok(())
}

fn object_key(fingerprint: &ForensicFingerprint) -> String {
    object_key_parts(
        fingerprint.object_type,
        fingerprint.source_id.as_deref(),
        &fingerprint.object_id,
    )
}

fn table_exists(conn: &Connection) -> DbResult<bool> {
    Ok(conn.query_row(
        "SELECT COUNT(*) > 0 FROM sqlite_master WHERE type = 'table' AND name = 'forensic_fingerprints'",
        [],
        |row| row.get(0),
    )?)
}

fn object_key_parts(
    object_type: ForensicObjectType,
    source_id: Option<&str>,
    object_id: &str,
) -> String {
    format!(
        "{}:{}:{}",
        object_type.as_str(),
        source_id.unwrap_or("-"),
        object_id
    )
}

fn row_to_fingerprint(row: &Row<'_>) -> rusqlite::Result<ForensicFingerprint> {
    let object_type = row.get::<_, String>(3)?;
    let object_type = ForensicObjectType::parse(&object_type).ok_or_else(|| {
        SqliteError::SqliteFailure(
            rusqlite::ffi::Error {
                code: ErrorCode::ConstraintViolation,
                extended_code: 0,
            },
            Some("unknown forensic object type".to_string()),
        )
    })?;
    Ok(ForensicFingerprint {
        object_id: row.get(0)?,
        case_id: row.get(1)?,
        source_id: row.get(2)?,
        object_type,
        parent_object_id: row.get(4)?,
        source_locator: row.get(5)?,
        byte_length: row
            .get::<_, Option<i64>>(6)?
            .and_then(|value| u64::try_from(value).ok()),
        content_sha256: row.get(7)?,
        metadata_sha256: row.get(8)?,
        parser_id: row.get(9)?,
        parser_version: row.get(10)?,
        created_at: crate::util::parse_datetime(&row.get::<_, String>(11)?),
    })
}

use rusqlite::OptionalExtension;

#[cfg(test)]
#[path = "../../tests/unit/repositories/fingerprint_repo.rs"]
mod tests;
