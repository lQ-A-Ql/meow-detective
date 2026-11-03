//! Case-scoped artifact object and reference lifecycle.

use artifact_storage::{ArtifactStorageError, ArtifactStore, ObjectAddress};
use persistence_sqlite::repositories::storage_control_repo::{
    ArtifactObjectRecord, ArtifactReferenceRecord, StorageControlRepo,
};
use rusqlite::Connection;
use thiserror::Error;
use uuid::Uuid;

const READY: &str = "ready";
const NONE_COMPRESSION: &str = "none";

#[derive(Debug, Error)]
pub enum ArtifactStorageServiceError {
    #[error("artifact storage operation failed: {0}")]
    Storage(#[source] ArtifactStorageError),
    #[error("artifact control database operation failed: {0}")]
    Database(#[source] persistence_sqlite::DbError),
    #[error("artifact input is invalid: {0}")]
    InvalidInput(String),
    #[error("artifact object was not found: {0}")]
    ObjectNotFound(String),
    #[error("artifact object is not ready: {object_id} ({state})")]
    ObjectNotReady { object_id: String, state: String },
    #[error("artifact reference was not found: {0}")]
    ReferenceNotFound(String),
}

impl From<ArtifactStorageError> for ArtifactStorageServiceError {
    fn from(error: ArtifactStorageError) -> Self {
        Self::Storage(error)
    }
}

impl From<persistence_sqlite::DbError> for ArtifactStorageServiceError {
    fn from(error: persistence_sqlite::DbError) -> Self {
        Self::Database(error)
    }
}

impl From<rusqlite::Error> for ArtifactStorageServiceError {
    fn from(error: rusqlite::Error) -> Self {
        Self::Database(error.into())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtifactObject {
    pub object_id: String,
    pub case_id: String,
    pub address: ObjectAddress,
    pub original_length: u64,
    pub stored_length: u64,
    pub media_type: String,
    pub compression: String,
    pub relative_path: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtifactReference {
    pub reference_id: String,
    pub case_id: String,
    pub object_id: String,
    pub source_id: Option<String>,
    pub owner_kind: String,
    pub owner_id: String,
    pub role: String,
}

pub struct ArtifactObjectBatchItem {
    pub stored: artifact_storage::StoredObject,
    pub media_type: String,
    pub source_id: Option<String>,
    pub owner_kind: String,
    pub owner_id: String,
    pub role: String,
}

pub struct ArtifactStorageService<'a> {
    connection: &'a Connection,
    store: &'a ArtifactStore,
}

pub type ArtifactStorageFacade<'a> = ArtifactStorageService<'a>;

impl<'a> ArtifactStorageService<'a> {
    pub fn new(connection: &'a Connection, store: &'a ArtifactStore) -> Self {
        Self { connection, store }
    }

    pub fn write_object(
        &self,
        case_id: &str,
        bytes: &[u8],
        media_type: &str,
    ) -> Result<ArtifactObject, ArtifactStorageServiceError> {
        write_object(self.connection, self.store, case_id, bytes, media_type)
    }

    pub fn write_file(
        &self,
        case_id: &str,
        path: &std::path::Path,
        media_type: &str,
    ) -> Result<ArtifactObject, ArtifactStorageServiceError> {
        write_file(self.connection, self.store, case_id, path, media_type)
    }

    pub fn create_reference(
        &self,
        case_id: &str,
        object_id: &str,
        source_id: Option<&str>,
        owner_kind: &str,
        owner_id: &str,
        role: &str,
    ) -> Result<ArtifactReference, ArtifactStorageServiceError> {
        let record = StorageControlRepo::new(self.connection)
            .find_artifact_by_id(case_id, object_id)?
            .ok_or_else(|| ArtifactStorageServiceError::ObjectNotFound(object_id.to_owned()))?;
        if record.write_state != READY {
            return Err(ArtifactStorageServiceError::ObjectNotReady {
                object_id: object_id.to_owned(),
                state: record.write_state,
            });
        }
        let address = ObjectAddress::parse(&record.sha256)?;
        self.store.verify(&address)?;
        create_reference(
            self.connection,
            case_id,
            object_id,
            source_id,
            owner_kind,
            owner_id,
            role,
        )
    }

    pub fn release_reference(
        &self,
        case_id: &str,
        reference_id: &str,
    ) -> Result<(), ArtifactStorageServiceError> {
        release_reference(self.connection, case_id, reference_id)
    }

    pub fn read_object(
        &self,
        case_id: &str,
        object_id: &str,
    ) -> Result<Vec<u8>, ArtifactStorageServiceError> {
        read_object(self.connection, self.store, case_id, object_id)
    }

    pub fn cleanup_unreferenced(
        &self,
        case_id: &str,
    ) -> Result<usize, ArtifactStorageServiceError> {
        let repo = StorageControlRepo::new(self.connection);
        let candidates = repo.list_unreferenced_ready_artifacts(case_id)?;
        let mut removed = 0usize;
        for candidate in candidates {
            let address = ObjectAddress::parse(&candidate.sha256)?;
            if self.store.remove(&address)? {
                repo.update_artifact_state(&candidate.object_id, "deleted")?;
                removed = removed.saturating_add(1);
            }
        }
        Ok(removed)
    }
}

pub fn write_object(
    connection: &Connection,
    store: &ArtifactStore,
    case_id: &str,
    bytes: &[u8],
    media_type: &str,
) -> Result<ArtifactObject, ArtifactStorageServiceError> {
    validate_text("case_id", case_id)?;
    validate_text("media_type", media_type)?;

    let stored = store.put_bytes(bytes)?;
    register_stored_object(connection, store, case_id, stored, media_type)
}

pub fn write_file(
    connection: &Connection,
    store: &ArtifactStore,
    case_id: &str,
    path: &std::path::Path,
    media_type: &str,
) -> Result<ArtifactObject, ArtifactStorageServiceError> {
    validate_text("case_id", case_id)?;
    validate_text("media_type", media_type)?;
    let stored = store.put_file(path)?;
    register_stored_object(connection, store, case_id, stored, media_type)
}

fn register_stored_object(
    connection: &Connection,
    _store: &ArtifactStore,
    case_id: &str,
    stored: artifact_storage::StoredObject,
    media_type: &str,
) -> Result<ArtifactObject, ArtifactStorageServiceError> {
    let address = stored.address;
    let repo = StorageControlRepo::new(connection);
    if let Some(existing) = repo.find_artifact_by_digest(case_id, address.as_str())? {
        if existing.write_state != READY {
            repo.update_artifact_state(&existing.object_id, READY)?;
        }
        return object_from_record(existing);
    }

    let object_id = Uuid::new_v4().to_string();
    let relative_path = relative_object_path(&address);
    let record = ArtifactObjectRecord {
        object_id,
        case_id: case_id.to_owned(),
        sha256: address.as_str().to_owned(),
        original_length: stored.length as i64,
        stored_length: stored.length as i64,
        media_type: media_type.to_owned(),
        compression: NONE_COMPRESSION.to_owned(),
        relative_path,
        write_state: READY.to_owned(),
    };
    repo.register_artifact_object(&record)?;
    object_from_record(record)
}

pub fn create_reference(
    connection: &Connection,
    case_id: &str,
    object_id: &str,
    source_id: Option<&str>,
    owner_kind: &str,
    owner_id: &str,
    role: &str,
) -> Result<ArtifactReference, ArtifactStorageServiceError> {
    validate_text("case_id", case_id)?;
    validate_text("object_id", object_id)?;
    validate_text("owner_kind", owner_kind)?;
    validate_text("owner_id", owner_id)?;
    validate_text("role", role)?;

    let repo = StorageControlRepo::new(connection);
    if let Some(existing) = repo
        .find_active_artifact_reference(case_id, object_id, source_id, owner_kind, owner_id, role)?
    {
        return Ok(reference_from_record(existing));
    }
    match repo.find_artifact_by_id(case_id, object_id)? {
        None => {
            return Err(ArtifactStorageServiceError::ObjectNotFound(
                object_id.to_owned(),
            ))
        }
        Some(record) if record.write_state != READY => {
            return Err(ArtifactStorageServiceError::ObjectNotReady {
                object_id: object_id.to_owned(),
                state: record.write_state,
            });
        }
        Some(_) => {}
    }

    let reference_id = Uuid::new_v4().to_string();
    repo.insert_artifact_reference(&ArtifactReferenceRecord {
        reference_id: reference_id.clone(),
        case_id: case_id.to_owned(),
        object_id: object_id.to_owned(),
        source_id: source_id.map(str::to_owned),
        owner_kind: owner_kind.to_owned(),
        owner_id: owner_id.to_owned(),
        role: role.to_owned(),
        released_at: None,
    })?;
    Ok(ArtifactReference {
        reference_id,
        case_id: case_id.to_owned(),
        object_id: object_id.to_owned(),
        source_id: source_id.map(str::to_owned),
        owner_kind: owner_kind.to_owned(),
        owner_id: owner_id.to_owned(),
        role: role.to_owned(),
    })
}

pub fn register_artifact_objects_and_references_batch(
    connection: &Connection,
    case_id: &str,
    items: &[ArtifactObjectBatchItem],
) -> Result<(), ArtifactStorageServiceError> {
    validate_text("case_id", case_id)?;
    if items.is_empty() {
        return Ok(());
    }
    let transaction = connection.unchecked_transaction()?;
    let mut object_insert = transaction.prepare_cached(
        "INSERT OR IGNORE INTO artifact_objects (
            object_id, case_id, sha256, original_length, stored_length, media_type,
            compression, relative_path, write_state
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 'ready')",
    )?;
    let mut object_lookup = transaction.prepare_cached(
        "SELECT object_id FROM artifact_objects WHERE case_id = ?1 AND sha256 = ?2",
    )?;
    let mut reference_insert = transaction.prepare_cached(
        "INSERT OR IGNORE INTO artifact_references (
            reference_id, case_id, object_id, source_id, owner_kind, owner_id, role
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
    )?;
    for item in items {
        validate_text("media_type", &item.media_type)?;
        validate_text("owner_kind", &item.owner_kind)?;
        validate_text("owner_id", &item.owner_id)?;
        validate_text("role", &item.role)?;
        let address = item.stored.address.as_str().to_owned();
        let length = i64::try_from(item.stored.length).map_err(|_| {
            ArtifactStorageServiceError::InvalidInput(
                "artifact length exceeds SQLite range".to_owned(),
            )
        })?;
        object_insert.execute(rusqlite::params![
            Uuid::new_v4().to_string(),
            case_id,
            address,
            length,
            length,
            item.media_type,
            NONE_COMPRESSION,
            relative_object_path(&item.stored.address),
        ])?;
        let object_id: String =
            object_lookup.query_row(rusqlite::params![case_id, address], |row| row.get(0))?;
        reference_insert.execute(rusqlite::params![
            Uuid::new_v4().to_string(),
            case_id,
            object_id,
            item.source_id,
            item.owner_kind,
            item.owner_id,
            item.role,
        ])?;
    }
    drop(reference_insert);
    drop(object_lookup);
    drop(object_insert);
    transaction.commit()?;
    Ok(())
}

pub fn release_reference(
    connection: &Connection,
    case_id: &str,
    reference_id: &str,
) -> Result<(), ArtifactStorageServiceError> {
    validate_text("case_id", case_id)?;
    validate_text("reference_id", reference_id)?;
    if !StorageControlRepo::new(connection).release_artifact_reference(case_id, reference_id)? {
        return Err(ArtifactStorageServiceError::ReferenceNotFound(
            reference_id.to_owned(),
        ));
    }
    Ok(())
}

pub fn read_object(
    connection: &Connection,
    store: &ArtifactStore,
    case_id: &str,
    object_id: &str,
) -> Result<Vec<u8>, ArtifactStorageServiceError> {
    validate_text("case_id", case_id)?;
    validate_text("object_id", object_id)?;
    let repo = StorageControlRepo::new(connection);
    let record = repo
        .find_artifact_by_id(case_id, object_id)?
        .ok_or_else(|| ArtifactStorageServiceError::ObjectNotFound(object_id.to_owned()))?;
    if record.write_state != READY {
        return Err(ArtifactStorageServiceError::ObjectNotReady {
            object_id: object_id.to_owned(),
            state: record.write_state,
        });
    }
    let address = ObjectAddress::parse(&record.sha256)?;
    let _ = repo
        .find_artifact_by_digest(case_id, address.as_str())?
        .ok_or_else(|| ArtifactStorageServiceError::ObjectNotFound(object_id.to_owned()))?;
    let bytes = store.read(&address)?;
    let stored_length = u64::try_from(record.stored_length).map_err(|_| {
        ArtifactStorageServiceError::InvalidInput("negative stored length".to_owned())
    })?;
    if bytes.len() as u64 != stored_length {
        return Err(ArtifactStorageServiceError::Storage(
            ArtifactStorageError::LengthMismatch {
                address,
                expected_length: stored_length,
                actual_length: bytes.len() as u64,
            },
        ));
    }
    Ok(bytes)
}

fn reference_from_record(record: ArtifactReferenceRecord) -> ArtifactReference {
    ArtifactReference {
        reference_id: record.reference_id,
        case_id: record.case_id,
        object_id: record.object_id,
        source_id: record.source_id,
        owner_kind: record.owner_kind,
        owner_id: record.owner_id,
        role: record.role,
    }
}

fn object_from_record(
    record: ArtifactObjectRecord,
) -> Result<ArtifactObject, ArtifactStorageServiceError> {
    let address = ObjectAddress::parse(&record.sha256)?;
    let original_length = u64::try_from(record.original_length).map_err(|_| {
        ArtifactStorageServiceError::InvalidInput("negative original length".to_owned())
    })?;
    let stored_length = u64::try_from(record.stored_length).map_err(|_| {
        ArtifactStorageServiceError::InvalidInput("negative stored length".to_owned())
    })?;
    Ok(ArtifactObject {
        object_id: record.object_id,
        case_id: record.case_id,
        address,
        original_length,
        stored_length,
        media_type: record.media_type,
        compression: record.compression,
        relative_path: record.relative_path,
    })
}

fn relative_object_path(address: &ObjectAddress) -> String {
    format!(
        "artifacts/objects/{}/{}/{}",
        address.first_prefix(),
        address.second_prefix(),
        address.as_str()
    )
}

fn validate_text(field: &str, value: &str) -> Result<(), ArtifactStorageServiceError> {
    if value.trim().is_empty() {
        return Err(ArtifactStorageServiceError::InvalidInput(format!(
            "{field} must not be empty"
        )));
    }
    Ok(())
}
