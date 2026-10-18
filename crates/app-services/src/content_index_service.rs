//! Cancellable content indexing publishes a complete metadata + content generation.
pub mod coverage;
mod projection;
use domain::{CaseId, DataSourceId};
use persistence_sqlite::repositories::{content_index_repo::ContentIndexRepo, file_repo::FileRepo};
use search::SearchIndex;
use sha2::{Digest, Sha256};
use std::{
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
};
use transport::{
    dto::{ContentIndexSummaryDto, EvidenceTaskRequestDto},
    CommandError,
};

pub fn build(
    conn: &rusqlite::Connection,
    root: &Path,
    case: &CaseId,
    source_id: &str,
    cancel: &AtomicBool,
    progress: &(dyn Fn(u64, u64) + Sync),
) -> Result<ContentIndexSummaryDto, CommandError> {
    let request = EvidenceTaskRequestDto::ContentIndex {
        data_source_id: source_id.into(),
    };
    let revision = crate::evidence_tasks::revision(conn, root, case, &request)?;
    let projection_source = DataSourceId(source_id.into());
    let projection = crate::search_metadata_projection::MetadataProjection::new(
        &projection_source,
        revision.clone(),
    );
    let key = hex::encode(Sha256::digest(format!("content-v5:{source_id}:{revision}")));
    let source = crate::source_db::open_ready_source_read_only_by_id(
        conn,
        root,
        case,
        &DataSourceId(source_id.into()),
    )
    .map_err(CommandError::from_service_error)?;
    let current = crate::source_db::registered_source_index_dir(conn, root, &source.data_source_id)
        .map_err(CommandError::from_service_error)?;
    let next = current.with_extension(format!("content-{}", &key[..32]));
    let repo = ContentIndexRepo::new(conn);
    let expected = FileRepo::new(&source.connection)
        .count_by_data_source(&source.data_source_id)
        .map_err(CommandError::from_service_error)?;
    let mut summary = ContentIndexSummaryDto {
        expected,
        ..ContentIndexSummaryDto::default()
    };
    let mut after = String::new();
    if let Some((cursor, json, _)) = repo
        .checkpoint(&key)
        .map_err(CommandError::from_service_error)?
    {
        if next.exists() {
            summary = serde_json::from_str(&json)
                .map_err(|_| CommandError::internal("Index checkpoint is corrupt"))?;
            after = cursor;
        }
    }
    repo.begin(&key, source_id, &revision, &encode(&summary)?)
        .map_err(CommandError::from_service_error)?;
    let index = SearchIndex::create(&next).map_err(index_error)?;
    let mut writer = index.metadata_writer().map_err(index_error)?;
    let mut context = crate::file_service::SourceReadContext::new(
        &source.connection,
        conn,
        root,
        case,
        &source.data_source_id,
    );
    index_remaining(
        &mut context,
        &mut writer,
        IndexPageContext {
            source_id,
            projection: &projection,
            repo: ContentIndexRepo::new(conn),
            key: &key,
            cancel,
            progress,
        },
        &mut summary,
        &mut after,
    )?;
    writer.commit().map_err(index_error)?;
    projection.validate(
        &projection_source,
        &crate::evidence_tasks::revision(conn, root, case, &request)?,
    )?;
    ensure_active(cancel)?;
    summary.complete = projection::processed(&summary) == expected;
    if !summary.complete {
        return Err(CommandError::internal(
            "Index catalog coverage is incomplete",
        ));
    }
    // The manifest travels with the generation; a crash after rename cannot
    // make a completed coverage report point at a different index.
    let manifest = serde_json::json!({"sourceRevision":revision,"summary":summary,"indexGeneration":index.generation()});
    std::fs::write(next.join("content-coverage.json"), manifest.to_string())
        .map_err(CommandError::from_service_error)?;
    drop(index);
    crate::import_analysis::replace_index_generation(&next, &current)
        .map_err(CommandError::from_service_error)?;
    repo.publish(&key)
        .map_err(CommandError::from_service_error)?;
    Ok(summary)
}
fn ensure_active(cancel: &AtomicBool) -> Result<(), CommandError> {
    if cancel.load(Ordering::Acquire) {
        Err(CommandError::cancelled("Content indexing cancelled"))
    } else {
        Ok(())
    }
}
fn encode(summary: &ContentIndexSummaryDto) -> Result<String, CommandError> {
    serde_json::to_string(summary)
        .map_err(|_| CommandError::internal("Index checkpoint encoding failed"))
}
fn index_error(error: search::indexer::tantivy_writer::IndexError) -> CommandError {
    tracing::warn!(error = %error, "Search index operation failed");
    CommandError::internal("Search index operation failed")
}

struct IndexPageContext<'a> {
    source_id: &'a str,
    projection: &'a crate::search_metadata_projection::MetadataProjection<'a>,
    repo: ContentIndexRepo<'a>,
    key: &'a str,
    cancel: &'a AtomicBool,
    progress: &'a (dyn Fn(u64, u64) + Sync),
}
fn index_remaining(
    context: &mut crate::file_service::SourceReadContext<'_>,
    writer: &mut search::SearchMetadataWriter,
    run: IndexPageContext<'_>,
    summary: &mut ContentIndexSummaryDto,
    after: &mut String,
) -> Result<(), CommandError> {
    let IndexPageContext {
        source_id,
        projection,
        repo,
        key,
        cancel,
        progress,
    } = run;
    let expected = summary.expected;
    loop {
        ensure_active(cancel)?;
        let entries = FileRepo::new(context.source_connection())
            .index_page(source_id, after, 128)
            .map_err(CommandError::from_service_error)?;
        if entries.is_empty() {
            break;
        }
        let mut statuses = Vec::with_capacity(entries.len());
        for entry in entries {
            ensure_active(cancel)?;
            let (status, text) = projection::read(&mut *context, &entry, cancel)?;
            projection::count(summary, status);
            let global = crate::source_db::encode_source_scoped_id(
                &DataSourceId(source_id.into()),
                &entry.id.0,
            );
            let artifacts = ContentIndexRepo::new(context.source_connection())
                .artifact_text(source_id, &entry.id.0, &global)
                .map_err(CommandError::from_service_error)?;
            writer
                .add_content_document(&projection.entry(&entry)?, &text, &artifacts)
                .map_err(index_error)?;
            *after = entry.id.0.clone();
            statuses.push((entry.id.0, status.into()));
        }
        writer.checkpoint().map_err(index_error)?;
        repo.save_page(key, after, &encode(summary)?, &statuses)
            .map_err(CommandError::from_service_error)?;
        progress(projection::processed(summary), expected);
    }
    Ok(())
}
