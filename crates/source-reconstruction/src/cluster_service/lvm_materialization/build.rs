use std::{path::Path, sync::atomic::AtomicBool};

use domain::{CaseId, DataSource};
use evidence_access::lvm::{open_lvm_volume_reader, LvmPlanError, LvmVolumePlan};
use persistence_sqlite::repositories::datasource_repo::DataSourceRepo;
use source_kernel::source_db;

use persistence_source::publication::catalog_publication as structure_publication;
use persistence_source::publication::catalog_publication::{
    CatalogProjection, StructureBuildRequest, StructurePublicationAttempt,
};

use super::{map_publication_error, phase::LvmAttempt};

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
    if structure_publication::recover_prepared_publication(&publication)
        .map_err(map_publication_error)?
    {
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
        Ok(digest) => structure_publication::publish_build(&publication, &digest)
            .map_err(map_publication_error)?,
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
        projection: CatalogProjection::Partition,
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
    structure_publication::ensure_not_cancelled(request.cancel).map_err(map_publication_error)?;
    drop(open_lvm_volume_reader(
        request.plan,
        &AtomicBool::new(false),
    )?);
    request
        .attempt
        .refresh(request.case_conn, &request.source.id, request.fingerprint)
        .map_err(map_publication_error)?;
    let digest = structure_publication::seal_catalog(publication, source_conn)
        .map_err(map_publication_error)?;
    source_db::finalize_source_build_db(source_conn)?;
    Ok(digest)
}

fn discard_build(request: &CatalogBuild<'_>) -> Result<(), LvmPlanError> {
    structure_publication::discard_unpublished_catalog_build(
        request.case_conn,
        request.case_root,
        &request.source.id,
        request.attempt.attempt_id(),
    )
    .map_err(map_publication_error)
}
