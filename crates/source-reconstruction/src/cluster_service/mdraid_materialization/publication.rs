use std::path::Path;

use domain::{structure::StructureViewAttempt, CaseId, DataSource};
use evidence_access::mdraid::MdRaidPlanError;
use persistence_source::fjall::{
    encode_key, KeyPart, KeyspaceName, PersistMode, VersionedEnvelope,
};
use persistence_sqlite::repositories::{
    catalog_publication_repo::CatalogPublicationRepo, datasource_repo::DataSourceRepo,
    structure_view_attempt_repo::StructureViewAttemptRepo,
};
use source_kernel::source_db;

use super::build::CatalogBuild;

const STRUCTURE_VIEW_GENERATION: &str = "source.fjall";

pub(super) fn seal_catalog(
    request: &CatalogBuild<'_>,
    conn: &rusqlite::Connection,
) -> Result<String, MdRaidPlanError> {
    let store = persistence_source::detail::open_store_for_write(conn)?
        .ok_or(MdRaidPlanError::InvalidPublication)?;
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
        &request.attempt.id,
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

pub(super) fn publish_build(
    request: &CatalogBuild<'_>,
    digest: &str,
) -> Result<(), MdRaidPlanError> {
    super::ensure_not_cancelled(request.cancel)?;
    super::inputs::validate(request.case_conn, request.source)?;
    super::object_version::validate(
        request.case_conn,
        &request.case_id.0,
        &request.source.id.0,
        request.fingerprint,
    )?;
    request
        .attempt
        .refresh(request.case_conn, &request.source.id, request.fingerprint)?;
    let publication = CatalogPublicationRepo::new(request.case_conn).prepare(
        &request.source.id,
        &request.attempt.id,
        request.fingerprint,
        &source_db::canonical_source_db_rel_path(&request.source.id),
        digest,
    )?;
    source_db::publish_source_build_db(request.case_root, &request.source.id, &request.attempt.id)?;
    let transaction = request.case_conn.unchecked_transaction()?;
    super::inputs::validate(&transaction, request.source)?;
    bind_view_attempt(
        &transaction,
        request.case_id,
        &request.source.id.0,
        request.fingerprint,
        request.attempt.id(),
        request.attempt.owner(),
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

pub(super) fn verify_publication(
    conn: &rusqlite::Connection,
    case_root: &Path,
    source: &DataSource,
    fingerprint: &str,
) -> Result<(), MdRaidPlanError> {
    super::inputs::validate(conn, source)?;
    let case_id = DataSourceRepo::new(conn).case_id(&source.id)?;
    super::object_version::validate(conn, &case_id.0, &source.id.0, fingerprint)?;
    let publications = CatalogPublicationRepo::new(conn);
    let stored = publications
        .find(&source.id)?
        .ok_or(MdRaidPlanError::InvalidPublication)?;
    let native = source_db::open_registered_fjall_source_read_only(case_root, &source.id)?;
    let manifest = native.generation_manifest(&source.id.0)?;
    let snapshot = native.snapshot();
    native.require_projection_complete(&snapshot, &source.id.0)?;
    let attempt = StructureViewAttemptRepo::new(conn)
        .find_for_attempt(&case_id.0, &source.id.0, &stored.attempt_id)?
        .ok_or(MdRaidPlanError::InvalidPublication)?;
    if !native.partition_projection_complete(&snapshot, &source.id.0)?
        || manifest.attempt_id != stored.attempt_id
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
        return Err(MdRaidPlanError::InvalidPublication);
    }
    Ok(())
}

pub(super) fn recover_prepared_publication(
    request: &CatalogBuild<'_>,
) -> Result<bool, MdRaidPlanError> {
    let path = source_db::source_db_path(request.case_root, &request.source.id);
    let stored = CatalogPublicationRepo::new(request.case_conn).find(&request.source.id)?;
    let Some(publication) = stored else {
        return if path.exists() {
            Err(MdRaidPlanError::InvalidPublication)
        } else {
            Ok(false)
        };
    };
    if publication.input_fingerprint != request.fingerprint {
        return Err(MdRaidPlanError::InvalidPublication);
    }
    if !path.exists() {
        if publication.state != "prepared" {
            return Err(MdRaidPlanError::InvalidPublication);
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
    if manifest.attempt_id != publication.attempt_id
        || manifest.projection_batch_digest != request.fingerprint
        || manifest.immutable_data_digest != publication.catalog_digest
    {
        return Err(MdRaidPlanError::InvalidPublication);
    }
    if publication.state == "prepared" {
        let transaction = request.case_conn.unchecked_transaction()?;
        bind_view_attempt(
            &transaction,
            request.case_id,
            &request.source.id.0,
            request.fingerprint,
            request.attempt.id(),
            request.attempt.owner(),
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
) -> Result<(), MdRaidPlanError> {
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
