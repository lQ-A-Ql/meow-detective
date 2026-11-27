use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

use domain::{structure::StructureViewAttempt, CaseId, DataSource, DataSourceId};
use persistence_fjall::{encode_key, KeyPart, KeyspaceName, PersistMode, VersionedEnvelope};
use persistence_sqlite::repositories::{
    catalog_publication_repo::{seal_for, CatalogPublication, CatalogPublicationRepo},
    datasource_repo::DataSourceRepo,
    structure_view_attempt_repo::StructureViewAttemptRepo,
};

use crate::control::paths::{canonical_source_db_rel_path, source_db_path};
use crate::publication::build::{publish_source_build_db, verify_finalized_source_db};

const STRUCTURE_VIEW_GENERATION: &str = "source.fjall";

mod recovery_files;
pub use recovery_files::restore_prepared_build;
mod cleanup;
pub use cleanup::discard_unpublished_catalog_build;
mod registered;
pub use registered::verify_registered_catalog;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CatalogProjection {
    SourceOnly,
    Partition,
}

#[derive(Debug, thiserror::Error)]
pub enum StructurePublicationError {
    #[error("structure publication database operation failed: {0}")]
    Database(#[from] persistence_sqlite::DbError),
    #[error("structure publication storage operation failed: {0}")]
    Storage(#[from] crate::runtime::StorageRuntimeError),
    #[error("structure publication generation operation failed: {0}")]
    Generation(#[from] persistence_fjall::FjallStorageError),
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

pub trait StructurePublicationAttempt {
    fn attempt_id(&self) -> &str;
    fn owner_id(&self) -> &str;
    fn refresh(
        &self,
        conn: &rusqlite::Connection,
        source: &DataSourceId,
        fingerprint: &str,
    ) -> Result<(), StructurePublicationError>;
    fn finish_ready(
        &self,
        conn: &rusqlite::Connection,
        source: &DataSourceId,
        fingerprint: &str,
    ) -> Result<(), StructurePublicationError>;
    fn finish_failed(
        &self,
        conn: &rusqlite::Connection,
        source: &DataSourceId,
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
    pub projection: CatalogProjection,
    pub cancel: &'a AtomicBool,
}

impl StructureBuildRequest<'_> {
    fn fence(&self, conn: &rusqlite::Connection) -> Result<(), StructurePublicationError> {
        ensure_not_cancelled(self.cancel)?;
        self.attempt
            .refresh(conn, &self.source.id, self.fingerprint)?;
        self.validator
            .validate(conn, self.case_id, self.source, self.fingerprint)
    }
}

pub fn seal_catalog_generation(
    source_conn: &rusqlite::Connection,
    source_id: &DataSourceId,
    attempt_id: &str,
    fingerprint: &str,
    projection: CatalogProjection,
    cancel: &AtomicBool,
) -> Result<String, StructurePublicationError> {
    ensure_not_cancelled(cancel)?;
    let store = crate::detail::open_store_for_write(source_conn)?
        .ok_or(StructurePublicationError::InvalidPublication)?;
    let expected = format!("source.fjall.build.{attempt_id}");
    if store
        .database()
        .path()
        .file_name()
        .and_then(|name| name.to_str())
        != Some(&expected)
    {
        return Err(StructurePublicationError::InvalidPublication);
    }
    if projection == CatalogProjection::Partition {
        let (keyspace, key, marker) =
            crate::runtime::partition_marker::partition_projection_marker(&source_id.0)?;
        store.write_envelope(keyspace, key, &marker, PersistMode::Buffer)?;
    }
    write_source_projection_complete(&store, &source_id.0)?;
    persistence_fjall::seal_source_generation_with_control(
        &store,
        &source_id.0,
        attempt_id,
        fingerprint,
        cancel,
        |_, _| {},
    )?;
    let manifest = persistence_fjall::verify_source_generation_with_control(
        &store,
        &source_id.0,
        cancel,
        |_, _| {},
    )?;
    Ok(manifest.immutable_data_digest)
}

pub fn seal_catalog(
    request: &StructureBuildRequest<'_>,
    source_conn: &rusqlite::Connection,
) -> Result<String, StructurePublicationError> {
    seal_catalog_generation(
        source_conn,
        &request.source.id,
        request.attempt.attempt_id(),
        request.fingerprint,
        request.projection,
        request.cancel,
    )
}

pub fn publish_build(
    request: &StructureBuildRequest<'_>,
    digest: &str,
) -> Result<(), StructurePublicationError> {
    ensure_not_cancelled(request.cancel)?;
    let transaction = request.case_conn.unchecked_transaction()?;
    request.fence(&transaction)?;
    let publication = CatalogPublicationRepo::new(&transaction).prepare(
        &request.source.id,
        request.attempt.attempt_id(),
        request.fingerprint,
        &canonical_source_db_rel_path(&request.source.id),
        digest,
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
    ensure_not_cancelled(request.cancel)?;
    transaction.commit()?;
    publish_source_build_db(
        request.case_root,
        &request.source.id,
        request.attempt.attempt_id(),
    )?;
    let transaction = request.case_conn.unchecked_transaction()?;
    request.fence(&transaction)?;
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
    verify_publication(
        &transaction,
        request.case_root,
        request.source,
        request.fingerprint,
        request.case_id,
        request.projection,
        request.validator,
    )?;
    DataSourceRepo::new(&transaction).update_import_state(&request.source.id, "ready", None)?;
    request
        .attempt
        .finish_ready(&transaction, &request.source.id, request.fingerprint)?;
    ensure_not_cancelled(request.cancel)?;
    transaction.commit()?;
    Ok(())
}

pub fn verify_publication(
    conn: &rusqlite::Connection,
    case_root: &Path,
    source: &DataSource,
    fingerprint: &str,
    case_id: &CaseId,
    projection: CatalogProjection,
    validator: &dyn StructurePublicationValidator,
) -> Result<(), StructurePublicationError> {
    validator.validate(conn, case_id, source, fingerprint)?;
    let publications = CatalogPublicationRepo::new(conn);
    let stored = publications
        .find(&source.id)?
        .ok_or(StructurePublicationError::InvalidPublication)?;
    let native = crate::runtime::open_source_read_only(case_root, &source.id.0)?;
    let manifest = native.generation_manifest(&source.id.0)?;
    let snapshot = native.snapshot();
    native.require_projection_complete(&snapshot, &source.id.0)?;
    let attempt = StructureViewAttemptRepo::new(conn)
        .find_for_attempt(&case_id.0, &source.id.0, &stored.attempt_id)?
        .ok_or(StructurePublicationError::InvalidPublication)?;
    if !native.source_projection_complete(&snapshot, &source.id.0)?
        || projection == CatalogProjection::Partition
            && !native.partition_projection_complete(&snapshot, &source.id.0)?
        || manifest.projection_batch_digest != fingerprint
        || attempt.attempt_id != stored.attempt_id
        || attempt.input_fingerprint != fingerprint
        || attempt.generation != STRUCTURE_VIEW_GENERATION
        || attempt.receipt != manifest.immutable_data_digest
        || !publications.is_published(
            &source.id,
            fingerprint,
            &canonical_source_db_rel_path(&source.id),
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
    ensure_not_cancelled(request.cancel)?;
    let path = source_db_path(request.case_root, &request.source.id);
    let stored = CatalogPublicationRepo::new(request.case_conn).find(&request.source.id)?;
    let Some(publication) = stored else {
        return if path.exists() {
            Err(StructurePublicationError::InvalidPublication)
        } else {
            Ok(false)
        };
    };
    validate_receipt(request, &publication)?;
    let transaction = request.case_conn.unchecked_transaction()?;
    request.fence(&transaction)?;
    transaction.commit()?;
    if !path.exists() {
        if publication.state != "prepared" {
            return Err(StructurePublicationError::InvalidPublication);
        }
        publish_source_build_db(
            request.case_root,
            &request.source.id,
            &publication.attempt_id,
        )?;
    }
    verify_finalized_source_db(&path, &request.source.id)?;
    let native = crate::runtime::open_source_read_only(request.case_root, &request.source.id.0)?;
    let manifest = native.generation_manifest(&request.source.id.0)?;
    let snapshot = native.snapshot();
    if !native.source_projection_complete(&snapshot, &request.source.id.0)?
        || request.projection == CatalogProjection::Partition
            && !native.partition_projection_complete(&snapshot, &request.source.id.0)?
        || manifest.projection_batch_digest != request.fingerprint
        || manifest.immutable_data_digest != publication.catalog_digest
        || (publication.state == "prepared" && manifest.attempt_id != publication.attempt_id)
    {
        return Err(StructurePublicationError::InvalidPublication);
    }
    let transaction = request.case_conn.unchecked_transaction()?;
    request.fence(&transaction)?;
    if CatalogPublicationRepo::new(&transaction).find(&request.source.id)?
        != Some(publication.clone())
    {
        return Err(StructurePublicationError::InvalidPublication);
    }
    if publication.state == "prepared" {
        CatalogPublicationRepo::new(&transaction).mark_published(
            &request.source.id,
            &publication.attempt_id,
            &publication.seal,
        )?;
    }
    verify_publication(
        &transaction,
        request.case_root,
        request.source,
        request.fingerprint,
        request.case_id,
        request.projection,
        request.validator,
    )?;
    DataSourceRepo::new(&transaction).update_import_state(&request.source.id, "ready", None)?;
    request
        .attempt
        .finish_ready(&transaction, &request.source.id, request.fingerprint)?;
    ensure_not_cancelled(request.cancel)?;
    transaction.commit()?;
    Ok(true)
}

fn validate_receipt(
    request: &StructureBuildRequest<'_>,
    publication: &CatalogPublication,
) -> Result<(), StructurePublicationError> {
    let path = canonical_source_db_rel_path(&request.source.id);
    if publication.input_fingerprint != request.fingerprint
        || publication.source_db_rel_path != path
        || publication.seal
            != seal_for(
                &request.source.id.0,
                &publication.attempt_id,
                request.fingerprint,
                &path,
                &publication.catalog_digest,
            )
    {
        return Err(StructurePublicationError::InvalidPublication);
    }
    let attempt = StructureViewAttemptRepo::new(request.case_conn)
        .find_for_attempt(
            &request.case_id.0,
            &request.source.id.0,
            &publication.attempt_id,
        )?
        .ok_or(StructurePublicationError::InvalidPublication)?;
    if attempt.input_fingerprint != request.fingerprint
        || attempt.receipt != publication.catalog_digest
        || attempt.generation != STRUCTURE_VIEW_GENERATION
    {
        return Err(StructurePublicationError::InvalidPublication);
    }
    Ok(())
}

fn write_source_projection_complete(
    store: &persistence_fjall::SourceStore,
    source_id: &str,
) -> Result<(), StructurePublicationError> {
    let key = encode_key("projection", &[KeyPart::Text(source_id.to_owned())])?;
    let marker = VersionedEnvelope::new(
        "source_projection_complete",
        1,
        format!("projection:{source_id}"),
        source_id,
        1,
        source_id.as_bytes().to_vec(),
    );
    store.write_envelope(KeyspaceName::Meta, key, &marker, PersistMode::SyncAll)?;
    Ok(())
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
    if cancel.load(Ordering::Acquire) {
        Err(StructurePublicationError::Cancelled)
    } else {
        Ok(())
    }
}
