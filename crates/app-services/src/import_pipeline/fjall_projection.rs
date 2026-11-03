use persistence_fjall::{encode_key, KeyPart, PersistMode, VersionedEnvelope};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::time::Instant;

use super::context::ImportJobContext;
use transport::CommandError;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct AnalysisProjectionRevision {
    source_id: String,
    artifact_revision: u64,
    artifact_high_water: i64,
    timeline_revision: u64,
    timeline_high_water: i64,
}

mod cephfs;
mod nested;
mod projections;
mod records;
use cephfs::mirror_namespace_locators;
use nested::mirror_nested_lineage;

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
    mirror_source_records_from_connections_internal(
        case_connection,
        case_root,
        case_id,
        source_connection,
        source_id,
        false,
        None,
    )
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

fn mirror_source_records_from_connections_internal(
    case_connection: &Connection,
    case_root: &std::path::Path,
    case_id: &str,
    source_connection: &Connection,
    source_id: &str,
    include_catalog: bool,
    source_root: Option<&std::path::Path>,
) -> Result<String, CommandError> {
    let started = Instant::now();
    let source = match source_root {
        Some(source_root) => crate::storage_runtime::open_source_read_write_at_path(source_root),
        None => crate::source_db::open_registered_fjall_source_read_write(
            case_root,
            &domain::DataSourceId(source_id.to_owned()),
        ),
    }
    .map_err(|_| CommandError::internal("Fjall source storage is unavailable"))?;
    tracing::info!(source_id, "Fjall source projection started");
    let analysis_revision = if include_catalog {
        None
    } else {
        Some(analysis_projection_revision(source_connection, source_id)?)
    };
    if let Some(revision) = analysis_revision.as_ref() {
        if analysis_projection_marker(&source, source_id)?.as_ref() == Some(revision) {
            write_analysis_projection_state(&source, source_id, "analysis_projection_complete")?;
            let digest = encode_key(
                "analysis_projection_revision",
                &[
                    KeyPart::Text(source_id.to_owned()),
                    KeyPart::U64(revision.artifact_revision),
                    KeyPart::U64(revision.artifact_high_water.max(0) as u64),
                    KeyPart::U64(revision.timeline_revision),
                    KeyPart::U64(revision.timeline_high_water.max(0) as u64),
                ],
            )
            .map_err(|error| CommandError::internal(error.to_string()))?;
            return Ok(hex::encode(persistence_fjall::digest(&digest)));
        }
    }
    let catalog_published = include_catalog && catalog_projection_is_published(&source, source_id)?;
    if !include_catalog {
        ensure_analysis_projection_building(&source, source_id)?;
    }
    mirror_projection_records(
        case_connection,
        case_root,
        case_id,
        source_connection,
        source_id,
        &source,
        include_catalog,
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
    publish_projection_marker(
        &source,
        source_id,
        include_catalog,
        analysis_revision.as_ref(),
    )
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
    projections::mirror_timeline(source_connection, source, source_id)?;
    projections::mirror_artifacts(
        case_connection,
        case_root,
        case_id,
        source_connection,
        source,
        source_id,
    )?;
    if include_catalog {
        records::mirror_android_parser_records(source_connection, source, source_id)?;
        records::mirror_linux_parser_record(source_connection, source, source_id)?;
        records::mirror_parser_tables(source_connection, source, source_id)?;
        mirror_namespace_locators(source_connection, source, source_id)?;
        mirror_nested_lineage(case_connection, source, source_id)?;
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
    if analysis_revision.is_some() {
        write_analysis_projection_state(source, source_id, "analysis_projection_complete")?;
    }
    Ok(hex::encode(receipt.batch_digest))
}

fn analysis_projection_revision(
    source_connection: &Connection,
    source_id: &str,
) -> Result<AnalysisProjectionRevision, CommandError> {
    let artifact_repo =
        persistence_sqlite::repositories::artifact_repo::ArtifactRepo::new(source_connection);
    let artifact_revision = artifact_repo
        .cursor_revision()
        .map_err(|error| CommandError::internal(error.to_string()))?;
    let artifact_high_water = artifact_repo
        .snapshot_high_water()
        .map_err(|error| CommandError::internal(error.to_string()))?;
    let timeline_repo =
        persistence_sqlite::repositories::timeline_repo::TimelineRepo::new(source_connection);
    let timeline_revision = timeline_repo
        .cursor_revision()
        .map_err(|error| CommandError::internal(error.to_string()))?;
    let timeline_high_water = timeline_repo
        .snapshot_high_water()
        .map_err(|error| CommandError::internal(error.to_string()))?;
    Ok(AnalysisProjectionRevision {
        source_id: source_id.to_owned(),
        artifact_revision,
        artifact_high_water,
        timeline_revision,
        timeline_high_water,
    })
}

fn analysis_projection_marker(
    source: &crate::source_db::FjallReadWriteSourceStorageHandle,
    source_id: &str,
) -> Result<Option<AnalysisProjectionRevision>, CommandError> {
    let key = encode_key(
        "analysis_projection",
        &[KeyPart::Text(source_id.to_owned())],
    )
    .map_err(|error| CommandError::internal(error.to_string()))?;
    let snapshot = source.snapshot();
    let Some(envelope) = source
        .read_envelope(&snapshot, persistence_fjall::KeyspaceName::Meta, &key)
        .map_err(|error| CommandError::internal(error.to_string()))?
    else {
        return Ok(None);
    };
    if envelope.record_kind != "analysis_projection_complete" || envelope.source_id != source_id {
        return Ok(None);
    }
    envelope
        .decode_payload()
        .map(Some)
        .map_err(|error| CommandError::internal(error.to_string()))
}

fn ensure_analysis_projection_building(
    source: &crate::source_db::FjallReadWriteSourceStorageHandle,
    source_id: &str,
) -> Result<(), CommandError> {
    ensure_projection_building(source, source_id, "analysis_projection_state")
}

fn write_analysis_projection_state(
    source: &crate::source_db::FjallReadWriteSourceStorageHandle,
    source_id: &str,
    record_kind: &str,
) -> Result<(), CommandError> {
    let key = encode_key(
        "analysis_projection_state",
        &[KeyPart::Text(source_id.to_owned())],
    )
    .map_err(|error| CommandError::internal(error.to_string()))?;
    let marker = VersionedEnvelope::new(
        record_kind,
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

fn ensure_projection_building(
    source: &crate::source_db::FjallReadWriteSourceStorageHandle,
    source_id: &str,
    family: &str,
) -> Result<(), CommandError> {
    let building_kind = if family == "projection" {
        "source_projection_building"
    } else {
        "analysis_projection_building"
    };
    let key = encode_key(family, &[KeyPart::Text(source_id.to_owned())])
        .map_err(|error| CommandError::internal(error.to_string()))?;
    let snapshot = source.snapshot();
    let current = source
        .read_envelope(&snapshot, persistence_fjall::KeyspaceName::Meta, &key)
        .map_err(|error| CommandError::internal(error.to_string()))?;
    if current.is_some_and(|envelope| {
        envelope.record_kind == building_kind && envelope.source_id == source_id
    }) {
        return Ok(());
    }
    let marker = VersionedEnvelope::new(
        building_kind,
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

fn catalog_projection_is_published(
    source: &crate::source_db::FjallReadWriteSourceStorageHandle,
    source_id: &str,
) -> Result<bool, CommandError> {
    let snapshot = source.snapshot();
    let key = encode_key("projection", &[KeyPart::Text(source_id.to_owned())])
        .map_err(|error| CommandError::internal(error.to_string()))?;
    Ok(source
        .read_envelope(&snapshot, persistence_fjall::KeyspaceName::Meta, &key)
        .map_err(|error| CommandError::internal(error.to_string()))?
        .is_some_and(|envelope| {
            envelope.record_kind == "source_projection_complete" && envelope.source_id == source_id
        }))
}
