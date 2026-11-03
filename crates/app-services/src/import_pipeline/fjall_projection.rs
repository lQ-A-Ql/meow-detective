use persistence_fjall::{encode_key, KeyPart, PersistMode, VersionedEnvelope};
use rusqlite::Connection;
use std::time::Instant;

use super::context::ImportJobContext;
use transport::CommandError;

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
    let result = mirror_source_records_from_connections(
        ctx.conn,
        ctx.case_root,
        &ctx.case_id.0,
        ctx.source_connection()?,
        source_id,
    );
    if result.is_ok() {
        ctx.report_job_progress(97, "Queryable record projection completed")?;
    }
    result
}

pub(crate) fn mirror_source_records_from_connections(
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
        true,
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
    )
}

fn mirror_source_records_from_connections_internal(
    case_connection: &Connection,
    case_root: &std::path::Path,
    case_id: &str,
    source_connection: &Connection,
    source_id: &str,
    include_catalog: bool,
) -> Result<String, CommandError> {
    let started = Instant::now();
    let source = crate::source_db::open_registered_fjall_source_read_write(
        case_root,
        &domain::DataSourceId(source_id.to_owned()),
    )
    .map_err(|_| CommandError::internal("Fjall source storage is unavailable"))?;
    tracing::info!(source_id, "Fjall source projection started");
    if !include_catalog || catalog_projection_is_published(&source, source_id)? {
        tracing::info!(
            source_id,
            "Skipping already-published Fjall catalog remirror"
        );
    } else {
        projections::mirror_catalog(source_connection, &source, source_id)?;
    }
    projections::mirror_timeline(source_connection, &source, source_id)?;
    projections::mirror_artifacts(
        case_connection,
        case_root,
        case_id,
        source_connection,
        &source,
        source_id,
    )?;
    records::mirror_android_parser_records(source_connection, &source, source_id)?;
    records::mirror_linux_parser_record(source_connection, &source, source_id)?;
    records::mirror_parser_tables(source_connection, &source, source_id)?;
    mirror_namespace_locators(source_connection, &source, source_id)?;
    mirror_nested_lineage(case_connection, &source, source_id)?;
    source
        .persist(PersistMode::SyncAll)
        .map_err(|error| CommandError::internal(error.to_string()))?;
    tracing::info!(
        source_id,
        elapsed_ms = started.elapsed().as_millis(),
        "Fjall source projection committed"
    );
    let marker = VersionedEnvelope::new(
        "source_projection_complete",
        1,
        format!("projection:{source_id}"),
        source_id,
        1,
        source_id.as_bytes().to_vec(),
    );
    let receipt = source
        .write_envelope(
            persistence_fjall::KeyspaceName::Meta,
            encode_key("projection", &[KeyPart::Text(source_id.to_owned())])
                .map_err(|error| CommandError::internal(error.to_string()))?,
            &marker,
            PersistMode::SyncAll,
        )
        .map_err(|error| CommandError::internal(error.to_string()))?;
    Ok(hex::encode(receipt.batch_digest))
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
        .is_some_and(|envelope| envelope.record_kind == "source_projection_complete"))
}
