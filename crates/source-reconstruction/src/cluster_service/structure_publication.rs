use std::path::Path;
use std::sync::atomic::AtomicBool;

use domain::{structure::StructureViewAttempt, CaseId, DataSource};
use persistence_source::fjall::{
    encode_key, KeyPart, KeyspaceName, PersistMode, VersionedEnvelope,
};
use persistence_sqlite::repositories::{
    catalog_publication_repo::CatalogPublicationRepo, datasource_repo::DataSourceRepo,
    structure_view_attempt_repo::StructureViewAttemptRepo,
};
use source_kernel::source_db;

const STRUCTURE_VIEW_GENERATION: &str = "source.fjall";

#[derive(Debug, thiserror::Error)]
pub enum StructurePublicationError {
    #[error("structure publication database operation failed: {0}")]
    Database(#[from] persistence_sqlite::DbError),
    #[error("structure publication storage operation failed: {0}")]
    Storage(#[from] persistence_source::runtime::StorageRuntimeError),
    #[error("structure publication generation operation failed: {0}")]
    Generation(#[from] persistence_source::fjall::FjallStorageError),
    #[error("structure publication I/O failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("structure publication is inconsistent")]
    InvalidPublication,
    #[error("structure publication validation failed: {0}")]
    Validation(String),
    #[error("structure publication was cancelled")]
    Cancelled,
}

impl From<rusqlite::Error> for StructurePublicationError {
    fn from(error: rusqlite::Error) -> Self {
        Self::Database(error.into())
    }
}

impl transport::ServiceErrorCategory for StructurePublicationError {
    fn category(&self) -> transport::ErrorCategory {
        match self {
            Self::Database(_) | Self::Storage(_) | Self::Generation(_) | Self::Io(_) => {
                transport::ErrorCategory::Io
            }
            Self::InvalidPublication | Self::Validation(_) => transport::ErrorCategory::Parser,
            Self::Cancelled => transport::ErrorCategory::Cancelled,
        }
    }
}

pub trait StructurePublicationAttempt {
    fn attempt_id(&self) -> &str;
    fn owner_id(&self) -> &str;
    fn refresh(
        &self,
        conn: &rusqlite::Connection,
        source: &domain::DataSourceId,
        fingerprint: &str,
    ) -> Result<(), StructurePublicationError>;
    fn finish_ready(
        &self,
        conn: &rusqlite::Connection,
        source: &domain::DataSourceId,
        fingerprint: &str,
    ) -> Result<(), StructurePublicationError>;
    fn finish_failed(
        &self,
        conn: &rusqlite::Connection,
        source: &domain::DataSourceId,
        fingerprint: &str,
        error: &str,
        cancelled: bool,
    ) -> Result<(), StructurePublicationError>;
}

pub trait StructurePublicationValidator {
    fn validate(
        &self,
        conn: &rusqlite::Connection,
        case_id: &CaseId,
        source: &DataSource,
        fingerprint: &str,
    ) -> Result<(), StructurePublicationError>;
}

pub struct StructureBuildRequest<'a> {
    pub case_conn: &'a rusqlite::Connection,
    pub case_root: &'a Path,
    pub case_id: &'a CaseId,
    pub source: &'a DataSource,
    pub fingerprint: &'a str,
    pub attempt: &'a dyn StructurePublicationAttempt,
    pub validator: &'a dyn StructurePublicationValidator,
    pub cancel: &'a AtomicBool,
}

pub fn seal_catalog(
    request: &StructureBuildRequest<'_>,
    source_conn: &rusqlite::Connection,
) -> Result<String, StructurePublicationError> {
    let store = persistence_source::detail::open_store_for_write(source_conn)?
        .ok_or(StructurePublicationError::InvalidPublication)?;
    let (keyspace, key, marker) =
        evidence_access::partition_storage_service::partition_projection_marker(
            &request.source.id.0,
        )?;
    store.write_envelope(keyspace, key, &marker, PersistMode::Buffer)?;
    let key = encode_key("projection", &[KeyPart::Text(request.source.id.0.clone())])?;
    let marker = VersionedEnvelope::new(
        "source_projection_complete",
        1,
        format!("projection:{}", request.source.id.0),
        &request.source.id.0,
        1,
        request.source.id.0.as_bytes().to_vec(),
    );
    store.write_envelope(KeyspaceName::Meta, key, &marker, PersistMode::SyncAll)?;
    persistence_source::fjall::seal_source_generation_with_control(
        &store,
        &request.source.id.0,
        request.attempt.attempt_id(),
        request.fingerprint,
        request.cancel,
        |_, _| {},
    )?;
    let manifest = persistence_source::fjall::verify_source_generation_with_control(
        &store,
        &request.source.id.0,
        request.cancel,
        |_, _| {},
    )?;
    Ok(manifest.immutable_data_digest)
}

pub fn publish_build(
    request: &StructureBuildRequest<'_>,
    digest: &str,
) -> Result<(), StructurePublicationError> {
    ensure_not_cancelled(request.cancel)?;
    request.validator.validate(
        request.case_conn,
        request.case_id,
        request.source,
        request.fingerprint,
    )?;
    request
        .attempt
        .refresh(request.case_conn, &request.source.id, request.fingerprint)?;
    let publication = CatalogPublicationRepo::new(request.case_conn).prepare(
        &request.source.id,
        request.attempt.attempt_id(),
        request.fingerprint,
        &source_db::canonical_source_db_rel_path(&request.source.id),
        digest,
    )?;
    source_db::publish_source_build_db(
        request.case_root,
        &request.source.id,
        request.attempt.attempt_id(),
    )?;
    let transaction = request.case_conn.unchecked_transaction()?;
    request.validator.validate(
        &transaction,
        request.case_id,
        request.source,
        request.fingerprint,
    )?;
    bind_view_attempt(
        &transaction,
        request.case_id,
        &request.source.id.0,
        request.fingerprint,
        request.attempt.attempt_id(),
        request.attempt.owner_id(),
        digest,
    )?;
    CatalogPublicationRepo::new(&transaction).mark_published(
        &request.source.id,
        &publication.attempt_id,
        &publication.seal,
    )?;
    DataSourceRepo::new(&transaction).update_import_state(&request.source.id, "ready", None)?;
    request
        .attempt
        .finish_ready(&transaction, &request.source.id, request.fingerprint)?;
    transaction.commit()?;
    Ok(())
}

pub fn verify_publication(
    conn: &rusqlite::Connection,
    case_root: &Path,
    source: &DataSource,
    fingerprint: &str,
    case_id: &CaseId,
    validator: &dyn StructurePublicationValidator,
) -> Result<(), StructurePublicationError> {
    validator.validate(conn, case_id, source, fingerprint)?;
    let publications = CatalogPublicationRepo::new(conn);
    let stored = publications
        .find(&source.id)?
        .ok_or(StructurePublicationError::InvalidPublication)?;
    let native = source_db::open_registered_fjall_source_read_only(case_root, &source.id)?;
    let manifest = native.generation_manifest(&source.id.0)?;
    let snapshot = native.snapshot();
    native.require_projection_complete(&snapshot, &source.id.0)?;
    let attempt = StructureViewAttemptRepo::new(conn)
        .find_for_attempt(&case_id.0, &source.id.0, &stored.attempt_id)?
        .ok_or(StructurePublicationError::InvalidPublication)?;
    if !native.partition_projection_complete(&snapshot, &source.id.0)?
        || manifest.projection_batch_digest != fingerprint
        || attempt.attempt_id != stored.attempt_id
        || attempt.input_fingerprint != fingerprint
        || attempt.generation != STRUCTURE_VIEW_GENERATION
        || attempt.receipt != manifest.immutable_data_digest
        || !publications.is_published(
            &source.id,
            fingerprint,
            &source_db::canonical_source_db_rel_path(&source.id),
            &manifest.immutable_data_digest,
        )?
    {
        return Err(StructurePublicationError::InvalidPublication);
    }
    Ok(())
}

pub fn recover_prepared_publication(
    request: &StructureBuildRequest<'_>,
) -> Result<bool, StructurePublicationError> {
    let path = source_db::source_db_path(request.case_root, &request.source.id);
    let stored = CatalogPublicationRepo::new(request.case_conn).find(&request.source.id)?;
    let Some(publication) = stored else {
        return if path.exists() {
            Err(StructurePublicationError::InvalidPublication)
        } else {
            Ok(false)
        };
    };
    if publication.input_fingerprint != request.fingerprint {
        return Err(StructurePublicationError::InvalidPublication);
    }
    if !path.exists() {
        if publication.state != "prepared" {
            return Err(StructurePublicationError::InvalidPublication);
        }
        source_db::publish_source_build_db(
            request.case_root,
            &request.source.id,
            &publication.attempt_id,
        )?;
    }
    source_db::verify_finalized_source_db(&path, &request.source.id)?;
    let native =
        source_db::open_registered_fjall_source_read_only(request.case_root, &request.source.id)?;
    let manifest = native.generation_manifest(&request.source.id.0)?;
    if manifest.projection_batch_digest != request.fingerprint
        || manifest.immutable_data_digest != publication.catalog_digest
        || (publication.state == "prepared" && manifest.attempt_id != publication.attempt_id)
    {
        return Err(StructurePublicationError::InvalidPublication);
    }
    if publication.state == "prepared" {
        let transaction = request.case_conn.unchecked_transaction()?;
        let owner_id = StructureViewAttemptRepo::new(&transaction)
            .find_for_attempt(
                &request.case_id.0,
                &request.source.id.0,
                &publication.attempt_id,
            )?
            .map_or_else(
                || request.attempt.owner_id().to_owned(),
                |attempt| attempt.owner_id,
            );
        bind_view_attempt(
            &transaction,
            request.case_id,
            &request.source.id.0,
            request.fingerprint,
            &publication.attempt_id,
            &owner_id,
            &manifest.immutable_data_digest,
        )?;
        CatalogPublicationRepo::new(&transaction).mark_published(
            &request.source.id,
            &publication.attempt_id,
            &publication.seal,
        )?;
        transaction.commit()?;
    }
    verify_publication(
        request.case_conn,
        request.case_root,
        request.source,
        request.fingerprint,
        request.case_id,
        request.validator,
    )?;
    let transaction = request.case_conn.unchecked_transaction()?;
    DataSourceRepo::new(&transaction).update_import_state(&request.source.id, "ready", None)?;
    request
        .attempt
        .finish_ready(&transaction, &request.source.id, request.fingerprint)?;
    transaction.commit()?;
    Ok(true)
}

#[allow(clippy::too_many_arguments)]
fn bind_view_attempt(
    conn: &rusqlite::Connection,
    case_id: &CaseId,
    source_id: &str,
    input_fingerprint: &str,
    attempt_id: &str,
    owner_id: &str,
    receipt: &str,
) -> Result<(), StructurePublicationError> {
    StructureViewAttemptRepo::new(conn).bind(
        &case_id.0,
        &StructureViewAttempt {
            derived_source_id: source_id.into(),
            input_fingerprint: input_fingerprint.into(),
            attempt_id: attempt_id.into(),
            owner_id: owner_id.into(),
            generation: STRUCTURE_VIEW_GENERATION.into(),
            receipt: receipt.into(),
        },
    )?;
    Ok(())
}

pub fn ensure_not_cancelled(cancel: &AtomicBool) -> Result<(), StructurePublicationError> {
    if cancel.load(std::sync::atomic::Ordering::Acquire) {
        Err(StructurePublicationError::Cancelled)
    } else {
        Ok(())
    }
}
