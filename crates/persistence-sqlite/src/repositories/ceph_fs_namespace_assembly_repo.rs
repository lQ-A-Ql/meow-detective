use rusqlite::{params, Connection, OptionalExtension};

use crate::connection::{DbError, DbResult};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CephFsNamespaceAssemblyRecord {
    pub filesystem_identity: String,
    pub data_source_id: String,
    pub assembly_sha256: String,
    pub assembly_version: u32,
    pub complete: bool,
    pub frozen: bool,
    pub freeze_reasons_json: String,
    pub mutation_state: String,
    pub mutation_digest: Option<String>,
}

pub struct CephFsNamespaceAssemblyRepo<'a> {
    conn: &'a Connection,
}

impl<'a> CephFsNamespaceAssemblyRepo<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    pub fn replace(&self, record: &CephFsNamespaceAssemblyRecord) -> DbResult<()> {
        validate(record)?;
        self.conn.execute(
            "INSERT INTO ceph_fs_namespace_assemblies (
                filesystem_identity, data_source_id, assembly_sha256,
                assembly_version, complete, frozen, freeze_reasons_json,
                mutation_state, mutation_digest
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
             ON CONFLICT(filesystem_identity, data_source_id) DO UPDATE SET
                assembly_sha256 = excluded.assembly_sha256,
                assembly_version = excluded.assembly_version,
                complete = excluded.complete,
                frozen = excluded.frozen,
                freeze_reasons_json = excluded.freeze_reasons_json,
                mutation_state = excluded.mutation_state,
                mutation_digest = excluded.mutation_digest,
                created_at = datetime('now')",
            params![
                record.filesystem_identity,
                record.data_source_id,
                record.assembly_sha256,
                record.assembly_version,
                i64::from(record.complete),
                i64::from(record.frozen),
                record.freeze_reasons_json,
                record.mutation_state,
                record.mutation_digest,
            ],
        )?;
        Ok(())
    }

    pub fn find(
        &self,
        filesystem_identity: &str,
        data_source_id: &str,
    ) -> DbResult<Option<CephFsNamespaceAssemblyRecord>> {
        self.conn
            .query_row(
                "SELECT filesystem_identity, data_source_id, assembly_sha256,
                        assembly_version, complete, frozen, freeze_reasons_json,
                        mutation_state, mutation_digest
                 FROM ceph_fs_namespace_assemblies
                 WHERE filesystem_identity = ?1 AND data_source_id = ?2",
                params![filesystem_identity, data_source_id],
                map_record,
            )
            .optional()
            .map_err(Into::into)
    }

    pub fn verify(
        &self,
        expected: &CephFsNamespaceAssemblyRecord,
    ) -> DbResult<CephFsNamespaceAssemblyRecord> {
        let actual = self
            .find(&expected.filesystem_identity, &expected.data_source_id)?
            .ok_or_else(|| DbError::System("CephFS namespace assembly is missing".to_string()))?;
        validate(&actual)?;
        if &actual != expected {
            return Err(DbError::System(
                "CephFS namespace assembly does not match the expected evidence".to_string(),
            ));
        }
        Ok(actual)
    }
}

fn map_record(row: &rusqlite::Row<'_>) -> rusqlite::Result<CephFsNamespaceAssemblyRecord> {
    Ok(CephFsNamespaceAssemblyRecord {
        filesystem_identity: row.get(0)?,
        data_source_id: row.get(1)?,
        assembly_sha256: row.get(2)?,
        assembly_version: decode_u32(row.get(3)?, 3)?,
        complete: decode_bool(row.get(4)?, 4)?,
        frozen: decode_bool(row.get(5)?, 5)?,
        freeze_reasons_json: row.get(6)?,
        mutation_state: row.get(7)?,
        mutation_digest: row.get(8)?,
    })
}

fn validate(record: &CephFsNamespaceAssemblyRecord) -> DbResult<()> {
    if record.filesystem_identity.trim().is_empty()
        || record.data_source_id.trim().is_empty()
        || !is_sha256(&record.assembly_sha256)
        || record.assembly_version == 0
        || record.frozen == record.complete
        || (record.complete && record.freeze_reasons_json != "[]")
        || (!record.complete && record.freeze_reasons_json == "[]")
        || !matches!(record.mutation_state.as_str(), "complete" | "unknown")
    {
        return Err(DbError::System(
            "CephFS namespace assembly record is invalid".to_string(),
        ));
    }
    if let Some(digest) = record.mutation_digest.as_deref() {
        if !is_sha256(digest) || record.mutation_state != "unknown" {
            return Err(DbError::System(
                "CephFS namespace mutation proof is invalid".to_string(),
            ));
        }
    } else if record.mutation_state == "unknown" {
        return Err(DbError::System(
            "unknown CephFS namespace mutation has no digest".to_string(),
        ));
    }
    serde_json::from_str::<Vec<String>>(&record.freeze_reasons_json).map_err(|_| {
        DbError::System("CephFS namespace freeze reasons are not valid JSON".to_string())
    })?;
    Ok(())
}

fn is_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn decode_bool(value: i64, index: usize) -> rusqlite::Result<bool> {
    match value {
        0 => Ok(false),
        1 => Ok(true),
        _ => Err(rusqlite::Error::FromSqlConversionFailure(
            index,
            rusqlite::types::Type::Integer,
            "boolean is not 0 or 1".into(),
        )),
    }
}

fn decode_u32(value: i64, index: usize) -> rusqlite::Result<u32> {
    u32::try_from(value).map_err(|_| {
        rusqlite::Error::FromSqlConversionFailure(
            index,
            rusqlite::types::Type::Integer,
            "integer is outside the u32 range".into(),
        )
    })
}
