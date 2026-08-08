use domain::{EntryType, FileEntryId};
use persistence_sqlite::repositories::file_repo::FileRepo;
use rusqlite::Connection;
use search::{extract_text, SearchIndex, SearchResult};
use std::path::Path;
use transport::commands::SearchFilesRequest;
use transport::dto::{
    PerformanceReportDto, SearchFileResultPageDto, SearchHighlightDto, SearchHitDto,
    SearchResultPageDto, SearchSnippetDto,
};

use crate::performance::{measure_rows, metric, report, PerfSample};
mod case_search;
mod error;
pub use error::SearchError;

#[derive(Debug, Clone)]
pub struct IndexStats {
    pub indexed_count: u64,
    pub warning_count: u32,
    pub skipped_count: u32,
    pub failed_count: u32,
}

#[derive(Debug, Clone)]
pub struct InstrumentedFileSearchResult {
    pub page: SearchFileResultPageDto,
    pub performance_report: PerformanceReportDto,
}

pub fn index_files(
    conn: &Connection,
    index_dir: &Path,
    file_ids: &[FileEntryId],
    reader_fn: impl Fn(&FileEntryId) -> Option<Box<dyn std::io::Read>>,
) -> Result<IndexStats, SearchError> {
    let repo = FileRepo::new(conn);
    let mut texts = Vec::new();
    let mut paths = Vec::new();
    let mut warning_count = 0u32;
    let mut skipped_count = 0u32;
    let failed_count = 0u32;

    for file_id in file_ids {
        let entry = repo.find_by_id(file_id)?;
        if let Some(entry) = entry {
            if entry.entry_type == EntryType::Directory {
                continue;
            }
            let ext = entry.ext.as_deref().unwrap_or("");
            let mime = if matches!(ext, "txt" | "log" | "csv" | "json" | "xml" | "html" | "md") {
                Some("text/plain")
            } else {
                None
            };

            if let Some(reader) = reader_fn(&entry.id) {
                let text = extract_text(reader, &entry.id.0, mime);
                if !text.extractable {
                    skipped_count += 1;
                }
                texts.push(text);
                paths.push((entry.id.0.clone(), entry.path.clone()));
            } else {
                warning_count += 1;
                skipped_count += 1;
            }
        }
    }

    if texts.is_empty() {
        return Ok(IndexStats {
            indexed_count: 0,
            warning_count,
            skipped_count,
            failed_count,
        });
    }

    let index = SearchIndex::create(index_dir).map_err(|e| SearchError::Index(e.to_string()))?;
    let count = index
        .index_documents(&texts, &paths)
        .map_err(|e| SearchError::Index(e.to_string()))?;

    Ok(IndexStats {
        indexed_count: count,
        warning_count,
        skipped_count,
        failed_count,
    })
}

pub fn search_files_real(
    index_dir: &Path,
    query: &str,
    offset: u64,
    limit: u32,
) -> Result<SearchResultPageDto, SearchError> {
    let content_index_dir = crate::source_db::source_content_index_dir(index_dir);
    let active_index_dir = if content_index_dir.exists() {
        content_index_dir.as_path()
    } else {
        index_dir
    };
    let index =
        SearchIndex::open(active_index_dir).map_err(|e| SearchError::Index(e.to_string()))?;
    if !index.supports_stable_paging() {
        return Err(SearchError::Unsupported(
            "Search index schema does not support deterministic pagination; rebuild the data source search index"
                .to_string(),
        ));
    }
    let start = std::time::Instant::now();
    let SearchResult { hits, total_count } = index
        .search_page(query, offset as usize, limit as usize)
        .map_err(|e| SearchError::Index(e.to_string()))?;
    let took_ms = start.elapsed().as_millis() as u64;

    let items: Vec<SearchHitDto> = hits
        .into_iter()
        .map(|h| {
            let snippets: Vec<SearchSnippetDto> = h
                .snippets
                .into_iter()
                .map(|s| SearchSnippetDto {
                    text: s.text,
                    highlights: s
                        .highlights
                        .into_iter()
                        .map(|hl| SearchHighlightDto {
                            start: hl.start,
                            end: hl.end,
                        })
                        .collect(),
                })
                .collect();
            SearchHitDto {
                file_id: h.file_id,
                path: h.path,
                score: h.score,
                snippets: if snippets.is_empty() {
                    vec![SearchSnippetDto {
                        text: String::new(),
                        highlights: vec![],
                    }]
                } else {
                    snippets
                },
            }
        })
        .collect();

    Ok(SearchResultPageDto {
        total: total_count,
        available: total_count,
        truncated: false,
        took_ms,
        items,
        next_cursor: None,
    })
}

pub fn search_files_for_case(
    case_conn: &Connection,
    case_root: &Path,
    case_id: &domain::CaseId,
    request: &SearchFilesRequest,
) -> Result<SearchFileResultPageDto, SearchError> {
    case_search::search_files_for_case(case_conn, case_root, case_id, request)
}

pub fn search_files_for_case_cursor(
    case_conn: &Connection,
    case_root: &Path,
    case_id: &domain::CaseId,
    request: &SearchFilesRequest,
) -> Result<SearchFileResultPageDto, SearchError> {
    case_search::search_files_for_case_cursor(case_conn, case_root, case_id, request)
}

pub fn search_files_for_case_instrumented(
    case_conn: &Connection,
    case_root: &Path,
    case_id: &domain::CaseId,
    request: &SearchFilesRequest,
) -> Result<InstrumentedFileSearchResult, SearchError> {
    let (page, sample) = measure_rows(0, || {
        search_files_for_case(case_conn, case_root, case_id, request)
    });
    let page = page?;
    let sample = PerfSample {
        elapsed_ms: page.took_ms.max(sample.elapsed_ms),
        rows: page.items.len() as u64,
    };
    let performance_report = search_query_report(sample, page.total);
    Ok(InstrumentedFileSearchResult {
        page,
        performance_report,
    })
}

pub fn search_files_for_case_cursor_instrumented(
    case_conn: &Connection,
    case_root: &Path,
    case_id: &domain::CaseId,
    request: &SearchFilesRequest,
) -> Result<InstrumentedFileSearchResult, SearchError> {
    let (page, sample) = measure_rows(0, || {
        search_files_for_case_cursor(case_conn, case_root, case_id, request)
    });
    let page = page?;
    let sample = PerfSample {
        elapsed_ms: page.took_ms.max(sample.elapsed_ms),
        rows: page.items.len() as u64,
    };
    let performance_report = search_query_report(sample, page.total);
    Ok(InstrumentedFileSearchResult {
        page,
        performance_report,
    })
}

fn search_query_report(sample: PerfSample, total: u64) -> PerformanceReportDto {
    let mut metrics = vec![
        metric("search.query.elapsedMs", sample.elapsed_ms as f64, "ms"),
        metric("search.query.rows", sample.rows as f64, "rows"),
        metric("search.query.totalRows", total as f64, "rows"),
    ];
    if let Some(rows_per_sec) = sample.rows_per_sec() {
        metrics.push(metric("search.query.rowsPerSec", rows_per_sec, "rows/s"));
    }
    report(
        format!("search.query:{}:{}", sample.elapsed_ms, sample.rows),
        None,
        sample.elapsed_ms,
        format!(
            "Search query returned {} rows in {} ms",
            sample.rows, sample.elapsed_ms
        ),
        metrics,
    )
}

#[cfg(test)]
#[path = "../tests/unit/search_service.rs"]
mod tests;
