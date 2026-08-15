use rusqlite::Connection;
use std::path::Path;
use transport::commands::SearchFilesRequest;
use transport::dto::{PerformanceReportDto, SearchFileResultPageDto};

use crate::performance::{measure_rows, metric, report, PerfSample};

use super::{search_files_for_case, search_files_for_case_cursor, SearchError};

#[derive(Debug, Clone)]
pub struct InstrumentedFileSearchResult {
    pub page: SearchFileResultPageDto,
    pub performance_report: PerformanceReportDto,
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
