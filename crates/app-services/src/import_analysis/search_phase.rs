use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::Ordering,
};

use domain::{DataSourcePlatform, EntryType, FileEntry, FileEntryId};
use rusqlite::{params, Connection};
use search::{extract_text, ExtractedText, SearchIndex};

use super::{
    budget::ContentBudget,
    extractor_policy::validate_analysis_platform,
    options::{SearchIndexPhaseOptions, SearchIndexPhaseStats},
    search_policy::{mime_hint_for_entry, search_budget_allows_file, should_index_file},
    source_reader::{prepare_derived_runtime_for_source, AnalysisSourceReader},
    worker_runtime::{release_content_quota, reserve_content_quota, SharedAnalysisState},
    ImportAnalysisError,
};

const SEARCH_INDEX_BATCH_SIZE: usize = 50;
const SEARCH_CANDIDATE_PAGE_SIZE: u64 = 256;
const SEARCH_ELIGIBLE_PREDICATE: &str = r#"
    data_source_id = ?1
    AND LOWER(entry_type) = 'file'
    AND size IS NOT NULL
    AND size <= ?2
    AND (
        (
            ext IS NOT NULL
            AND LOWER(LTRIM(ext, '.')) IN
                ('txt', 'log', 'csv', 'json', 'xml', 'html', 'htm', 'md')
        )
        OR (
            ext IS NULL
            AND (
                LOWER(name) LIKE '%.txt'
                OR LOWER(name) LIKE '%.log'
                OR LOWER(name) LIKE '%.csv'
                OR LOWER(name) LIKE '%.json'
                OR LOWER(name) LIKE '%.xml'
                OR LOWER(name) LIKE '%.html'
                OR LOWER(name) LIKE '%.htm'
                OR LOWER(name) LIKE '%.md'
            )
        )
        OR (
            ?3 = 1
            AND LOWER(TRIM(name)) IN (
                'crypttab', 'fstab', 'group', 'gshadow', 'hostname', 'hosts',
                'machine-id', 'mtab', 'networks', 'os-release', 'passwd',
                'protocols', 'services', 'shadow', 'shells', 'sudoers'
            )
            AND COALESCE(LTRIM(ext, '.'), '') = ''
        )
    )
"#;
const SEARCH_PRIORITY_EXPRESSION: &str = r#"
    CASE
        WHEN ?3 = 1
            AND LOWER(TRIM(name)) IN (
                'crypttab', 'fstab', 'group', 'gshadow', 'hostname', 'hosts',
                'machine-id', 'mtab', 'networks', 'os-release', 'passwd',
                'protocols', 'services', 'shadow', 'shells', 'sudoers'
            )
            AND COALESCE(LTRIM(ext, '.'), '') = ''
        THEN 0
        ELSE 1
    END
"#;

#[derive(Debug, Clone)]
struct SearchCursor {
    priority_rank: i64,
    path: String,
    id: String,
}

impl SearchCursor {
    fn before_first() -> Self {
        Self {
            priority_rank: -1,
            path: String::new(),
            id: String::new(),
        }
    }
}

#[derive(Debug)]
struct SearchCandidate {
    file: FileEntry,
    priority_rank: i64,
}

pub(crate) fn run_search_index_phase(
    options: SearchIndexPhaseOptions,
) -> Result<SearchIndexPhaseStats, ImportAnalysisError> {
    validate_analysis_platform(options.platform)?;
    reconcile_index_generation(&options.index_dir)?;
    let connection = persistence_sqlite::open_existing_source_read_only(&options.db_path)?;
    let derived_runtime = prepare_derived_runtime_for_source(
        &options.case_root,
        &options.db_path,
        &options.case_id,
        &options.data_source_id,
        true,
    )
    .map_err(ImportAnalysisError::Other)?;
    let mut source_reader = AnalysisSourceReader::for_source(
        options.case_id.clone(),
        options.data_source_id.clone(),
        derived_runtime,
    );
    let next_index_dir = next_index_dir(&options.index_dir);
    let mut build = IndexBuildGuard::prepare(next_index_dir.clone())?;
    let index = SearchIndex::create(&next_index_dir)
        .map_err(|error| ImportAnalysisError::Other(format!("Create search index: {error}")))?;
    let budget = ContentBudget::conservative();
    let usage = SharedAnalysisState::new();
    let mut stats = SearchIndexPhaseStats {
        eligible_count: count_eligible_search_candidates(
            &connection,
            &options.data_source_id,
            options.platform,
        )?,
        ..SearchIndexPhaseStats::default()
    };
    let mut texts = Vec::with_capacity(SEARCH_INDEX_BATCH_SIZE);
    let mut paths = Vec::with_capacity(SEARCH_INDEX_BATCH_SIZE);
    scan_search_candidates(
        &connection,
        &mut source_reader,
        &options,
        &budget,
        &usage,
        &index,
        &mut stats,
        &mut texts,
        &mut paths,
    )?;
    flush_index_batch(&index, &mut texts, &mut paths, &mut stats)?;
    let accounted = stats
        .indexed_count
        .checked_add(stats.failed_count)
        .ok_or_else(|| {
            ImportAnalysisError::Other("Search phase counters overflowed".to_string())
        })?;
    stats.skipped_count = stats.eligible_count.checked_sub(accounted).ok_or_else(|| {
        ImportAnalysisError::Other(format!(
            "Search phase accounting exceeded eligible candidates: eligible={}, indexed={}, failed={}",
            stats.eligible_count, stats.indexed_count, stats.failed_count
        ))
    })?;
    drop(index);
    if stats.failed_count > 0 {
        return Err(ImportAnalysisError::Other(format!(
            "Search indexing failed to read {} eligible files; the previous index was preserved",
            stats.failed_count
        )));
    }
    replace_index_generation(&next_index_dir, &options.index_dir)?;
    build.mark_published();
    Ok(stats)
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

#[allow(clippy::too_many_arguments)]
fn scan_search_candidates(
    connection: &rusqlite::Connection,
    source_reader: &mut AnalysisSourceReader,
    options: &SearchIndexPhaseOptions,
    budget: &ContentBudget,
    usage: &SharedAnalysisState,
    index: &SearchIndex,
    stats: &mut SearchIndexPhaseStats,
    texts: &mut Vec<ExtractedText>,
    paths: &mut Vec<(String, String)>,
) -> Result<(), ImportAnalysisError> {
    let mut cursor = SearchCursor::before_first();
    loop {
        ensure_not_cancelled(options)?;
        if index_limit_reached(usage) {
            break;
        }
        let page = fetch_search_candidate_page(
            connection,
            &options.data_source_id,
            options.platform,
            &cursor,
        )?;
        if page.is_empty() {
            break;
        }
        let page_len = page.len();
        for candidate in page {
            ensure_not_cancelled(options)?;
            cursor = SearchCursor {
                priority_rank: candidate.priority_rank,
                path: candidate.file.path.clone(),
                id: candidate.file.id.0.clone(),
            };
            if index_limit_reached(usage) {
                break;
            }
            index_candidate(
                connection,
                source_reader,
                options,
                budget,
                usage,
                candidate.file,
                stats,
                texts,
                paths,
            )?;
            if texts.len() >= SEARCH_INDEX_BATCH_SIZE {
                flush_index_batch(index, texts, paths, stats)?;
            }
        }
        if page_len < SEARCH_CANDIDATE_PAGE_SIZE as usize {
            break;
        }
    }
    Ok(())
}

fn count_eligible_search_candidates(
    connection: &Connection,
    data_source_id: &domain::DataSourceId,
    platform: DataSourcePlatform,
) -> Result<u64, ImportAnalysisError> {
    let sql = format!("SELECT COUNT(*) FROM file_entries WHERE {SEARCH_ELIGIBLE_PREDICATE}");
    let count: i64 = connection.query_row(
        &sql,
        params![
            data_source_id.0,
            infrastructure::constants::IMPORT_TEXT_INDEX_FILE_LIMIT_BYTES,
            is_linux(platform)
        ],
        |row| row.get(0),
    )?;
    Ok(count.max(0) as u64)
}

fn fetch_search_candidate_page(
    connection: &Connection,
    data_source_id: &domain::DataSourceId,
    platform: DataSourcePlatform,
    cursor: &SearchCursor,
) -> Result<Vec<SearchCandidate>, ImportAnalysisError> {
    let sql = search_candidate_page_sql();
    let mut statement = connection.prepare(&sql)?;
    let rows = statement.query_map(
        params![
            data_source_id.0,
            infrastructure::constants::IMPORT_TEXT_INDEX_FILE_LIMIT_BYTES,
            is_linux(platform),
            cursor.priority_rank,
            cursor.path,
            cursor.id,
            SEARCH_CANDIDATE_PAGE_SIZE
        ],
        row_to_search_candidate,
    )?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(ImportAnalysisError::Db)
}

pub(super) fn search_candidate_page_sql() -> String {
    format!(
        "WITH eligible AS (
            SELECT id, data_source_id, path, name, size, ext, deleted, hidden, system,
                   {SEARCH_PRIORITY_EXPRESSION} AS priority_rank
            FROM file_entries
            WHERE {SEARCH_ELIGIBLE_PREDICATE}
        )
        SELECT id, data_source_id, path, name, size, ext, deleted, hidden, system, priority_rank
        FROM eligible
        WHERE priority_rank > ?4
           OR (
               priority_rank = ?4
               AND (path > ?5 OR (path = ?5 AND id > ?6))
           )
        ORDER BY priority_rank ASC, path ASC, id ASC
        LIMIT ?7"
    )
}

fn row_to_search_candidate(row: &rusqlite::Row<'_>) -> rusqlite::Result<SearchCandidate> {
    Ok(SearchCandidate {
        file: FileEntry {
            id: FileEntryId(row.get(0)?),
            parent_id: None,
            data_source_id: domain::DataSourceId(row.get(1)?),
            path: row.get(2)?,
            name: row.get(3)?,
            entry_type: EntryType::File,
            size: row.get(4)?,
            ext: row.get(5)?,
            deleted: row.get::<_, i32>(6)? != 0,
            hidden: row.get::<_, i32>(7)? != 0,
            system: row.get::<_, i32>(8)? != 0,
            encrypted: false,
            created_at: None,
            modified_at: None,
            accessed_at: None,
            changed_at: None,
            hash_sha256: None,
        },
        priority_rank: row.get(9)?,
    })
}

fn is_linux(platform: DataSourcePlatform) -> i64 {
    i64::from(platform == DataSourcePlatform::Linux)
}

fn index_limit_reached(usage: &SharedAnalysisState) -> bool {
    usage.indexed_total.load(Ordering::Relaxed)
        >= infrastructure::constants::IMPORT_TEXT_INDEX_LIMIT
}

#[allow(clippy::too_many_arguments)]
fn index_candidate(
    connection: &rusqlite::Connection,
    source_reader: &mut AnalysisSourceReader,
    options: &SearchIndexPhaseOptions,
    budget: &ContentBudget,
    usage: &SharedAnalysisState,
    file: domain::FileEntry,
    stats: &mut SearchIndexPhaseStats,
    texts: &mut Vec<ExtractedText>,
    paths: &mut Vec<(String, String)>,
) -> Result<(), ImportAnalysisError> {
    debug_assert!(should_index_file(&file, options.platform));
    if !search_budget_allows_file(budget, &file, options.platform)
        || !reserve_content_quota(budget, &file, usage)
    {
        return Ok(());
    }

    let bytes = match source_reader.read_file_header_by_id(
        connection,
        &file.id,
        infrastructure::constants::IMPORT_TEXT_INDEX_FILE_LIMIT_BYTES as usize,
    ) {
        Ok(bytes) => bytes,
        Err(error) => {
            release_content_quota(&file, usage);
            stats.failed_count += 1;
            tracing::warn!(path = %file.path, error = %error, "Search indexing could not read candidate");
            return Ok(());
        }
    };
    let text = extract_text(
        std::io::Cursor::new(bytes),
        &file.id.0,
        mime_hint_for_entry(&file, options.platform),
    );
    if !text.extractable || text.content.is_empty() {
        release_content_quota(&file, usage);
        return Ok(());
    }

    usage.indexed_total.fetch_add(1, Ordering::Relaxed);
    paths.push((file.id.0.clone(), file.path));
    texts.push(text);
    Ok(())
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

fn replace_index_generation(next: &Path, current: &Path) -> Result<(), ImportAnalysisError> {
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
    if let Err(error) = SearchIndex::open(current) {
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
                "Published search index is active, but the previous generation could not be removed"
            );
        }
    }
    Ok(())
}

fn flush_index_batch(
    index: &SearchIndex,
    texts: &mut Vec<ExtractedText>,
    paths: &mut Vec<(String, String)>,
    stats: &mut SearchIndexPhaseStats,
) -> Result<(), ImportAnalysisError> {
    if texts.is_empty() {
        return Ok(());
    }
    let attempted = texts.len() as u64;
    let indexed = index
        .index_documents(texts, paths)
        .map_err(|error| ImportAnalysisError::Other(format!("Write search index: {error}")))?;
    stats.indexed_count += indexed;
    debug_assert!(indexed <= attempted);
    texts.clear();
    paths.clear();
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
