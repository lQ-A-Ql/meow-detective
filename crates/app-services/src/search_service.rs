use domain::{EntryType, FileEntryId};
use persistence_sqlite::repositories::file_repo::FileRepo;
use rusqlite::Connection;
use search::{extract_text, SearchIndex, SearchResult};
use std::path::Path;
use transport::dto::{
    PerformanceReportDto, SearchHighlightDto, SearchHitDto, SearchResultPageDto, SearchSnippetDto,
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
pub struct InstrumentedIndexStats {
    pub stats: IndexStats,
    pub performance_report: PerformanceReportDto,
}

#[derive(Debug, Clone)]
pub struct InstrumentedSearchResult {
    pub page: SearchResultPageDto,
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

pub fn index_files_instrumented(
    conn: &Connection,
    index_dir: &Path,
    file_ids: &[FileEntryId],
    reader_fn: impl Fn(&FileEntryId) -> Option<Box<dyn std::io::Read>>,
) -> Result<InstrumentedIndexStats, SearchError> {
    let (stats, sample) = measure_rows(file_ids.len() as u64, || {
        index_files(conn, index_dir, file_ids, reader_fn)
    });
    let stats = stats?;
    let sample = PerfSample {
        rows: stats.indexed_count,
        ..sample
    };
    let performance_report = search_index_report(sample, &stats);
    Ok(InstrumentedIndexStats {
        stats,
        performance_report,
    })
}

pub fn search_files_real(
    index_dir: &Path,
    query: &str,
    offset: u64,
    limit: u32,
) -> Result<SearchResultPageDto, SearchError> {
    let index = SearchIndex::open(index_dir).map_err(|e| SearchError::Index(e.to_string()))?;
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
        took_ms,
        items,
    })
}

pub fn search_files_real_instrumented(
    index_dir: &Path,
    query: &str,
    offset: u64,
    limit: u32,
) -> Result<InstrumentedSearchResult, SearchError> {
    let (page, sample) = measure_rows(0, || search_files_real(index_dir, query, offset, limit));
    let page = page?;
    let sample = PerfSample {
        elapsed_ms: page.took_ms.max(sample.elapsed_ms),
        rows: page.items.len() as u64,
    };
    let performance_report = search_query_report(sample, page.total);
    Ok(InstrumentedSearchResult {
        page,
        performance_report,
    })
}

pub fn search_files_for_case(
    case_conn: &Connection,
    case_root: &Path,
    case_id: &domain::CaseId,
    query: &str,
    offset: u64,
    limit: u32,
) -> Result<SearchResultPageDto, SearchError> {
    case_search::search_files_for_case(case_conn, case_root, case_id, query, offset, limit)
}

pub fn search_files_for_case_instrumented(
    case_conn: &Connection,
    case_root: &Path,
    case_id: &domain::CaseId,
    query: &str,
    offset: u64,
    limit: u32,
) -> Result<InstrumentedSearchResult, SearchError> {
    let (page, sample) = measure_rows(0, || {
        search_files_for_case(case_conn, case_root, case_id, query, offset, limit)
    });
    let page = page?;
    let sample = PerfSample {
        elapsed_ms: page.took_ms.max(sample.elapsed_ms),
        rows: page.items.len() as u64,
    };
    let performance_report = search_query_report(sample, page.total);
    Ok(InstrumentedSearchResult {
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

fn search_index_report(sample: PerfSample, stats: &IndexStats) -> PerformanceReportDto {
    let mut metrics = vec![
        metric("search.index.elapsedMs", sample.elapsed_ms as f64, "ms"),
        metric("search.index.rows", stats.indexed_count as f64, "rows"),
        metric("search.index.skipped", stats.skipped_count as f64, "rows"),
        metric(
            "search.index.warnings",
            stats.warning_count as f64,
            "warnings",
        ),
        metric("search.index.failed", stats.failed_count as f64, "rows"),
    ];
    if let Some(rows_per_sec) = sample.rows_per_sec() {
        metrics.push(metric("search.index.rowsPerSec", rows_per_sec, "rows/s"));
    }
    report(
        format!("search.index:{}:{}", sample.elapsed_ms, stats.indexed_count),
        None,
        sample.elapsed_ms,
        format!(
            "Search indexing processed {} rows in {} ms",
            stats.indexed_count, sample.elapsed_ms
        ),
        metrics,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use domain::{DataSourceId, EntryType, FileEntry, FileEntryId};
    use persistence_sqlite::repositories::{datasource_repo::DataSourceRepo, file_repo::FileRepo};
    use search::ExtractedText;
    use std::io::Cursor;
    use tempfile::TempDir;
    fn setup_file_db() -> (rusqlite::Connection, Vec<FileEntryId>) {
        let conn = persistence_sqlite::connection::open_in_memory().unwrap();
        conn.execute_batch(include_str!(
            "../../persistence-sqlite/src/migrations/scripts/0001_cases.sql"
        ))
        .unwrap();
        conn.execute_batch(include_str!(
            "../../persistence-sqlite/src/migrations/scripts/0002_data_sources.sql"
        ))
        .unwrap();
        conn.execute_batch(include_str!(
            "../../persistence-sqlite/src/migrations/scripts/0003_file_entries.sql"
        ))
        .unwrap();
        conn.execute_batch(include_str!(
            "../../persistence-sqlite/src/migrations/scripts/0022_file_entry_visibility_flags.sql"
        ))
        .unwrap();
        conn.execute(
            "INSERT INTO cases (id, name, created_at, updated_at)
             VALUES ('case-1', 'Case', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO data_sources (id, case_id, name, kind, source_path, imported_at)
             VALUES ('ds-1', 'case-1', 'sample', 'LogicalDirectory', 'C:/sample', '2026-01-01T00:00:00Z')",
            [],
        )
        .unwrap();
        let ids = vec![
            FileEntryId("file-1".to_string()),
            FileEntryId("file-2".to_string()),
        ];
        let entries = ids
            .iter()
            .map(|id| FileEntry {
                id: id.clone(),
                parent_id: None,
                data_source_id: DataSourceId("ds-1".to_string()),
                path: format!("/{}.txt", id.0),
                name: format!("{}.txt", id.0),
                entry_type: EntryType::File,
                size: Some(32),
                ext: Some("txt".to_string()),
                deleted: false,
                hidden: false,
                system: false,
                encrypted: false,
                created_at: None,
                modified_at: None,
                accessed_at: None,
                changed_at: None,
                hash_sha256: None,
            })
            .collect::<Vec<_>>();
        FileRepo::new(&conn).insert_batch(&entries).unwrap();
        (conn, ids)
    }

    fn setup_case_db_with_source(tmp: &TempDir) -> rusqlite::Connection {
        let conn = persistence_sqlite::connection::open_in_memory().unwrap();
        persistence_sqlite::runner::run_all(&conn).unwrap();
        let case = domain::CaseMeta {
            id: domain::CaseId("case-1".to_string()),
            name: "case".to_string(),
            number: None,
            examiner: None,
            notes: None,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };
        persistence_sqlite::repositories::case_repo::CaseRepo::new(&conn)
            .create(&case)
            .unwrap();
        let ds = domain::DataSource {
            id: DataSourceId("ds-1".to_string()),
            name: "source".to_string(),
            kind: domain::DataSourceKind::LogicalDirectory,
            source_path: tmp.path().join("source"),
            imported_at: chrono::Utc::now(),
            provenance: domain::DataSourceProvenance::unknown(),
        };
        DataSourceRepo::new(&conn)
            .insert(&domain::CaseId("case-1".to_string()), &ds)
            .unwrap();
        conn.execute_batch("UPDATE data_sources SET import_state='ready',platform='linux'")
            .unwrap();
        conn
    }

    fn metric_value(report: &PerformanceReportDto, key: &str) -> Option<f64> {
        report
            .metrics
            .iter()
            .find(|metric| metric.key == key)
            .map(|metric| metric.value)
    }

    #[test]
    fn search_index_instrumentation_reports_bounded_metrics() {
        let (conn, ids) = setup_file_db();
        let tmp = TempDir::new().unwrap();
        let result = index_files_instrumented(&conn, tmp.path(), &ids, |id| {
            Some(Box::new(Cursor::new(format!("alpha beta {}", id.0))))
        })
        .unwrap();
        assert_eq!(result.stats.indexed_count, 2);
        assert_eq!(
            metric_value(&result.performance_report, "search.index.rows"),
            Some(2.0)
        );
        assert!(metric_value(&result.performance_report, "search.index.elapsedMs").is_some());
        assert!(result
            .performance_report
            .metrics
            .iter()
            .all(|metric| !metric.key.contains("path")));
    }

    #[test]
    fn search_query_instrumentation_reports_query_metrics() {
        let (conn, ids) = setup_file_db();
        let tmp = TempDir::new().unwrap();
        index_files_instrumented(&conn, tmp.path(), &ids, |id| {
            Some(Box::new(Cursor::new(format!("needle haystack {}", id.0))))
        })
        .unwrap();

        let result = search_files_real_instrumented(tmp.path(), "needle", 0, 10).unwrap();

        assert_eq!(result.page.items.len(), 2);
        assert_eq!(
            metric_value(&result.performance_report, "search.query.rows"),
            Some(2.0)
        );
        assert_eq!(
            metric_value(&result.performance_report, "search.query.totalRows"),
            Some(2.0)
        );
        assert!(result
            .performance_report
            .summary
            .summary
            .starts_with("Search query returned 2 rows"));
    }

    #[test]
    fn search_files_for_case_reads_source_indexes_and_wraps_file_ids() {
        let tmp = TempDir::new().unwrap();
        let case_conn = setup_case_db_with_source(&tmp);
        let index_dir =
            crate::source_db::source_index_dir(tmp.path(), &DataSourceId("ds-1".to_string()));
        let index = SearchIndex::create(&index_dir).unwrap();
        index
            .index_documents(
                &[ExtractedText {
                    file_id: "file-1".to_string(),
                    content: "needle source scoped content".to_string(),
                    encoding: "utf-8".to_string(),
                    extractable: true,
                    byte_count: 28,
                }],
                &[("file-1".to_string(), "/evidence/file-1.txt".to_string())],
            )
            .unwrap();

        let page = search_files_for_case(
            &case_conn,
            tmp.path(),
            &domain::CaseId("case-1".to_string()),
            "needle",
            0,
            10,
        )
        .unwrap();

        assert_eq!(page.total, 1);
        assert_eq!(page.items[0].file_id, "ds:ds-1:file-1");
        assert_eq!(page.items[0].path, "/evidence/file-1.txt");
    }
}
