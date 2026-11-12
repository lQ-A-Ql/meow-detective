use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::Ordering,
};

use crate::search_metadata_projection::{MetadataProjection, SearchProjectionMarker};
use persistence_source::fjall::{
    CatalogEntryType, KeyPart, PersistMode, ScanDirection, VersionedEnvelope,
};
use rusqlite::Connection;
use search::{SearchIndex, SearchMetadataWriter};

use super::{
    extractor_policy::validate_analysis_platform,
    options::{SearchIndexPhaseOptions, SearchIndexPhaseStats},
    ImportAnalysisError,
};

const SEARCH_METADATA_PAGE_SIZE: u64 = 1_000;

pub fn run_search_index_phase(
    options: SearchIndexPhaseOptions,
) -> Result<SearchIndexPhaseStats, ImportAnalysisError> {
    run_search_index_phase_with_page_size(options, SEARCH_METADATA_PAGE_SIZE)
}

fn run_search_index_phase_with_page_size(
    options: SearchIndexPhaseOptions,
    page_size: u64,
) -> Result<SearchIndexPhaseStats, ImportAnalysisError> {
    if !(1..=4000).contains(&page_size) {
        return Err(ImportAnalysisError::Other(
            "Invalid metadata page size".into(),
        ));
    }
    let _budget = source_kernel::resource_budget::pool_for(options.publication_lease.as_ref())
        .acquire(
            source_kernel::resource_budget::Resources {
                bytes: 128 * source_kernel::resource_budget::MIB,
                ..Default::default()
            },
            Some(&options.cancel_token),
        )
        .map_err(|e| ImportAnalysisError::Io(e.into_io()))?;
    validate_analysis_platform(options.platform)?;
    reconcile_index_generation(&options.index_dir)?;
    ensure_not_cancelled(&options)?;
    let connection = persistence_sqlite::open_existing_source_read_only(&options.db_path)?;
    let projection =
        MetadataProjection::new(&options.data_source_id, search_input_revision(&connection)?);
    let next_index_dir = next_index_dir(&options.index_dir);
    let mut build = IndexBuildGuard::prepare(next_index_dir.clone())?;
    let index = SearchIndex::create(&next_index_dir)
        .map_err(|error| ImportAnalysisError::Other(format!("Create search index: {error}")))?;
    let mut writer = index
        .metadata_writer_for_new_generation()
        .map_err(|error| ImportAnalysisError::Other(format!("Open search writer: {error}")))?;
    let mut stats = SearchIndexPhaseStats::default();
    let source = source_kernel::source_db::open_registered_fjall_source_read_only(
        &options.case_root,
        &options.data_source_id,
    )?;
    let _ = index_fjall_metadata_rows(
        &source,
        &options,
        &projection,
        page_size,
        &mut writer,
        &mut stats,
    )?;
    writer
        .commit()
        .map_err(|error| ImportAnalysisError::Other(format!("Commit search index: {error}")))?;
    stats.skipped_count = stats
        .eligible_count
        .saturating_sub(stats.indexed_count.saturating_add(stats.failed_count));
    let generation = index.generation().to_owned();
    let schema_version = index.schema_version();
    drop(index);
    projection
        .validate(
            &options.data_source_id,
            &search_input_revision(&connection)?,
        )
        .map_err(|_| ImportAnalysisError::Other("Search projection input changed".into()))?;
    if let Some(lease) = &options.publication_lease {
        lease.validate()?;
    }
    ensure_not_cancelled(&options)?;
    replace_index_generation(&next_index_dir, &options.index_dir)?;
    build.mark_published();
    write_search_marker(&options, projection.revision(), &generation, schema_version)?;
    Ok(stats)
}

fn write_search_marker(
    options: &SearchIndexPhaseOptions,
    input_revision: &str,
    generation: &str,
    schema_version: u32,
) -> Result<(), ImportAnalysisError> {
    let source = source_kernel::source_db::open_registered_fjall_source_read_write(
        &options.case_root,
        &options.data_source_id,
    )?;
    let marker_payload = SearchProjectionMarker {
        source_id: options.data_source_id.0.clone(),
        input_revision: input_revision.to_owned(),
        generation: generation.to_owned(),
        schema_version,
        index_path: options.index_dir.to_string_lossy().into_owned(),
    };
    let marker = VersionedEnvelope::from_serializable(
        "search_projection_generation",
        1,
        format!("search:{}", options.data_source_id.0),
        &options.data_source_id.0,
        1,
        &marker_payload,
    )
    .map_err(|error| ImportAnalysisError::Other(error.to_string()))?;
    let key = persistence_source::fjall::encode_key(
        "search",
        &[KeyPart::Text(options.data_source_id.0.clone())],
    )
    .map_err(|error| ImportAnalysisError::Other(error.to_string()))?;
    source
        .write_envelope(
            persistence_source::fjall::KeyspaceName::Meta,
            key,
            &marker,
            PersistMode::SyncAll,
        )
        .map_err(|error| ImportAnalysisError::Other(error.to_string()))?;
    Ok(())
}

fn search_input_revision(conn: &Connection) -> Result<String, ImportAnalysisError> {
    persistence_sqlite::repositories::source_meta_repo::SourceMetaRepo::new(conn)
        .read_revision(
            persistence_sqlite::repositories::source_meta_repo::SEARCH_INPUT_REVISION_KEY,
        )
        .map(|revision| revision.to_string())
        .map_err(|error| ImportAnalysisError::Other(error.to_string()))
}

fn index_fjall_metadata_rows(
    source: &source_kernel::storage_runtime::ReadOnlySourceStorageHandle,
    options: &SearchIndexPhaseOptions,
    projection: &MetadataProjection<'_>,
    page_size: u64,
    writer: &mut SearchMetadataWriter,
    stats: &mut SearchIndexPhaseStats,
) -> Result<u64, ImportAnalysisError> {
    let snapshot = source.snapshot();
    let mut cursor = None;
    loop {
        ensure_not_cancelled(options)?;
        let page = source
            .catalog_list_all(
                &snapshot,
                &options.data_source_id.0,
                cursor.as_deref(),
                ScanDirection::Forward,
                page_size as usize,
            )
            .map_err(|error| ImportAnalysisError::Other(error.to_string()))?;
        if page.records.is_empty() {
            break;
        }
        stats.eligible_count = stats
            .eligible_count
            .saturating_add(page.records.len() as u64);
        let mut documents = Vec::with_capacity(page.records.len());
        for record in page.records {
            let entry = domain::FileEntry {
                id: domain::FileEntryId(record.id),
                parent_id: record.parent_id.map(domain::FileEntryId),
                data_source_id: domain::DataSourceId(record.data_source_id),
                path: record.path,
                name: record.name,
                entry_type: match record.entry_type {
                    CatalogEntryType::File => domain::EntryType::File,
                    CatalogEntryType::Directory => domain::EntryType::Directory,
                },
                size: record.size,
                ext: record.ext,
                deleted: record.deleted,
                hidden: record.hidden,
                system: record.system,
                encrypted: record.encrypted.unwrap_or(true),
                read_only: record.read_only,
                archive: record.archive,
                unix_mode: record.unix_mode,
                created_at: record.created,
                modified_at: record.modified,
                accessed_at: record.accessed,
                changed_at: record.changed,
                hash_sha256: record.hash_sha256,
            };
            match projection.entry(&entry) {
                Ok(document) => documents.push(document),
                Err(error) => {
                    stats.failed_count = stats.failed_count.saturating_add(1);
                    tracing::warn!(error = %error, "Skipping malformed Fjall file metadata during search indexing");
                }
            }
        }
        stats.indexed_count =
            stats
                .indexed_count
                .saturating_add(writer.add_documents(&documents).map_err(|error| {
                    ImportAnalysisError::Other(format!("Write search metadata: {error}"))
                })?);
        cursor = page.next_cursor;
        if cursor.is_none() {
            break;
        }
    }
    Ok(stats.indexed_count)
}

struct IndexBuildGuard {
    path: PathBuf,
    published: bool,
}

impl IndexBuildGuard {
    fn prepare(path: PathBuf) -> Result<Self, ImportAnalysisError> {
        reset_build_directory(&path)?;
        Ok(Self {
            path,
            published: false,
        })
    }

    fn mark_published(&mut self) {
        self.published = true;
    }
}

impl Drop for IndexBuildGuard {
    fn drop(&mut self) {
        if !self.published && self.path.exists() {
            if let Err(error) = fs::remove_dir_all(&self.path) {
                tracing::warn!(
                    path = %self.path.display(),
                    error = %error,
                    "Failed to clean an unpublished search index generation"
                );
            }
        }
    }
}

fn next_index_dir(index_dir: &Path) -> PathBuf {
    index_dir.with_extension("next")
}

fn previous_index_dir(index_dir: &Path) -> PathBuf {
    index_dir.with_extension("previous")
}

fn reset_build_directory(path: &Path) -> Result<(), ImportAnalysisError> {
    if path.try_exists().map_err(ImportAnalysisError::Io)? {
        fs::remove_dir_all(path).map_err(ImportAnalysisError::Io)?;
    }
    Ok(())
}

fn reconcile_index_generation(current: &Path) -> Result<(), ImportAnalysisError> {
    let previous = previous_index_dir(current);
    let next = next_index_dir(current);
    let current_exists = current.try_exists().map_err(ImportAnalysisError::Io)?;
    let previous_exists = previous.try_exists().map_err(ImportAnalysisError::Io)?;
    if !current_exists && previous_exists {
        fs::rename(&previous, current).map_err(ImportAnalysisError::Io)?;
    } else if current_exists && previous_exists {
        fs::remove_dir_all(&previous).map_err(ImportAnalysisError::Io)?;
    }
    if next.try_exists().map_err(ImportAnalysisError::Io)? {
        fs::remove_dir_all(next).map_err(ImportAnalysisError::Io)?;
    }
    Ok(())
}

pub fn replace_index_generation(next: &Path, current: &Path) -> Result<(), ImportAnalysisError> {
    let previous = previous_index_dir(current);
    if previous.try_exists().map_err(ImportAnalysisError::Io)? {
        fs::remove_dir_all(&previous).map_err(ImportAnalysisError::Io)?;
    }
    let had_current = current.try_exists().map_err(ImportAnalysisError::Io)?;
    if had_current {
        fs::rename(current, &previous).map_err(ImportAnalysisError::Io)?;
    }
    if let Err(error) = fs::rename(next, current) {
        if had_current {
            let _ = fs::rename(&previous, current);
        }
        return Err(ImportAnalysisError::Io(error));
    }
    if let Err(error) =
        SearchIndex::open(current).and_then(|index| index.validate_file_search_schema())
    {
        let _ = fs::remove_dir_all(current);
        if had_current {
            let _ = fs::rename(&previous, current);
        }
        return Err(ImportAnalysisError::Other(format!(
            "Published search index validation failed: {error}"
        )));
    }
    if had_current {
        if let Err(error) = fs::remove_dir_all(&previous) {
            tracing::warn!(
                path = %previous.display(),
                error = %error,
                "Published search index is active, but its previous generation remains"
            );
        }
    }
    Ok(())
}

fn ensure_not_cancelled(options: &SearchIndexPhaseOptions) -> Result<(), ImportAnalysisError> {
    if options.cancel_token.load(Ordering::Relaxed) {
        Err(ImportAnalysisError::Other(
            "Search indexing cancelled by user".to_string(),
        ))
    } else {
        Ok(())
    }
}

#[cfg(test)]
#[path = "../../tests/unit/import_analysis/search_batch_experiment.rs"]
mod tests;
