use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::Ordering,
};

use crate::search_metadata_projection::MetadataProjection;
use rusqlite::{params, Connection};
use search::{SearchFileDocument, SearchIndex, SearchMetadataWriter};

use super::{
    extractor_policy::validate_analysis_platform,
    options::{SearchIndexPhaseOptions, SearchIndexPhaseStats},
    ImportAnalysisError,
};

const SEARCH_METADATA_PAGE_SIZE: u64 = 1_000;

pub(crate) fn run_search_index_phase(
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
    let _budget = crate::resource_budget::pool_for(options.publication_lease.as_ref())
        .acquire(
            crate::resource_budget::Resources {
                bytes: 128 * crate::resource_budget::MIB,
                ..Default::default()
            },
            Some(&options.cancel_token),
        )
        .map_err(|e| ImportAnalysisError::Io(e.into_io()))?;
    validate_analysis_platform(options.platform)?;
    reconcile_index_generation(&options.index_dir)?;
    ensure_not_cancelled(&options)?;
    let connection = persistence_sqlite::open_existing_source_read_only(&options.db_path)?;
    let projection = MetadataProjection::new(&options.data_source_id, data_version(&connection)?);
    let next_index_dir = next_index_dir(&options.index_dir);
    let mut build = IndexBuildGuard::prepare(next_index_dir.clone())?;
    let index = SearchIndex::create(&next_index_dir)
        .map_err(|error| ImportAnalysisError::Other(format!("Create search index: {error}")))?;
    let mut writer = index
        .metadata_writer_for_new_generation()
        .map_err(|error| ImportAnalysisError::Other(format!("Open search writer: {error}")))?;
    let mut stats = SearchIndexPhaseStats {
        eligible_count: count_metadata_rows(&connection, &options)?,
        ..SearchIndexPhaseStats::default()
    };
    index_metadata_rows(
        &connection,
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
    drop(index);
    projection
        .validate(&options.data_source_id, &data_version(&connection)?)
        .map_err(|_| ImportAnalysisError::Other("Search projection input changed".into()))?;
    if let Some(lease) = &options.publication_lease {
        lease.validate()?;
    }
    ensure_not_cancelled(&options)?;
    replace_index_generation(&next_index_dir, &options.index_dir)?;
    build.mark_published();
    Ok(stats)
}

fn data_version(conn: &Connection) -> rusqlite::Result<String> {
    conn.pragma_query_value(None, "data_version", |row| {
        row.get::<_, i64>(0).map(|v| v.to_string())
    })
}

fn count_metadata_rows(
    connection: &Connection,
    options: &SearchIndexPhaseOptions,
) -> Result<u64, ImportAnalysisError> {
    let count: i64 = connection.query_row(
        "SELECT COUNT(*) FROM file_entries WHERE data_source_id = ?1",
        [&options.data_source_id.0],
        |row| row.get(0),
    )?;
    Ok(count.max(0) as u64)
}

fn index_metadata_rows(
    connection: &Connection,
    options: &SearchIndexPhaseOptions,
    projection: &MetadataProjection<'_>,
    page_size: u64,
    writer: &mut SearchMetadataWriter,
    stats: &mut SearchIndexPhaseStats,
) -> Result<(), ImportAnalysisError> {
    let mut after_id = String::new();
    loop {
        ensure_not_cancelled(options)?;
        let page = fetch_metadata_page(
            connection,
            &options.data_source_id.0,
            &after_id,
            page_size,
            projection,
        )?;
        if page.scanned == 0 {
            break;
        }
        after_id = page.last_id;
        stats.failed_count = stats.failed_count.saturating_add(page.failed);
        stats.indexed_count =
            stats
                .indexed_count
                .saturating_add(writer.add_documents(&page.documents).map_err(|error| {
                    ImportAnalysisError::Other(format!("Write search metadata: {error}"))
                })?);
        if page.scanned < page_size {
            break;
        }
    }
    Ok(())
}

struct MetadataPage {
    documents: Vec<SearchFileDocument>,
    last_id: String,
    scanned: u64,
    failed: u64,
}

fn fetch_metadata_page(
    connection: &Connection,
    data_source_id: &str,
    after_id: &str,
    page_size: u64,
    projection: &MetadataProjection<'_>,
) -> Result<MetadataPage, ImportAnalysisError> {
    let mut statement = connection.prepare(
        "SELECT id, path, name, entry_type, size, ext, modified_at,
                deleted, hidden, system, encrypted, data_source_id
         FROM file_entries
         WHERE data_source_id = ?1 AND id > ?2
         ORDER BY id ASC
         LIMIT ?3",
    )?;
    let mut rows = statement.query(params![data_source_id, after_id, page_size])?;
    let mut page = MetadataPage {
        documents: Vec::with_capacity(page_size as usize),
        last_id: after_id.to_string(),
        scanned: 0,
        failed: 0,
    };
    while let Some(row) = rows.next()? {
        page.scanned = page.scanned.saturating_add(1);
        let id = match row.get::<_, String>(0) {
            Ok(id) if !id.is_empty() => id,
            Ok(_) | Err(_) => {
                page.failed = page.failed.saturating_add(1);
                continue;
            }
        };
        page.last_id = id.clone();
        match projection.row(row, id) {
            Ok(document) => page.documents.push(document),
            Err(error) => {
                page.failed = page.failed.saturating_add(1);
                tracing::warn!(error = %error, "Skipping malformed file metadata during search indexing");
            }
        }
    }
    Ok(page)
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

pub(crate) fn replace_index_generation(
    next: &Path,
    current: &Path,
) -> Result<(), ImportAnalysisError> {
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
