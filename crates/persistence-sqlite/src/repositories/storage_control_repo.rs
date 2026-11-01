use crate::{DbError, DbResult};
use rusqlite::{params, Connection, OptionalExtension};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StorageOperationState {
    Planned,
    Running,
    Committed,
    Published,
    Failed,
    Cancelled,
}

impl StorageOperationState {
    fn as_str(self) -> &'static str {
        match self {
            Self::Planned => "planned",
            Self::Running => "running",
            Self::Committed => "committed",
            Self::Published => "published",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
        }
    }

    fn parse(value: String) -> DbResult<Self> {
        match value.as_str() {
            "planned" => Ok(Self::Planned),
            "running" => Ok(Self::Running),
            "committed" => Ok(Self::Committed),
            "published" => Ok(Self::Published),
            "failed" => Ok(Self::Failed),
            "cancelled" => Ok(Self::Cancelled),
            _ => Err(DbError::Migration(format!(
                "unknown storage operation state: {value}"
            ))),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StorageOperationRecord {
    pub operation_id: String,
    pub case_id: String,
    pub source_id: Option<String>,
    pub attempt_id: String,
    pub operation_kind: String,
    pub state: StorageOperationState,
    pub input_digest: Option<String>,
    pub expected_batch_digest: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub last_error_code: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StorageManifestRecord {
    pub manifest_id: String,
    pub case_id: String,
    pub source_id: Option<String>,
    pub relative_path: String,
    pub format_version: i64,
    pub schema_version: i64,
    pub manifest_digest: String,
    pub state: String,
    pub sealed_at: Option<String>,
    pub published_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtifactObjectRecord {
    pub object_id: String,
    pub case_id: String,
    pub sha256: String,
    pub original_length: i64,
    pub stored_length: i64,
    pub media_type: String,
    pub compression: String,
    pub relative_path: String,
    pub write_state: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StorageRepairEventRecord {
    pub event_id: String,
    pub case_id: String,
    pub source_id: Option<String>,
    pub operation_id: Option<String>,
    pub event_kind: String,
    pub state: String,
    pub details_json: Option<String>,
    pub error_code: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtifactReferenceRecord {
    pub reference_id: String,
    pub case_id: String,
    pub object_id: String,
    pub source_id: Option<String>,
    pub owner_kind: String,
    pub owner_id: String,
    pub role: String,
    pub released_at: Option<String>,
}

pub struct StorageControlRepo<'a> {
    connection: &'a Connection,
}

impl<'a> StorageControlRepo<'a> {
    pub fn new(connection: &'a Connection) -> Self {
        Self { connection }
    }

    pub fn insert_operation(&self, record: &StorageOperationRecord) -> DbResult<()> {
        self.connection.execute(
            "INSERT INTO storage_operations (
                operation_id, case_id, source_id, attempt_id, operation_kind,
                state, input_digest, expected_batch_digest, created_at, updated_at,
                last_error_code
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![
                record.operation_id,
                record.case_id,
                record.source_id,
                record.attempt_id,
                record.operation_kind,
                record.state.as_str(),
                record.input_digest,
                record.expected_batch_digest,
                record.created_at,
                record.updated_at,
                record.last_error_code,
            ],
        )?;
        Ok(())
    }

    pub fn find_operation(&self, operation_id: &str) -> DbResult<Option<StorageOperationRecord>> {
        self.connection
            .query_row(
                "SELECT operation_id, case_id, source_id, attempt_id, operation_kind,
                        state, input_digest, expected_batch_digest, created_at,
                        updated_at, last_error_code
                 FROM storage_operations WHERE operation_id = ?1",
                [operation_id],
                map_operation,
            )
            .optional()
            .map_err(Into::into)
    }

    pub fn update_operation_state(
        &self,
        operation_id: &str,
        state: StorageOperationState,
        updated_at: &str,
        last_error_code: Option<&str>,
    ) -> DbResult<bool> {
        let changed = self.connection.execute(
            "UPDATE storage_operations
             SET state = ?2, updated_at = ?3, last_error_code = ?4
             WHERE operation_id = ?1",
            params![operation_id, state.as_str(), updated_at, last_error_code],
        )?;
        Ok(changed == 1)
    }

    pub fn list_open_operations(&self, case_id: &str) -> DbResult<Vec<StorageOperationRecord>> {
        let mut statement = self.connection.prepare(
            "SELECT operation_id, case_id, source_id, attempt_id, operation_kind,
                    state, input_digest, expected_batch_digest, created_at,
                    updated_at, last_error_code
             FROM storage_operations
             WHERE case_id = ?1 AND state IN ('planned', 'running', 'committed')
             ORDER BY updated_at ASC, operation_id ASC",
        )?;
        let rows = statement.query_map([case_id], map_operation)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    pub fn insert_manifest(&self, record: &StorageManifestRecord) -> DbResult<()> {
        self.connection.execute(
            "INSERT INTO storage_manifests (
                manifest_id, case_id, source_id, relative_path, format_version,
                schema_version, manifest_digest, state, sealed_at, published_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                record.manifest_id,
                record.case_id,
                record.source_id,
                record.relative_path,
                record.format_version,
                record.schema_version,
                record.manifest_digest,
                record.state,
                record.sealed_at,
                record.published_at,
            ],
        )?;
        Ok(())
    }

    pub fn update_manifest_state(
        &self,
        manifest_id: &str,
        state: &str,
        sealed_at: Option<&str>,
        published_at: Option<&str>,
    ) -> DbResult<bool> {
        let changed = self.connection.execute(
            "UPDATE storage_manifests
             SET state = ?2, sealed_at = ?3, published_at = ?4, updated_at = datetime('now')
             WHERE manifest_id = ?1",
            params![manifest_id, state, sealed_at, published_at],
        )?;
        Ok(changed == 1)
    }

    pub fn find_manifest_by_path(
        &self,
        case_id: &str,
        relative_path: &str,
    ) -> DbResult<Option<StorageManifestRecord>> {
        self.connection
            .query_row(
                "SELECT manifest_id, case_id, source_id, relative_path, format_version,
                        schema_version, manifest_digest, state, sealed_at, published_at
                 FROM storage_manifests
                 WHERE case_id = ?1 AND relative_path = ?2",
                params![case_id, relative_path],
                map_manifest,
            )
            .optional()
            .map_err(Into::into)
    }

    pub fn register_artifact_object(&self, record: &ArtifactObjectRecord) -> DbResult<()> {
        self.connection.execute(
            "INSERT INTO artifact_objects (
                object_id, case_id, sha256, original_length, stored_length, media_type,
                compression, relative_path, write_state
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                record.object_id,
                record.case_id,
                record.sha256,
                record.original_length,
                record.stored_length,
                record.media_type,
                record.compression,
                record.relative_path,
                record.write_state,
            ],
        )?;
        Ok(())
    }

    pub fn update_artifact_state(&self, object_id: &str, write_state: &str) -> DbResult<bool> {
        let changed = self.connection.execute(
            "UPDATE artifact_objects
             SET write_state = ?2, updated_at = datetime('now')
             WHERE object_id = ?1",
            params![object_id, write_state],
        )?;
        Ok(changed == 1)
    }

    pub fn find_artifact_by_digest(
        &self,
        case_id: &str,
        sha256: &str,
    ) -> DbResult<Option<ArtifactObjectRecord>> {
        self.connection
            .query_row(
                "SELECT object_id, case_id, sha256, original_length, stored_length,
                        media_type, compression, relative_path, write_state
                 FROM artifact_objects WHERE case_id = ?1 AND sha256 = ?2",
                params![case_id, sha256],
                map_artifact_object,
            )
            .optional()
            .map_err(Into::into)
    }

    pub fn find_artifact_by_id(
        &self,
        case_id: &str,
        object_id: &str,
    ) -> DbResult<Option<ArtifactObjectRecord>> {
        self.connection
            .query_row(
                "SELECT object_id, case_id, sha256, original_length, stored_length,
                        media_type, compression, relative_path, write_state
                 FROM artifact_objects WHERE object_id = ?1 AND case_id = ?2",
                params![object_id, case_id],
                map_artifact_object,
            )
            .optional()
            .map_err(Into::into)
    }

    pub fn insert_artifact_reference(&self, record: &ArtifactReferenceRecord) -> DbResult<()> {
        self.connection.execute(
            "INSERT INTO artifact_references (
                reference_id, case_id, object_id, source_id, owner_kind, owner_id, role
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                record.reference_id,
                record.case_id,
                record.object_id,
                record.source_id,
                record.owner_kind,
                record.owner_id,
                record.role,
            ],
        )?;
        Ok(())
    }

    pub fn release_artifact_reference(&self, case_id: &str, reference_id: &str) -> DbResult<bool> {
        let changed = self.connection.execute(
            "UPDATE artifact_references
             SET released_at = datetime('now')
             WHERE reference_id = ?1 AND case_id = ?2 AND released_at IS NULL",
            params![reference_id, case_id],
        )?;
        Ok(changed == 1)
    }

    pub fn insert_repair_event(&self, record: &StorageRepairEventRecord) -> DbResult<()> {
        self.connection.execute(
            "INSERT INTO storage_repair_events (
                event_id, case_id, source_id, operation_id, event_kind, state,
                details_json, error_code
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                record.event_id,
                record.case_id,
                record.source_id,
                record.operation_id,
                record.event_kind,
                record.state,
                record.details_json,
                record.error_code,
            ],
        )?;
        Ok(())
    }
}

fn map_operation(row: &rusqlite::Row<'_>) -> rusqlite::Result<StorageOperationRecord> {
    Ok(StorageOperationRecord {
        operation_id: row.get(0)?,
        case_id: row.get(1)?,
        source_id: row.get(2)?,
        attempt_id: row.get(3)?,
        operation_kind: row.get(4)?,
        state: StorageOperationState::parse(row.get(5)?).map_err(to_sql_error)?,
        input_digest: row.get(6)?,
        expected_batch_digest: row.get(7)?,
        created_at: row.get(8)?,
        updated_at: row.get(9)?,
        last_error_code: row.get(10)?,
    })
}

fn map_manifest(row: &rusqlite::Row<'_>) -> rusqlite::Result<StorageManifestRecord> {
    Ok(StorageManifestRecord {
        manifest_id: row.get(0)?,
        case_id: row.get(1)?,
        source_id: row.get(2)?,
        relative_path: row.get(3)?,
        format_version: row.get(4)?,
        schema_version: row.get(5)?,
        manifest_digest: row.get(6)?,
        state: row.get(7)?,
        sealed_at: row.get(8)?,
        published_at: row.get(9)?,
    })
}

fn map_artifact_object(row: &rusqlite::Row<'_>) -> rusqlite::Result<ArtifactObjectRecord> {
    Ok(ArtifactObjectRecord {
        object_id: row.get(0)?,
        case_id: row.get(1)?,
        sha256: row.get(2)?,
        original_length: row.get(3)?,
        stored_length: row.get(4)?,
        media_type: row.get(5)?,
        compression: row.get(6)?,
        relative_path: row.get(7)?,
        write_state: row.get(8)?,
    })
}

fn to_sql_error(error: DbError) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(error))
}

#[cfg(test)]
#[path = "../../tests/unit/repositories/storage_control_repo.rs"]
mod tests;
