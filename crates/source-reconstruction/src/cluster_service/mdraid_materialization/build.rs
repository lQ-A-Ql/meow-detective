use std::{path::Path, sync::atomic::AtomicBool};

use domain::{CaseId, DataSource};
use evidence_access::mdraid::{open_mdraid_reader, MdRaidArrayPlan, MdRaidPlanError};
use persistence_sqlite::repositories::datasource_repo::DataSourceRepo;
use source_kernel::source_db;

use super::{catalog, ensure_not_cancelled, phase::CatalogAttempt, publication};

pub(super) struct CatalogBuild<'a> {
    pub case_conn: &'a rusqlite::Connection,
    pub case_root: &'a Path,
    pub case_id: &'a CaseId,
    pub source: &'a DataSource,
    pub plan: &'a MdRaidArrayPlan,
    pub fingerprint: &'a str,
    pub attempt: &'a CatalogAttempt,
    pub cancel: &'a AtomicBool,
}

pub(super) fn build_and_publish(request: CatalogBuild<'_>) -> Result<(), MdRaidPlanError> {
    ensure_not_cancelled(request.cancel)?;
    if publication::recover_prepared_publication(&request)? {
        return Ok(());
    }
    let source_conn = source_db::open_fresh_source_build_db(
        request.case_root,
        &request.source.id,
        &request.attempt.id,
    )?;
    let result = populate_and_seal(&request, &source_conn);
    drop(source_conn);
    match result {
        Ok(digest) => publication::publish_build(&request, &digest),
        Err(error) => {
            discard_build(&request)?;
            Err(error)
        }
    }
}

fn populate_and_seal(
    request: &CatalogBuild<'_>,
    source_conn: &rusqlite::Connection,
) -> Result<String, MdRaidPlanError> {
    DataSourceRepo::new(source_conn)
        .upsert_source_local_metadata(request.case_id, request.source)?;
    catalog::enumerate_mdraid_catalog(
        request.case_root,
        request.source,
        source_conn,
        request.cancel,
    )?;
    ensure_not_cancelled(request.cancel)?;
    drop(open_mdraid_reader(request.plan)?);
    request
        .attempt
        .refresh(request.case_conn, &request.source.id, request.fingerprint)?;
    let digest = publication::seal_catalog(request, source_conn)?;
    source_db::finalize_source_build_db(source_conn)?;
    Ok(digest)
}

fn discard_build(request: &CatalogBuild<'_>) -> Result<(), MdRaidPlanError> {
    persistence_source::publication::catalog_publication::discard_unpublished_catalog_build(
        request.case_conn,
        request.case_root,
        &request.source.id,
        &request.attempt.id,
    )
    .map_err(publication::map_publication_error)
}
