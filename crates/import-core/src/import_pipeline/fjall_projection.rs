use persistence_source::fjall::PersistMode;
use rusqlite::Connection;
use std::time::Instant;

use super::context::ImportJobContext;
use transport::CommandError;

mod cephfs;
mod generation;
mod nested;
mod projection_state;
mod projections;
mod records;
use cephfs::mirror_namespace_locators;
use generation::{
    active_root as active_import_generation_root, publish as publish_source_generation,
};
use nested::mirror_nested_lineage;
use projection_state::{
    analysis_projection_marker, analysis_projection_revision, catalog_projection_is_published,
    ensure_analysis_projection_building, ensure_projection_building, publish_projection_marker,
};

pub(super) fn mirror_source_records(
    ctx: &ImportJobContext<'_>,
    source_id: &str,
) -> Result<String, CommandError> {
    let target = match ctx.content_kind {
        super::context::ImportContentKind::Filesystem => {
            persistence_source::control::import_state::ImportedSourceState::Ready
        }
        super::context::ImportContentKind::CephBlueStoreMetadata => {
            persistence_source::control::import_state::ImportedSourceState::ReadyMetadata
        }
    };
    persistence_source::control::import_state::record_import_target(
        ctx.source_connection()?,
        target,
    )
    .map_err(CommandError::from_service_error)?;
    ctx.report_job_progress(96, "Projecting queryable records...")?;
    let layout = persistence_source::fjall::SourceBuildLayout::for_case_source(
        ctx.case_root,
        source_id,
        &ctx.job_id.0,
    )
    .map_err(|error| CommandError::internal(error.to_string()))?;
    let existing_generation = layout.published_path().is_dir();
    let target_root = if existing_generation {
        layout.published_path()
    } else {
        layout.build_path()
    };
    std::fs::create_dir_all(&target_root)
        .map_err(|error| CommandError::internal(error.to_string()))?;
    let result = mirror_source_records_into_root(
        ctx.conn,
        ctx.case_root,
        &ctx.case_id.0,
        ctx.source_connection()?,
        source_id,
        &target_root,
    );
    let projection_digest = result?;
    publish_source_generation(
        &layout,
        &target_root,
        existing_generation,
        source_id,
        &ctx.job_id.0,
        &projection_digest,
    )?;
    persistence_source::detail::bind_generation(ctx.source_connection()?, "source.fjall")
        .map_err(CommandError::from_service_error)?;
    ctx.report_job_progress(97, "Queryable record projection completed")?;
    Ok(projection_digest)
}

pub(crate) fn mirror_partition_data_for_import(
    ctx: &ImportJobContext<'_>,
    source_id: &str,
    partitions: &[evidence_access::datasource_service::PartitionRecord],
    gpt_disk_guid: Option<&str>,
) -> Result<(), CommandError> {
    let layout = persistence_source::fjall::SourceBuildLayout::for_case_source(
        ctx.case_root,
        source_id,
        &ctx.job_id.0,
    )
    .map_err(|error| CommandError::internal(error.to_string()))?;
    let target_root = if layout.published_path().is_dir() {
        layout.published_path()
    } else {
        layout.build_path()
    };
    std::fs::create_dir_all(&target_root)
        .map_err(|error| CommandError::internal(error.to_string()))?;
    let source = source_kernel::storage_runtime::open_source_read_write_at_path(&target_root)
        .map_err(|error| CommandError::internal(error.to_string()))?;
    if target_root == layout.published_path() {
        ensure_projection_building(&source, source_id, "projection")?;
    }
    evidence_access::partition_storage_service::replace_from_probe(
        &source,
        &domain::DataSourceId(source_id.to_owned()),
        partitions,
        gpt_disk_guid,
    )
    .map_err(|error| CommandError::internal(error.to_string()))?;
    source
        .persist(PersistMode::SyncAll)
        .map_err(|error| CommandError::internal(error.to_string()))
}

fn mirror_source_records_into_root(
    case_connection: &Connection,
    case_root: &std::path::Path,
    case_id: &str,
    source_connection: &Connection,
    source_id: &str,
    source_root: &std::path::Path,
) -> Result<String, CommandError> {
    mirror_source_records_from_connections_internal(
        case_connection,
        case_root,
        case_id,
        source_connection,
        source_id,
        true,
        false,
        Some(source_root),
    )
}

pub(crate) fn mirror_analysis_outputs_from_connections(
    case_connection: &Connection,
    case_root: &std::path::Path,
    case_id: &str,
    source_connection: &Connection,
    source_id: &str,
) -> Result<String, CommandError> {
    let source_root = active_import_generation_root(case_root, case_id, source_id)?;
    let projection_digest = mirror_source_records_from_connections_internal(
        case_connection,
        case_root,
        case_id,
        source_connection,
        source_id,
        false,
        true,
        source_root.as_deref(),
    )?;
    if let Some(root) = source_root.as_deref() {
        if let Some(attempt_id) = root
            .file_name()
            .and_then(|name| name.to_str())
            .and_then(|name| name.strip_prefix("source.fjall.build."))
        {
            let layout = persistence_source::fjall::SourceBuildLayout::for_case_source(
                case_root, source_id, attempt_id,
            )
            .map_err(|error| CommandError::internal(error.to_string()))?;
            publish_source_generation(
                &layout,
                root,
                false,
                source_id,
                attempt_id,
                &projection_digest,
            )?;
            persistence_source::detail::bind_generation(source_connection, "source.fjall")
                .map_err(CommandError::from_service_error)?;
        }
    }
    generation::publication_receipt(source_connection, source_id)
}

pub(crate) fn mirror_catalog_records_from_connection(
    connection: &Connection,
    source: &source_kernel::storage_runtime::ReadWriteSourceStorageHandle,
    source_id: &str,
) -> Result<(), CommandError> {
    ::evidence_access::catalog_projection::mirror_catalog_records_from_connection(
        connection, source, source_id,
    )
}

pub(crate) fn mirror_analysis_records_before_sqlite_commit(
    connection: &Connection,
    source: &source_kernel::storage_runtime::ReadWriteSourceStorageHandle,
    source_id: &str,
) -> Result<(), CommandError> {
    projections::mirror_timeline(connection, source, source_id)?;
    projections::mirror_artifact_records_only(connection, source, source_id)
}

pub(crate) fn mark_catalog_projection_seeded(
    source: &source_kernel::storage_runtime::ReadWriteSourceStorageHandle,
    source_id: &str,
) -> Result<(), CommandError> {
    ::evidence_access::catalog_projection::mark_catalog_projection_seeded(source, source_id)
}

pub fn begin_partition_projection(
    case_root: &std::path::Path,
    source_id: &domain::DataSourceId,
) -> Result<(), CommandError> {
    let control = persistence_source::control::opening::open_source_database_read_only(
        &source_kernel::source_db::source_db_path(case_root, source_id),
    )
    .map_err(CommandError::from_service_error)?;
    let root = persistence_source::control::bound_source_generation_root(&control, source_id)
        .map_err(CommandError::from_service_error)?;
    if !root.is_dir() {
        return Ok(());
    }
    let source = source_kernel::source_db::open_fjall_source_for_build(&control, source_id)
        .map_err(|error| CommandError::internal(error.to_string()))?;
    ensure_projection_building(&source, &source_id.0, "projection")
}

#[allow(clippy::too_many_arguments)]
fn mirror_source_records_from_connections_internal(
    case_connection: &Connection,
    case_root: &std::path::Path,
    case_id: &str,
    source_connection: &Connection,
    source_id: &str,
    include_catalog: bool,
    include_analysis: bool,
    source_root: Option<&std::path::Path>,
) -> Result<String, CommandError> {
    let started = Instant::now();
    let sqlite_projection_transaction = source_connection
        .unchecked_transaction()
        .map_err(|error| CommandError::internal(error.to_string()))?;
    let source_connection = &sqlite_projection_transaction;
    let source = match source_root {
        Some(source_root) => {
            source_kernel::storage_runtime::open_source_read_write_at_path(source_root)
        }
        None => source_kernel::source_db::open_registered_fjall_source_read_write(
            case_root,
            &domain::DataSourceId(source_id.to_owned()),
        ),
    }
    .map_err(|_| CommandError::internal("Fjall source storage is unavailable"))?;
    tracing::info!(source_id, "Fjall source projection started");
    let analysis_revision = Some(analysis_projection_revision(source_connection, source_id)?);
    let previous_analysis_revision = analysis_projection_marker(&source, source_id)?;
    let counts_ready = source
        .timeline_counts_ready(&source.snapshot(), source_id)
        .map_err(|error| CommandError::internal(error.to_string()))?;
    if !include_catalog && counts_ready {
        if let Some(revision) = analysis_revision.as_ref() {
            if previous_analysis_revision.as_ref() == Some(revision) {
                let digest = publish_projection_marker(&source, source_id, false, Some(revision))?;
                sqlite_projection_transaction
                    .commit()
                    .map_err(|error| CommandError::internal(error.to_string()))?;
                return Ok(digest);
            }
        }
    }
    let catalog_published = include_catalog && catalog_projection_is_published(&source, source_id)?;
    ensure_analysis_projection_building(&source, source_id)?;
    apply_projection_records(
        case_connection,
        case_root,
        case_id,
        source_connection,
        source_id,
        &source,
        include_catalog,
        include_analysis,
        catalog_published,
    )?;
    source
        .persist(PersistMode::SyncAll)
        .map_err(|error| CommandError::internal(error.to_string()))?;
    tracing::info!(
        source_id,
        elapsed_ms = started.elapsed().as_millis(),
        "Fjall source projection committed"
    );
    let projection_digest = publish_projection_marker(
        &source,
        source_id,
        include_catalog,
        analysis_revision.as_ref(),
    )?;
    sqlite_projection_transaction
        .commit()
        .map_err(|error| CommandError::internal(error.to_string()))?;
    Ok(projection_digest)
}

#[allow(clippy::too_many_arguments)]
fn apply_projection_records(
    case_connection: &Connection,
    case_root: &std::path::Path,
    case_id: &str,
    source_connection: &Connection,
    source_id: &str,
    source: &source_kernel::source_db::FjallReadWriteSourceStorageHandle,
    include_catalog: bool,
    include_analysis: bool,
    catalog_published: bool,
) -> Result<(), CommandError> {
    mirror_projection_records(
        case_connection,
        case_root,
        case_id,
        source_connection,
        source_id,
        source,
        include_catalog,
        include_analysis,
        catalog_published,
    )?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn mirror_projection_records(
    case_connection: &Connection,
    case_root: &std::path::Path,
    case_id: &str,
    source_connection: &Connection,
    source_id: &str,
    source: &source_kernel::source_db::FjallReadWriteSourceStorageHandle,
    include_catalog: bool,
    include_analysis: bool,
    catalog_published: bool,
) -> Result<(), CommandError> {
    if include_catalog {
        ensure_projection_building(source, source_id, "projection")?;
    }
    if include_catalog && !catalog_published {
        ::evidence_access::catalog_projection::mirror_catalog_records_from_connection(
            source_connection,
            source,
            source_id,
        )?;
    } else {
        tracing::debug!(source_id, "Skipping Fjall catalog mirror");
    }
    if include_analysis {
        projections::mirror_timeline(source_connection, source, source_id)?;
        projections::mirror_artifacts(
            case_connection,
            case_root,
            case_id,
            source_connection,
            source,
            source_id,
        )?;
        records::mirror_android_parser_records(source_connection, source, source_id)?;
        records::mirror_linux_parser_record(source_connection, source, source_id)?;
        records::mirror_parser_tables(source_connection, source, source_id)?;
    }
    if include_catalog {
        mirror_namespace_locators(source_connection, source, source_id)?;
        mirror_nested_lineage(case_connection, source, source_id)?;
        source
            .partition_projection_publish(source_id)
            .map_err(|error| CommandError::internal(error.to_string()))?;
    }
    Ok(())
}

#[cfg(test)]
#[path = "../../tests/unit/import_pipeline/fjall_projection.rs"]
mod tests;
