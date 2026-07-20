use rusqlite::{params, Connection, OptionalExtension};

use crate::connection::{DbError, DbResult};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum CephFsSourceCapability {
    MetadataOnly,
    MetadataBrowseable,
    BoundedPreview,
}

impl CephFsSourceCapability {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::MetadataOnly => "metadata-only",
            Self::MetadataBrowseable => "metadata-browseable",
            Self::BoundedPreview => "bounded-preview",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "metadata-only" => Some(Self::MetadataOnly),
            "metadata-browseable" => Some(Self::MetadataBrowseable),
            "bounded-preview" => Some(Self::BoundedPreview),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CephFsSourceCapabilityRecord {
    pub filesystem_identity: String,
    pub data_source_id: String,
    pub capability: CephFsSourceCapability,
    pub lineage_fingerprint: String,
    pub assembly_sha256: String,
    pub namespace_projection_sha256: String,
    pub schema_version: u32,
    pub decoder_profile: String,
}

pub struct CephFsSourceCapabilityRepo<'a> {
    conn: &'a Connection,
}

impl<'a> CephFsSourceCapabilityRepo<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    pub fn replace(&self, record: &CephFsSourceCapabilityRecord) -> DbResult<()> {
        validate(record)?;
        self.conn.execute(
            "INSERT INTO ceph_fs_source_capabilities (
                filesystem_identity, data_source_id, capability,
                lineage_fingerprint, assembly_sha256, namespace_projection_sha256,
                schema_version, decoder_profile
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
             ON CONFLICT(filesystem_identity, data_source_id) DO UPDATE SET
                capability = excluded.capability,
                lineage_fingerprint = excluded.lineage_fingerprint,
                assembly_sha256 = excluded.assembly_sha256,
                namespace_projection_sha256 = excluded.namespace_projection_sha256,
                schema_version = excluded.schema_version,
                decoder_profile = excluded.decoder_profile,
                created_at = datetime('now')",
            params![
                record.filesystem_identity,
                record.data_source_id,
                record.capability.as_str(),
                record.lineage_fingerprint,
                record.assembly_sha256,
                record.namespace_projection_sha256,
                record.schema_version,
                record.decoder_profile,
            ],
        )?;
        Ok(())
    }

    pub fn find(
        &self,
        filesystem_identity: &str,
        data_source_id: &str,
    ) -> DbResult<Option<CephFsSourceCapabilityRecord>> {
        self.conn
            .query_row(
                "SELECT filesystem_identity, data_source_id, capability,
                        lineage_fingerprint, assembly_sha256,
                        namespace_projection_sha256, schema_version, decoder_profile
                 FROM ceph_fs_source_capabilities
                 WHERE filesystem_identity = ?1 AND data_source_id = ?2",
                params![filesystem_identity, data_source_id],
                map_record,
            )
            .optional()
            .map_err(Into::into)
    }

    pub fn verify(
        &self,
        expected: &CephFsSourceCapabilityRecord,
    ) -> DbResult<CephFsSourceCapabilityRecord> {
        let actual = self
            .find(&expected.filesystem_identity, &expected.data_source_id)?
            .ok_or_else(|| DbError::System("CephFS source capability is missing".to_string()))?;
        validate(&actual)?;
        if &actual != expected {
            return Err(DbError::System(
                "CephFS source capability does not match the expected evidence".to_string(),
            ));
        }
        Ok(actual)
    }
}

fn map_record(row: &rusqlite::Row<'_>) -> rusqlite::Result<CephFsSourceCapabilityRecord> {
    let capability =
        CephFsSourceCapability::parse(row.get::<_, String>(2)?.as_str()).ok_or_else(|| {
            rusqlite::Error::FromSqlConversionFailure(
                2,
                rusqlite::types::Type::Text,
                "unknown CephFS source capability".into(),
            )
        })?;
    Ok(CephFsSourceCapabilityRecord {
        filesystem_identity: row.get(0)?,
        data_source_id: row.get(1)?,
        capability,
        lineage_fingerprint: row.get(3)?,
        assembly_sha256: row.get(4)?,
        namespace_projection_sha256: row.get(5)?,
        schema_version: decode_u32(row.get(6)?, 6)?,
        decoder_profile: row.get(7)?,
    })
}

fn validate(record: &CephFsSourceCapabilityRecord) -> DbResult<()> {
    if record.filesystem_identity.trim().is_empty()
        || record.data_source_id.trim().is_empty()
        || !is_sha256(&record.lineage_fingerprint)
        || !is_sha256(&record.assembly_sha256)
        || !is_sha256(&record.namespace_projection_sha256)
        || record.schema_version != 1
        || record.decoder_profile != "cephfs-namespace-v1"
    {
        return Err(DbError::System(
            "CephFS source capability record is invalid".to_string(),
        ));
    }
    Ok(())
}

fn is_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
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
