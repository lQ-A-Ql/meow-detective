use std::{path::Path, sync::atomic::AtomicBool};

use domain::{CaseId, DataSource};
use evidence_access::lvm::{open_lvm_volume_reader, LvmPlanError, LvmVolumePlan};
use persistence_sqlite::repositories::datasource_repo::DataSourceRepo;
use source_kernel::source_db;

use crate::cluster_service::structure_publication::{
    self, StructureBuildRequest, StructurePublicationAttempt, StructurePublicationError,
};

use super::phase::LvmAttempt;

pub(super) struct CatalogBuild<'a> {
    pub case_conn: &'a rusqlite::Connection,
    pub case_root: &'a Path,
    pub case_id: &'a CaseId,
    pub source: &'a DataSource,
    pub plan: &'a LvmVolumePlan,
    pub fingerprint: &'a str,
    pub attempt: &'a LvmAttempt,
    pub validator: &'a super::validator::LvmValidator<'a>,
    pub cancel: &'a AtomicBool,
}

pub(super) fn build_and_publish(request: CatalogBuild<'_>) -> Result<(), LvmPlanError> {
    let publication = publication_request(&request);
    if structure_publication::recover_prepared_publication(&publication)? {
        return Ok(());
    }
    let source_conn = source_db::open_fresh_source_build_db(
        request.case_root,
        &request.source.id,
        request.attempt.attempt_id(),
    )?;
    let result = populate_and_seal(&request, &source_conn, &publication);
    drop(source_conn);
    match result {
        Ok(digest) => structure_publication::publish_build(&publication, &digest)?,
        Err(error) => {
            discard_build(&request)?;
            return Err(error);
        }
    }
    Ok(())
}

fn publication_request<'a>(request: &'a CatalogBuild<'a>) -> StructureBuildRequest<'a> {
    StructureBuildRequest {
        case_conn: request.case_conn,
        case_root: request.case_root,
        case_id: request.case_id,
        source: request.source,
        fingerprint: request.fingerprint,
        attempt: request.attempt,
        validator: request.validator,
        cancel: request.cancel,
    }
}

fn populate_and_seal(
    request: &CatalogBuild<'_>,
    source_conn: &rusqlite::Connection,
    publication: &StructureBuildRequest<'_>,
) -> Result<String, LvmPlanError> {
    DataSourceRepo::new(source_conn)
        .upsert_source_local_metadata(request.case_id, request.source)?;
    super::catalog::enumerate_lvm_catalog(request.source, source_conn, request.cancel)?;
    super::super::structure_publication::ensure_not_cancelled(request.cancel)?;
    drop(open_lvm_volume_reader(
        request.plan,
        &AtomicBool::new(false),
    )?);
    request
        .attempt
        .refresh(request.case_conn, &request.source.id, request.fingerprint)
        .map_err(LvmPlanError::from)?;
    let digest = structure_publication::seal_catalog(publication, source_conn)?;
    source_db::finalize_source_build_db(source_conn)?;
    Ok(digest)
}

fn discard_build(request: &CatalogBuild<'_>) -> Result<(), LvmPlanError> {
    source_db::discard_source_build_db(
        request.case_root,
        &request.source.id,
        request.attempt.attempt_id(),
    )?;
    let layout = persistence_source::fjall::SourceBuildLayout::for_case_source(
        request.case_root,
        &request.source.id.0,
        request.attempt.attempt_id(),
    )?;
    let path = layout.build_path();
    if path.exists() {
        let parent = source_db::source_dir(request.case_root, &request.source.id).canonicalize()?;
        let resolved = path.canonicalize()?;
        if resolved.parent() != Some(parent.as_path()) {
            return Err(LvmPlanError::InvalidPublication);
        }
        let _guard = source_kernel::storage_runtime::close_source_store_at_path(&path)?
            .ok_or(LvmPlanError::InvalidPublication)?;
        std::fs::remove_dir_all(resolved)?;
    }
    Ok(())
}

impl From<StructurePublicationError> for LvmPlanError {
    fn from(error: StructurePublicationError) -> Self {
        match error {
            StructurePublicationError::Database(error) => Self::Database(error),
            StructurePublicationError::Storage(error) => Self::Storage(error),
            StructurePublicationError::Generation(error) => Self::Generation(error),
            StructurePublicationError::Io(error) => Self::Io(error),
            StructurePublicationError::InvalidPublication
            | StructurePublicationError::Validation(_) => Self::InvalidPublication,
            StructurePublicationError::Cancelled => Self::Cancelled,
        }
    }
}
