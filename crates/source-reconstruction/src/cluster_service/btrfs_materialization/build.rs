use evidence_access::{
    btrfs_pool::{self, BtrfsPoolPlan, BtrfsSourceError},
    file_service,
};
use persistence_source::publication::catalog_publication::{
    self as publication, StructureBuildRequest,
};
use std::sync::Arc;

pub(super) fn execute(
    request: &StructureBuildRequest<'_>,
    plan: &BtrfsPoolPlan,
) -> Result<(), BtrfsSourceError> {
    if publication::recover_prepared_publication(request)? {
        return Ok(());
    }
    let source = source_kernel::source_db::open_fresh_source_build_db(
        request.case_root,
        &request.source.id,
        request.attempt.attempt_id(),
    )?;
    let result = populate(request, plan, &source);
    drop(source);
    match result {
        Ok(digest) => Ok(publication::publish_build(request, &digest)?),
        Err(error) => {
            publication::discard_unpublished_catalog_build(
                request.case_conn,
                request.case_root,
                &request.source.id,
                request.attempt.attempt_id(),
            )?;
            Err(error)
        }
    }
}

fn populate(
    request: &StructureBuildRequest<'_>,
    plan: &BtrfsPoolPlan,
    source: &rusqlite::Connection,
) -> Result<String, BtrfsSourceError> {
    let workers = source_kernel::resource_budget::default_cpu_budget().max(4);
    let _budget = source_kernel::resource_budget::global_pool()
        .acquire(
            source_kernel::resource_budget::Resources {
                bytes: source_kernel::resource_budget::ENUMERATION_BYTES
                    + plan.members.len() as u64 * 2 * 1024 * 1024,
                readers: plan.members.len() as u64,
                workers: workers as u64,
                catalogs: 1,
                ..Default::default()
            },
            Some(request.cancel),
        )
        .map_err(|error| std::io::Error::other(error.to_string()))?;
    persistence_sqlite::repositories::datasource_repo::DataSourceRepo::new(source)
        .upsert_source_local_metadata(request.case_id, request.source)?;
    let root = file_service::insert_partition_placeholder_root(
        source,
        &request.source.id,
        0,
        "Btrfs",
        "pending",
    )?;
    let filesystem = Arc::new(btrfs_pool::open_filesystem(plan, request.cancel)?);
    let factory = Arc::new(move || {
        Ok(Box::new(filesystem.as_ref().clone()) as Box<dyn evidence_core::FileSystemReader>)
    });
    let stats = file_service::enumerate_partition_parallel_with_factory(
        source,
        &root,
        workers,
        factory,
        "Btrfs",
        None,
        Some(request.cancel),
    )?;
    if stats.incomplete_catalog_diagnostic_count() != 0 {
        return Err(BtrfsSourceError::Identity);
    }
    publication::ensure_not_cancelled(request.cancel)?;
    let digest = publication::seal_catalog(request, source)?;
    source_kernel::source_db::finalize_source_build_db(source)?;
    Ok(digest)
}
