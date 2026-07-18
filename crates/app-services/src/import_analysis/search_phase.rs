use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::Ordering,
};

use search::{extract_text, ExtractedText, SearchIndex};

use super::{
    budget::ContentBudget,
    extractor_policy::validate_analysis_platform,
    options::{SearchIndexPhaseOptions, SearchIndexPhaseStats},
    search_policy::{
        is_priority_search_candidate, mime_hint_for_entry, search_budget_allows_file,
        should_index_file,
    },
    source_reader::{prepare_derived_runtime_for_source, AnalysisSourceReader},
    task_feed::{fetch_analysis_file_page, FILE_PAGE_SIZE},
    worker_runtime::{release_content_quota, reserve_content_quota, SharedAnalysisState},
    ImportAnalysisError,
};

const SEARCH_INDEX_BATCH_SIZE: usize = 50;

pub(crate) fn run_search_index_phase(
    options: SearchIndexPhaseOptions,
) -> Result<SearchIndexPhaseStats, ImportAnalysisError> {
    validate_analysis_platform(options.platform)?;
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
    reset_build_directory(&next_index_dir)?;
    let index = SearchIndex::create(&next_index_dir)
        .map_err(|error| ImportAnalysisError::Other(format!("Create search index: {error}")))?;
    let budget = ContentBudget::conservative();
    let usage = SharedAnalysisState::new();
    let mut stats = SearchIndexPhaseStats::default();
    let mut texts = Vec::with_capacity(SEARCH_INDEX_BATCH_SIZE);
    let mut paths = Vec::with_capacity(SEARCH_INDEX_BATCH_SIZE);
    for priority_pass in [true, false] {
        scan_search_candidates(
            &connection,
            &mut source_reader,
            &options,
            &budget,
            &usage,
            priority_pass,
            &index,
            &mut stats,
            &mut texts,
            &mut paths,
        )?;
    }

    flush_index_batch(&index, &mut texts, &mut paths, &mut stats)?;
    debug_assert_eq!(
        stats.eligible_count,
        stats.indexed_count + stats.skipped_count + stats.failed_count
    );
    drop(index);
    if stats.failed_count > 0 {
        let _ = fs::remove_dir_all(&next_index_dir);
        return Err(ImportAnalysisError::Other(format!(
            "Search indexing failed to read {} eligible files; the previous index was preserved",
            stats.failed_count
        )));
    }
    replace_index_generation(&next_index_dir, &options.index_dir)?;
    Ok(stats)
}

#[allow(clippy::too_many_arguments)]
fn scan_search_candidates(
    connection: &rusqlite::Connection,
    source_reader: &mut AnalysisSourceReader,
    options: &SearchIndexPhaseOptions,
    budget: &ContentBudget,
    usage: &SharedAnalysisState,
    priority_pass: bool,
    index: &SearchIndex,
    stats: &mut SearchIndexPhaseStats,
    texts: &mut Vec<ExtractedText>,
    paths: &mut Vec<(String, String)>,
) -> Result<(), ImportAnalysisError> {
    let mut offset = 0u64;
    loop {
        ensure_not_cancelled(options)?;
        let page =
            fetch_analysis_file_page(connection, &options.data_source_id, offset, FILE_PAGE_SIZE)?;
        if page.is_empty() {
            break;
        }
        let page_len = page.len() as u64;
        for task in page {
            ensure_not_cancelled(options)?;
            let file = task.to_file_entry();
            if is_priority_search_candidate(&file, options.platform) != priority_pass {
                continue;
            }
            index_candidate(
                connection,
                source_reader,
                options,
                budget,
                usage,
                file,
                stats,
                texts,
                paths,
            )?;
            if texts.len() >= SEARCH_INDEX_BATCH_SIZE {
                flush_index_batch(index, texts, paths, stats)?;
            }
        }
        offset = offset.saturating_add(page_len);
        if page_len < FILE_PAGE_SIZE {
            break;
        }
    }
    Ok(())
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
    if !should_index_file(&file, options.platform) {
        return Ok(());
    }
    stats.eligible_count += 1;
    if !search_budget_allows_file(budget, &file, options.platform)
        || !reserve_content_quota(budget, &file, usage)
        || usage.indexed_total.load(Ordering::Relaxed)
            >= infrastructure::constants::IMPORT_TEXT_INDEX_LIMIT
    {
        stats.skipped_count += 1;
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
        stats.skipped_count += 1;
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
    if had_current {
        fs::remove_dir_all(&previous).map_err(ImportAnalysisError::Io)?;
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
    stats.skipped_count += attempted.saturating_sub(indexed);
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
