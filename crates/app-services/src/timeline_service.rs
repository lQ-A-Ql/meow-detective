use transport::{dto::TimelineEventDto, paging::PageResponse};

use crate::performance::{measure_rows, metric, report, PerfSample};
use domain::FileEntry;
use persistence_sqlite::repositories::timeline_repo::TimelineRepo;
use rayon::prelude::*;
use rusqlite::{params, Connection, OptionalExtension};
use std::time::Instant;
use transport::dto::PerformanceReportDto;

const MACB_PROJECTION_KEY: &str = "macb";

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TimelineProjectionStats {
    pub inserted_count: u64,
    pub elapsed_ms: u128,
    pub already_projected: bool,
}

#[derive(Debug, Clone)]
pub struct InstrumentedPage<T> {
    pub page: PageResponse<T>,
    pub performance_report: PerformanceReportDto,
}

pub fn project_and_store_macb(conn: &Connection, files: &[FileEntry]) -> Result<u64, String> {
    let repo = TimelineRepo::new(conn);

    // Parallel: generate events from all files concurrently
    let all_events: Vec<domain::TimelineEvent> = files
        .par_iter()
        .flat_map_iter(timeline::project_file_macb)
        .collect();

    let count = all_events.len() as u64;
    if !all_events.is_empty() {
        repo.insert_batch(&all_events).map_err(|e| e.to_string())?;
    }
    Ok(count)
}

pub fn ensure_macb_timeline_projected(
    conn: &Connection,
) -> Result<TimelineProjectionStats, String> {
    if !timeline_projection_source_tables_present(conn)? {
        return Ok(TimelineProjectionStats {
            already_projected: true,
            ..TimelineProjectionStats::default()
        });
    }
    ensure_projection_meta_table(conn)?;
    if is_projection_done(conn, MACB_PROJECTION_KEY)? {
        return Ok(TimelineProjectionStats {
            already_projected: true,
            ..TimelineProjectionStats::default()
        });
    }

    let started = Instant::now();
    let inserted = project_macb_timeline_sql(conn)?;
    mark_projection_done(conn, MACB_PROJECTION_KEY, inserted)?;
    Ok(TimelineProjectionStats {
        inserted_count: inserted,
        elapsed_ms: started.elapsed().as_millis(),
        already_projected: false,
    })
}

fn timeline_projection_source_tables_present(conn: &Connection) -> Result<bool, String> {
    let count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master
             WHERE type='table' AND name IN ('file_entries', 'data_sources')",
            [],
            |row| row.get(0),
        )
        .map_err(|e| e.to_string())?;
    Ok(count == 2)
}

/// Query timeline events without filtering.
pub fn query_timeline(
    conn: &Connection,
    offset: u64,
    limit: u32,
) -> Result<PageResponse<TimelineEventDto>, String> {
    ensure_macb_timeline_projected(conn)?;
    let repo = TimelineRepo::new(conn);
    let total = repo.count().map_err(|e| e.to_string())?;
    let events = repo.query(offset, limit).map_err(|e| e.to_string())?;
    let items: Vec<TimelineEventDto> = events
        .into_iter()
        .map(|ev| TimelineEventDto {
            id: ev.id.0,
            source_object_id: ev.source_object_id,
            event_type: ev.event_type,
            ts: ev.timestamp.to_rfc3339(),
            title: ev.title,
            description: ev.description,
            parser_id: ev.parser_id,
            parser_version: ev.parser_version,
            confidence: ev.confidence,
            source_attribution: ev.source_attribution,
            attrs: ev.attrs,
        })
        .collect();
    Ok(PageResponse { total, items })
}

pub fn query_timeline_instrumented(
    conn: &Connection,
    offset: u64,
    limit: u32,
) -> Result<InstrumentedPage<TimelineEventDto>, String> {
    let (page, sample) = measure_rows(0, || query_timeline(conn, offset, limit));
    let page = page?;
    let sample = PerfSample {
        rows: page.items.len() as u64,
        ..sample
    };
    let performance_report = timeline_query_report("timeline.query", sample, page.total);
    Ok(InstrumentedPage {
        page,
        performance_report,
    })
}

/// Query timeline events with optional filtering by time range and event type.
pub fn query_timeline_filtered(
    conn: &Connection,
    offset: u64,
    limit: u32,
    time_start: Option<&str>,
    time_end: Option<&str>,
    event_type: Option<&str>,
) -> Result<PageResponse<TimelineEventDto>, String> {
    ensure_macb_timeline_projected(conn)?;
    let repo = TimelineRepo::new(conn);
    let total = repo
        .count_filtered(time_start, time_end, event_type)
        .map_err(|e| e.to_string())?;
    let events = repo
        .query_filtered(offset, limit, time_start, time_end, event_type)
        .map_err(|e| e.to_string())?;
    let items: Vec<TimelineEventDto> = events
        .into_iter()
        .map(|ev| TimelineEventDto {
            id: ev.id.0,
            source_object_id: ev.source_object_id,
            event_type: ev.event_type,
            ts: ev.timestamp.to_rfc3339(),
            title: ev.title,
            description: ev.description,
            parser_id: ev.parser_id,
            parser_version: ev.parser_version,
            confidence: ev.confidence,
            source_attribution: ev.source_attribution,
            attrs: ev.attrs,
        })
        .collect();
    Ok(PageResponse { total, items })
}

pub fn query_timeline_filtered_instrumented(
    conn: &Connection,
    offset: u64,
    limit: u32,
    time_start: Option<&str>,
    time_end: Option<&str>,
    event_type: Option<&str>,
) -> Result<InstrumentedPage<TimelineEventDto>, String> {
    let (page, sample) = measure_rows(0, || {
        query_timeline_filtered(conn, offset, limit, time_start, time_end, event_type)
    });
    let page = page?;
    let sample = PerfSample {
        rows: page.items.len() as u64,
        ..sample
    };
    let performance_report = timeline_query_report("timeline.query", sample, page.total);
    Ok(InstrumentedPage {
        page,
        performance_report,
    })
}

fn timeline_query_report(prefix: &str, sample: PerfSample, total: u64) -> PerformanceReportDto {
    let mut metrics = vec![
        metric(
            format!("{prefix}.elapsedMs"),
            sample.elapsed_ms as f64,
            "ms",
        ),
        metric(format!("{prefix}.rows"), sample.rows as f64, "rows"),
        metric(format!("{prefix}.totalRows"), total as f64, "rows"),
    ];
    if let Some(rows_per_sec) = sample.rows_per_sec() {
        metrics.push(metric(
            format!("{prefix}.rowsPerSec"),
            rows_per_sec,
            "rows/s",
        ));
    }
    report(
        format!("{prefix}:{}:{}", sample.elapsed_ms, sample.rows),
        None,
        sample.elapsed_ms,
        format!(
            "Timeline query returned {} rows in {} ms",
            sample.rows, sample.elapsed_ms
        ),
        metrics,
    )
}

fn ensure_projection_meta_table(conn: &Connection) -> Result<(), String> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS timeline_projection_meta (
            projection_key TEXT PRIMARY KEY NOT NULL,
            status TEXT NOT NULL,
            inserted_count INTEGER NOT NULL DEFAULT 0,
            updated_at TEXT NOT NULL DEFAULT (datetime('now'))
        );",
    )
    .map_err(|e| e.to_string())
}

fn is_projection_done(conn: &Connection, key: &str) -> Result<bool, String> {
    let status: Option<String> = conn
        .query_row(
            "SELECT status FROM timeline_projection_meta WHERE projection_key = ?1",
            params![key],
            |row| row.get(0),
        )
        .optional()
        .map_err(|e| e.to_string())?;
    Ok(status.as_deref() == Some("done"))
}

fn mark_projection_done(conn: &Connection, key: &str, inserted_count: u64) -> Result<(), String> {
    conn.execute(
        "INSERT INTO timeline_projection_meta (projection_key, status, inserted_count, updated_at)
         VALUES (?1, 'done', ?2, datetime('now'))
         ON CONFLICT(projection_key) DO UPDATE SET
            status = excluded.status,
            inserted_count = excluded.inserted_count,
            updated_at = excluded.updated_at",
        params![key, inserted_count as i64],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

fn project_macb_timeline_sql(conn: &Connection) -> Result<u64, String> {
    let tx = conn
        .unchecked_transaction()
        .map_err(|e| format!("Begin MACB timeline projection: {e}"))?;
    let mut inserted = 0u64;
    inserted += insert_macb_kind_sql(
        &tx,
        "created_at",
        "FILE_CREATED",
        "File created: ",
        " created",
    )?;
    inserted += insert_macb_kind_sql(
        &tx,
        "modified_at",
        "FILE_MODIFIED",
        "File modified: ",
        " modified",
    )?;
    inserted += insert_macb_kind_sql(
        &tx,
        "accessed_at",
        "FILE_ACCESSED",
        "File accessed: ",
        " accessed",
    )?;
    inserted += insert_macb_kind_sql(
        &tx,
        "changed_at",
        "FILE_METADATA_CHANGED",
        "File metadata changed: ",
        " metadata changed",
    )?;
    tx.commit()
        .map_err(|e| format!("Commit MACB timeline projection: {e}"))?;
    Ok(inserted)
}

fn insert_macb_kind_sql(
    conn: &Connection,
    timestamp_column: &str,
    event_type: &str,
    title_prefix: &str,
    description_suffix: &str,
) -> Result<u64, String> {
    let sql = format!(
        "INSERT OR IGNORE INTO timeline_events
         (id, case_id, source_object_id, event_type, ts, title, description, parser_id, source_attribution, attrs)
         SELECT
            'macb:' || fe.id || ':{event_type}',
            ds.case_id,
            fe.id,
            '{event_type}',
            fe.{timestamp_column},
            ?1 || fe.name,
            fe.path || ?2,
            'timeline.macb',
            '{event_type}',
            '{{}}'
         FROM file_entries fe
         JOIN data_sources ds ON ds.id = fe.data_source_id
         WHERE fe.{timestamp_column} IS NOT NULL
         AND LOWER(fe.entry_type) = 'file'
         AND NOT EXISTS (
            SELECT 1 FROM timeline_events existing
            WHERE existing.source_object_id = fe.id
              AND existing.event_type = '{event_type}'
              AND existing.ts = fe.{timestamp_column}
         )",
    );
    conn.execute(&sql, params![title_prefix, description_suffix])
        .map(|count| count as u64)
        .map_err(|e| format!("Insert {event_type} timeline projection: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};
    use domain::{DataSourceId, EntryType, FileEntry, FileEntryId};

    const TIMELINE_SCHEMA: &str =
        include_str!("../../persistence-sqlite/src/migrations/scripts/0005_timeline_events.sql");

    fn in_memory_db_with_timeline() -> rusqlite::Connection {
        let conn = persistence_sqlite::connection::open_in_memory().unwrap();
        conn.execute_batch(TIMELINE_SCHEMA).unwrap();
        conn.execute_batch(
            "ALTER TABLE timeline_events ADD COLUMN parser_id TEXT;
             ALTER TABLE timeline_events ADD COLUMN parser_version TEXT;
             ALTER TABLE timeline_events ADD COLUMN confidence REAL;
             ALTER TABLE timeline_events ADD COLUMN source_attribution TEXT;",
        )
        .unwrap();
        conn
    }

    fn make_file(name: &str, path: &str, created: bool, modified: bool) -> FileEntry {
        FileEntry {
            id: FileEntryId(uuid::Uuid::new_v4().to_string()),
            parent_id: None,
            data_source_id: DataSourceId("ds-1".to_string()),
            path: path.to_string(),
            name: name.to_string(),
            entry_type: EntryType::File,
            size: Some(1024),
            ext: Some("txt".to_string()),
            deleted: false,
            created_at: if created {
                Some(Utc.with_ymd_and_hms(2024, 6, 15, 10, 0, 0).unwrap())
            } else {
                None
            },
            modified_at: if modified {
                Some(Utc.with_ymd_and_hms(2024, 6, 15, 12, 0, 0).unwrap())
            } else {
                None
            },
            accessed_at: Some(Utc.with_ymd_and_hms(2024, 6, 15, 14, 0, 0).unwrap()),
            changed_at: None,
            hash_sha256: None,
        }
    }

    #[test]
    fn project_and_store_macb_inserts_events() {
        let conn = in_memory_db_with_timeline();

        let files = vec![
            make_file("a.txt", "/a.txt", true, true),
            make_file("b.txt", "/b.txt", true, false),
        ];

        let count = project_and_store_macb(&conn, &files).unwrap();
        // a.txt: created + modified + accessed = 3 events
        // b.txt: created + accessed = 2 events
        assert_eq!(count, 5);

        let repo = persistence_sqlite::repositories::timeline_repo::TimelineRepo::new(&conn);
        let total = repo.count().unwrap();
        assert_eq!(total, 5);
    }

    #[test]
    fn project_and_store_macb_empty_files() {
        let conn = in_memory_db_with_timeline();
        let count = project_and_store_macb(&conn, &[]).unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn query_timeline_returns_inserted_events() {
        let conn = in_memory_db_with_timeline();
        let files = vec![make_file("test.txt", "/test.txt", true, true)];
        project_and_store_macb(&conn, &files).unwrap();

        let page = query_timeline(&conn, 0, 100).unwrap();
        assert_eq!(page.items.len(), 3);
        assert_eq!(page.total, 3);
    }

    fn metric_value(report: &PerformanceReportDto, key: &str) -> Option<f64> {
        report
            .metrics
            .iter()
            .find(|metric| metric.key == key)
            .map(|metric| metric.value)
    }

    #[test]
    fn query_timeline_instrumented_reports_bounded_metrics() {
        let conn = in_memory_db_with_timeline();
        let files = vec![make_file("test.txt", "/test.txt", true, true)];
        project_and_store_macb(&conn, &files).unwrap();

        let result = query_timeline_instrumented(&conn, 0, 100).unwrap();

        assert_eq!(result.page.items.len(), 3);
        assert_eq!(
            metric_value(&result.performance_report, "timeline.query.rows"),
            Some(3.0)
        );
        assert_eq!(
            metric_value(&result.performance_report, "timeline.query.totalRows"),
            Some(3.0)
        );
        assert!(metric_value(&result.performance_report, "timeline.query.elapsedMs").is_some());
        assert!(result
            .performance_report
            .metrics
            .iter()
            .all(|metric| !metric.key.contains("path")));
    }

    #[test]
    fn query_timeline_filtered_instrumented_reports_filtered_rows() {
        let conn = in_memory_db_with_timeline();
        let files = vec![make_file("test.txt", "/test.txt", true, true)];
        project_and_store_macb(&conn, &files).unwrap();

        let result =
            query_timeline_filtered_instrumented(&conn, 0, 100, None, None, Some("FILE_CREATED"))
                .unwrap();

        assert_eq!(result.page.items.len(), 1);
        assert_eq!(
            metric_value(&result.performance_report, "timeline.query.rows"),
            Some(1.0)
        );
        assert_eq!(
            metric_value(&result.performance_report, "timeline.query.totalRows"),
            Some(1.0)
        );
    }

    #[test]
    fn ensure_macb_timeline_projected_is_lazy_and_idempotent() {
        let conn = persistence_sqlite::connection::open_in_memory().unwrap();
        conn.execute_batch(TIMELINE_SCHEMA).unwrap();
        conn.execute_batch(
            "ALTER TABLE timeline_events ADD COLUMN parser_id TEXT;
             ALTER TABLE timeline_events ADD COLUMN parser_version TEXT;
             ALTER TABLE timeline_events ADD COLUMN confidence REAL;
             ALTER TABLE timeline_events ADD COLUMN source_attribution TEXT;",
        )
        .unwrap();
        conn.execute_batch(
            "CREATE TABLE data_sources (
                id TEXT PRIMARY KEY NOT NULL,
                case_id TEXT NOT NULL,
                name TEXT NOT NULL,
                kind TEXT NOT NULL,
                source_path TEXT NOT NULL,
                size INTEGER,
                imported_at TEXT NOT NULL DEFAULT ''
            );
            CREATE TABLE file_entries (
                id TEXT PRIMARY KEY NOT NULL,
                parent_id TEXT,
                data_source_id TEXT NOT NULL,
                path TEXT NOT NULL,
                name TEXT NOT NULL,
                entry_type TEXT NOT NULL,
                size INTEGER,
                ext TEXT,
                deleted INTEGER NOT NULL DEFAULT 0,
                created_at TEXT,
                modified_at TEXT,
                accessed_at TEXT,
                changed_at TEXT,
                hash_sha256 TEXT
            );",
        )
        .unwrap();
        conn.execute(
            "INSERT INTO data_sources (id, case_id, name, kind, source_path)
             VALUES ('ds-1', 'case-1', 'sample', 'Raw', '/sample.raw')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO file_entries
             (id, data_source_id, path, name, entry_type, created_at, modified_at, accessed_at)
             VALUES ('file-1', 'ds-1', '/file.txt', 'file.txt', 'file',
                     '2026-01-01T00:00:00Z', '2026-01-02T00:00:00Z', '2026-01-03T00:00:00Z')",
            [],
        )
        .unwrap();

        let stats = ensure_macb_timeline_projected(&conn).unwrap();
        assert_eq!(stats.inserted_count, 3);
        assert!(!stats.already_projected);
        let second = ensure_macb_timeline_projected(&conn).unwrap();
        assert_eq!(second.inserted_count, 0);
        assert!(second.already_projected);

        let page = query_timeline(&conn, 0, 100).unwrap();
        assert_eq!(page.total, 3);
        assert!(page
            .items
            .iter()
            .any(|event| event.id == "macb:file-1:FILE_CREATED"));
    }
}
