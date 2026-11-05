use persistence_fjall::{encode_key, KeyPart, PersistMode, VersionedEnvelope};
use rusqlite::Connection;
use std::time::Instant;

use super::context::ImportJobContext;
use transport::CommandError;

mod cephfs;
mod nested;
mod output_changes;
mod projection_state;
mod projections;
mod records;
use cephfs::mirror_namespace_locators;
use nested::mirror_nested_lineage;
use projection_state::{
    analysis_projection_marker, analysis_projection_revision, catalog_projection_is_published,
    ensure_analysis_projection_building, ensure_projection_building,
    write_analysis_projection_state, AnalysisProjectionRevision,
};

pub(super) fn mirror_source_records(
    ctx: &ImportJobContext<'_>,
    source_id: &str,
) -> Result<String, CommandError> {
    ctx.report_job_progress(96, "Projecting queryable records...")?;
    let layout = persistence_fjall::SourceBuildLayout::for_case_source(
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
    ctx.report_job_progress(97, "Queryable record projection completed")?;
    Ok(projection_digest)
}

pub(crate) fn mirror_partition_data_for_import(
    ctx: &ImportJobContext<'_>,
    source_id: &str,
    partitions: &[crate::datasource_service::PartitionRecord],
    gpt_disk_guid: Option<&str>,
) -> Result<(), CommandError> {
    let layout = persistence_fjall::SourceBuildLayout::for_case_source(
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
    let source = crate::storage_runtime::open_source_read_write_at_path(&target_root)
        .map_err(|error| CommandError::internal(error.to_string()))?;
    if target_root == layout.published_path() {
        ensure_projection_building(&source, source_id, "projection")?;
    }
    crate::partition_storage_service::replace_from_probe(
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

fn publish_source_generation(
    layout: &persistence_fjall::SourceBuildLayout,
    target_root: &std::path::Path,
    existing_generation: bool,
    source_id: &str,
    attempt_id: &str,
    projection_digest: &str,
) -> Result<(), CommandError> {
    let _close_guard = crate::storage_runtime::close_source_store_at_path(target_root)
        .map_err(|error| CommandError::internal(error.to_string()))?
        .ok_or_else(|| CommandError::internal("Fjall source generation is still in use"))?;
    let source = persistence_fjall::SourceStore::open(
        target_root,
        persistence_fjall::DatabaseConfig::default(),
    )
    .map_err(|error| CommandError::internal(error.to_string()))?;
    if existing_generation {
        persistence_fjall::replace_source_generation(
            &source,
            source_id,
            attempt_id,
            projection_digest,
        )
    } else {
        persistence_fjall::seal_source_generation(&source, source_id, attempt_id, projection_digest)
    }
    .map_err(|error| CommandError::internal(error.to_string()))?;
    source
        .database()
        .persist(PersistMode::SyncAll)
        .map_err(|error| CommandError::internal(error.to_string()))?;
    drop(source);
    if !existing_generation {
        persistence_fjall::publish_source_generation(layout.build_path(), layout.published_path())
            .map_err(|error| CommandError::internal(error.to_string()))?;
    }
    drop(_close_guard);
    Ok(())
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
            let layout = persistence_fjall::SourceBuildLayout::for_case_source(
                case_root, source_id, attempt_id,
            )
            .map_err(|error| CommandError::internal(error.to_string()))?;
            publish_analysis_generation(
                &layout,
                root,
                false,
                source_id,
                attempt_id,
                &projection_digest,
            )?;
            if layout.published_path().is_dir() && layout.build_path().is_dir() {
                std::fs::remove_dir_all(layout.build_path())
                    .map_err(|error| CommandError::internal(error.to_string()))?;
            }
        }
    }
    Ok(projection_digest)
}

fn publish_analysis_generation(
    layout: &persistence_fjall::SourceBuildLayout,
    target_root: &std::path::Path,
    _existing_generation: bool,
    source_id: &str,
    attempt_id: &str,
    projection_digest: &str,
) -> Result<(), CommandError> {
    let _close_guard = crate::storage_runtime::close_source_store_at_path(target_root)
        .map_err(|error| CommandError::internal(error.to_string()))?
        .ok_or_else(|| CommandError::internal("Fjall analysis generation is still in use"))?;
    let source = persistence_fjall::SourceStore::open(
        target_root,
        persistence_fjall::DatabaseConfig::default(),
    )
    .map_err(|error| CommandError::internal(error.to_string()))?;
    persistence_fjall::replace_source_generation(&source, source_id, attempt_id, projection_digest)
        .map_err(|error| CommandError::internal(error.to_string()))?;
    source
        .database()
        .persist(PersistMode::SyncAll)
        .map_err(|error| CommandError::internal(error.to_string()))?;
    drop(source);
    if !layout.published_path().is_dir() {
        persistence_fjall::publish_source_generation(target_root, layout.published_path())
            .map_err(|error| CommandError::internal(error.to_string()))?;
    }
    drop(_close_guard);
    Ok(())
}

fn active_import_generation_root(
    case_root: &std::path::Path,
    case_id: &str,
    source_id: &str,
) -> Result<Option<std::path::PathBuf>, CommandError> {
    let case_connection = crate::connection::open_existing_case_db(&case_root.join("app.db"))
        .map_err(|error| CommandError::internal(error.to_string()))?;
    let operations = persistence_sqlite::repositories::storage_control_repo::StorageControlRepo::new(
        &case_connection,
    )
    .list_open_operations(case_id)
    .map_err(|error| CommandError::internal(error.to_string()))?
    .into_iter()
    .filter(|operation| {
        operation.source_id.as_deref() == Some(source_id)
            && operation.operation_kind == "source_import"
            && matches!(
                operation.state,
                persistence_sqlite::repositories::storage_control_repo::StorageOperationState::Planned
                    | persistence_sqlite::repositories::storage_control_repo::StorageOperationState::Running
            )
    })
    .collect::<Vec<_>>();
    if operations.len() > 1 {
        return Err(CommandError::internal(
            "multiple source import attempts are active",
        ));
    }
    let Some(operation) = operations.into_iter().next() else {
        return Ok(None);
    };
    persistence_fjall::SourceBuildLayout::for_case_source(
        case_root,
        source_id,
        &operation.attempt_id,
    )
    .map(|layout| Some(layout.build_path()))
    .map_err(|error| CommandError::internal(error.to_string()))
}

pub(crate) fn mirror_catalog_records_from_connection(
    connection: &Connection,
    source: &crate::storage_runtime::ReadWriteSourceStorageHandle,
    source_id: &str,
) -> Result<(), CommandError> {
    projections::mirror_catalog(connection, source, source_id)
}

pub(crate) fn mirror_analysis_records_before_sqlite_commit(
    connection: &Connection,
    source: &crate::storage_runtime::ReadWriteSourceStorageHandle,
    source_id: &str,
) -> Result<(), CommandError> {
    projections::mirror_timeline(connection, source, source_id)?;
    projections::mirror_artifact_records_only(connection, source, source_id)
}

pub(crate) fn mirror_analysis_outputs_before_sqlite_commit(
    case_connection: &Connection,
    case_root: &std::path::Path,
    case_id: &str,
    source_connection: &rusqlite::Transaction<'_>,
    source_id: &str,
) -> Result<(), CommandError> {
    let source_root = active_import_generation_root(case_root, case_id, source_id)?
        .or_else(|| crate::source_db::fjall_source_storage_root(case_root, source_id).ok())
        .ok_or_else(|| CommandError::internal("Fjall source generation is unavailable"))?;
    let source = crate::storage_runtime::open_source_read_write_at_path(&source_root)
        .map_err(|error| CommandError::internal(error.to_string()))?;
    let analysis_revision = analysis_projection_revision(source_connection, source_id)?;
    let previous_analysis_revision = analysis_projection_marker(&source, source_id)?;
    let counts_ready = source
        .timeline_counts_ready(&source.snapshot(), source_id)
        .map_err(|error| CommandError::internal(error.to_string()))?;
    ensure_analysis_projection_building(&source, source_id)?;
    apply_projection_records(
        case_connection,
        case_root,
        case_id,
        source_connection,
        source_id,
        &source,
        false,
        true,
        false,
        counts_ready,
        &analysis_revision,
        previous_analysis_revision.as_ref(),
    )?;
    source
        .persist(PersistMode::SyncAll)
        .map_err(|error| CommandError::internal(error.to_string()))?;
    publish_projection_marker(&source, source_id, false, Some(&analysis_revision))?;
    prune_analysis_projection_changes(source_connection, analysis_revision.change_high_water)
}

pub(crate) fn mark_catalog_projection_seeded(
    source: &crate::storage_runtime::ReadWriteSourceStorageHandle,
    source_id: &str,
) -> Result<(), CommandError> {
    let key = encode_key("projection", &[KeyPart::Text(source_id.to_owned())])
        .map_err(|error| CommandError::internal(error.to_string()))?;
    let marker = VersionedEnvelope::new(
        "catalog_projection_seeded",
        1,
        format!("projection:{source_id}"),
        source_id,
        1,
        source_id.as_bytes().to_vec(),
    );
    source
        .write_envelope(
            persistence_fjall::KeyspaceName::Meta,
            key,
            &marker,
            PersistMode::SyncAll,
        )
        .map_err(|error| CommandError::internal(error.to_string()))?;
    Ok(())
}

pub(crate) fn begin_analysis_output_projection(
    source_connection: &Connection,
) -> Result<(), CommandError> {
    let database_path: String = source_connection
        .query_row("PRAGMA database_list", [], |row| row.get(2))
        .map_err(|error| CommandError::internal(error.to_string()))?;
    let Some(case_root) = std::path::Path::new(&database_path)
        .parent()
        .and_then(std::path::Path::parent)
        .and_then(std::path::Path::parent)
    else {
        return Ok(());
    };
    let mut statement = source_connection
        .prepare("SELECT id FROM data_sources ORDER BY id LIMIT 2")
        .map_err(|error| CommandError::internal(error.to_string()))?;
    let source_ids = statement
        .query_map([], |row| row.get::<_, String>(0))
        .map_err(|error| CommandError::internal(error.to_string()))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| CommandError::internal(error.to_string()))?;
    let source_id = match source_ids.as_slice() {
        [source_id] => source_id.clone(),
        [] => return Err(CommandError::internal("Source database has no data source")),
        _ => {
            return Err(CommandError::internal(
                "Source database contains multiple data sources",
            ))
        }
    };
    let root = crate::source_db::fjall_source_storage_root(case_root, &source_id)
        .map_err(|error| CommandError::internal(error.to_string()))?;
    if !root.is_dir() {
        return Ok(());
    }
    let source = crate::source_db::open_registered_fjall_source_read_write(
        case_root,
        &domain::DataSourceId(source_id.clone()),
    )
    .map_err(|error| CommandError::internal(error.to_string()))?;
    ensure_analysis_projection_building(&source, &source_id)
}

pub(crate) fn begin_partition_projection(
    case_root: &std::path::Path,
    source_id: &domain::DataSourceId,
) -> Result<(), CommandError> {
    let root = crate::source_db::fjall_source_storage_root(case_root, &source_id.0)
        .map_err(|error| CommandError::internal(error.to_string()))?;
    if !root.is_dir() {
        return Ok(());
    }
    let source = crate::source_db::open_registered_fjall_source_read_write(case_root, source_id)
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
    persistence_sqlite::repositories::source_projection_change_repo::SourceProjectionChangeRepo::new(
        source_connection,
    )
    .acquire_projection_writer_barrier()
    .map_err(|error| CommandError::internal(error.to_string()))?;
    let source = match source_root {
        Some(source_root) => crate::storage_runtime::open_source_read_write_at_path(source_root),
        None => crate::source_db::open_registered_fjall_source_read_write(
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
                write_analysis_projection_state(
                    &source,
                    source_id,
                    "analysis_projection_complete",
                )?;
                prune_analysis_projection_changes(source_connection, revision.change_high_water)?;
                let digest = encode_key(
                    "analysis_projection_revision",
                    &[
                        KeyPart::Text(source_id.to_owned()),
                        KeyPart::U64(revision.artifact_revision),
                        KeyPart::U64(revision.artifact_high_water.max(0) as u64),
                        KeyPart::U64(revision.timeline_revision),
                        KeyPart::U64(revision.timeline_high_water.max(0) as u64),
                        KeyPart::U64(revision.change_high_water),
                    ],
                )
                .map_err(|error| CommandError::internal(error.to_string()))?;
                sqlite_projection_transaction
                    .commit()
                    .map_err(|error| CommandError::internal(error.to_string()))?;
                return Ok(hex::encode(persistence_fjall::digest(&digest)));
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
        counts_ready,
        analysis_revision
            .as_ref()
            .expect("analysis revision exists"),
        previous_analysis_revision.as_ref(),
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
    if let Some(revision) = analysis_revision {
        prune_analysis_projection_changes(source_connection, revision.change_high_water)?;
    }
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
    source: &crate::source_db::FjallReadWriteSourceStorageHandle,
    include_catalog: bool,
    include_analysis: bool,
    catalog_published: bool,
    counts_ready: bool,
    analysis_revision: &AnalysisProjectionRevision,
    previous_analysis_revision: Option<&AnalysisProjectionRevision>,
) -> Result<(), CommandError> {
    let incremental_analysis = !include_catalog
        && counts_ready
        && previous_analysis_revision.is_some_and(|revision| revision.change_log_initialized);
    if incremental_analysis {
        let checkpoint =
            previous_analysis_revision.map_or(0, |revision| revision.change_high_water);
        output_changes::mirror_analysis_output_changes(
            case_connection,
            case_root,
            case_id,
            source_connection,
            source,
            source_id,
            checkpoint,
            analysis_revision.change_high_water,
        )?;
    } else {
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
    }
    Ok(())
}

fn prune_analysis_projection_changes(
    source_connection: &Connection,
    through_revision: u64,
) -> Result<(), CommandError> {
    persistence_sqlite::repositories::source_projection_change_repo::SourceProjectionChangeRepo::new(
        source_connection,
    )
    .prune_through(through_revision)
    .map_err(|error| CommandError::internal(error.to_string()))?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn mirror_projection_records(
    case_connection: &Connection,
    case_root: &std::path::Path,
    case_id: &str,
    source_connection: &Connection,
    source_id: &str,
    source: &crate::source_db::FjallReadWriteSourceStorageHandle,
    include_catalog: bool,
    include_analysis: bool,
    catalog_published: bool,
) -> Result<(), CommandError> {
    if include_catalog {
        ensure_projection_building(source, source_id, "projection")?;
    }
    if include_catalog && !catalog_published {
        projections::mirror_catalog(source_connection, source, source_id)?;
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

fn publish_projection_marker(
    source: &crate::source_db::FjallReadWriteSourceStorageHandle,
    source_id: &str,
    include_catalog: bool,
    analysis_revision: Option<&AnalysisProjectionRevision>,
) -> Result<String, CommandError> {
    let (marker_family, marker_kind) = if include_catalog {
        ("projection", "source_projection_complete")
    } else {
        ("analysis_projection", "analysis_projection_complete")
    };
    let marker = if let Some(revision) = analysis_revision {
        VersionedEnvelope::from_serializable(
            marker_kind,
            1,
            format!("projection:{source_id}"),
            source_id,
            1,
            revision,
        )
        .map_err(|error| CommandError::internal(error.to_string()))?
    } else {
        VersionedEnvelope::new(
            marker_kind,
            1,
            format!("projection:{source_id}"),
            source_id,
            1,
            source_id.as_bytes().to_vec(),
        )
    };
    let receipt = source
        .write_envelope(
            persistence_fjall::KeyspaceName::Meta,
            encode_key(marker_family, &[KeyPart::Text(source_id.to_owned())])
                .map_err(|error| CommandError::internal(error.to_string()))?,
            &marker,
            PersistMode::SyncAll,
        )
        .map_err(|error| CommandError::internal(error.to_string()))?;
    let mut batch_digest = receipt.batch_digest;
    if include_catalog {
        let revision = analysis_revision
            .ok_or_else(|| CommandError::internal("Import projection revision is missing"))?;
        let analysis_marker = VersionedEnvelope::from_serializable(
            "analysis_projection_complete",
            1,
            format!("projection:{source_id}"),
            source_id,
            1,
            revision,
        )
        .map_err(|error| CommandError::internal(error.to_string()))?;
        let analysis_receipt = source
            .write_envelope(
                persistence_fjall::KeyspaceName::Meta,
                encode_key(
                    "analysis_projection",
                    &[KeyPart::Text(source_id.to_owned())],
                )
                .map_err(|error| CommandError::internal(error.to_string()))?,
                &analysis_marker,
                PersistMode::SyncAll,
            )
            .map_err(|error| CommandError::internal(error.to_string()))?;
        batch_digest = analysis_receipt.batch_digest;
    }
    if analysis_revision.is_some() {
        write_analysis_projection_state(source, source_id, "analysis_projection_complete")?;
    }
    Ok(hex::encode(batch_digest))
}
