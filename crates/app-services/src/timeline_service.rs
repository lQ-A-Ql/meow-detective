use chrono::Utc;
use std::collections::HashMap;
use thiserror::Error;
use transport::{
    dto::{
        PerformanceReportDto, TimelineAggregatedDto, TimelineClusterDto, TimelineEventDto,
        TimelineStripeDto,
    },
    paging::PageResponse,
};

use crate::performance::{measure_rows, metric, report, PerfSample};
use crate::source_db::{self, encode_source_scoped_id};
use domain::{EdgeType, FileEntry, GraphEdge, GraphNode, NodeType};
use persistence_sqlite::repositories::{graph_repo::GraphRepo, timeline_repo::TimelineRepo};
use rayon::prelude::*;
use rusqlite::{params, Connection, OptionalExtension};
use std::path::Path;
use std::time::Instant;

mod query;
pub use query::TimelineQuery;
const MACB_PROJECTION_KEY: &str = "macb";

#[derive(Debug, Error)]
pub enum TimelineServiceError {
    #[error("database error: {0}")]
    Db(#[from] persistence_sqlite::DbError),
    #[error("not found: {0}")]
    NotFound(String),
    #[error("invalid input: {0}")]
    InvalidInput(String),
    #[error("unsupported: {0}")]
    Unsupported(String),
    #[error("{0}")]
    Other(String),
}

impl transport::ServiceErrorCategory for TimelineServiceError {
    fn category(&self) -> transport::ErrorCategory {
        match self {
            Self::Db(_) => transport::ErrorCategory::Io,
            Self::NotFound(_) | Self::InvalidInput(_) => transport::ErrorCategory::Validation,
            Self::Unsupported(_) => transport::ErrorCategory::Unsupported,
            Self::Other(_) => transport::ErrorCategory::Internal,
        }
    }
}

impl From<rusqlite::Error> for TimelineServiceError {
    fn from(e: rusqlite::Error) -> Self {
        Self::Db(persistence_sqlite::DbError::from(e))
    }
}

impl From<crate::source_db::ReadySourceError> for TimelineServiceError {
    fn from(error: crate::source_db::ReadySourceError) -> Self {
        match error {
            crate::source_db::ReadySourceError::Db(error) => Self::Db(error),
            crate::source_db::ReadySourceError::NotFound { .. } => {
                Self::NotFound(error.to_string())
            }
            crate::source_db::ReadySourceError::NotReady { .. } => {
                Self::InvalidInput(error.to_string())
            }
            crate::source_db::ReadySourceError::UnsupportedPlatform { .. } => {
                Self::Unsupported(error.to_string())
            }
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TimelineProjectionStats {
    pub inserted_count: u64,
    pub elapsed_ms: u128,
    pub already_projected: bool,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct InstrumentedPage<T> {
    pub page: PageResponse<T>,
    pub performance_report: PerformanceReportDto,
}

pub fn project_and_store_macb(
    conn: &Connection,
    files: &[FileEntry],
) -> Result<u64, TimelineServiceError> {
    let repo = TimelineRepo::new(conn);

    // Parallel: generate events from all files concurrently
    let all_events: Vec<domain::TimelineEvent> = files
        .par_iter()
        .flat_map_iter(timeline::project_file_macb)
        .collect();

    let count = all_events.len() as u64;
    if !all_events.is_empty() {
        repo.insert_batch(&all_events)?;
    }
    Ok(count)
}

pub fn ensure_macb_timeline_projected(
    conn: &Connection,
) -> Result<TimelineProjectionStats, TimelineServiceError> {
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

    // Populate investigative graph: TimelineEvent nodes and References edges
    let mut graph_warnings = Vec::new();
    if inserted > 0 {
        match populate_timeline_event_graph(conn) {
            Ok(warnings) => graph_warnings = warnings,
            Err(err) => {
                let message = format!("Timeline graph population failed: {err}");
                tracing::warn!("{}", message);
                graph_warnings.push(message);
            }
        }
    }

    Ok(TimelineProjectionStats {
        inserted_count: inserted,
        elapsed_ms: started.elapsed().as_millis(),
        already_projected: false,
        warnings: graph_warnings,
    })
}

fn timeline_projection_source_tables_present(
    conn: &Connection,
) -> Result<bool, TimelineServiceError> {
    let count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM sqlite_master
             WHERE type='table' AND name IN ('file_entries', 'data_sources')",
        [],
        |row| row.get(0),
    )?;
    Ok(count == 2)
}

/// Query timeline events without filtering.
pub fn query_timeline(
    conn: &Connection,
    offset: u64,
    limit: u32,
) -> Result<PageResponse<TimelineEventDto>, TimelineServiceError> {
    ensure_macb_timeline_projected(conn)?;
    let repo = TimelineRepo::new(conn);
    let total = repo.count()?;
    let events = repo.query(offset, limit)?;
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

pub fn query_timeline_for_case(
    case_conn: &Connection,
    case_root: &Path,
    case_id: &domain::CaseId,
    offset: u64,
    limit: u32,
) -> Result<PageResponse<TimelineEventDto>, TimelineServiceError> {
    query_timeline_filtered_for_case(
        case_conn,
        case_root,
        case_id,
        TimelineQuery::unfiltered(offset, limit),
    )
}

pub fn query_timeline_for_case_instrumented(
    case_conn: &Connection,
    case_root: &Path,
    case_id: &domain::CaseId,
    offset: u64,
    limit: u32,
) -> Result<InstrumentedPage<TimelineEventDto>, TimelineServiceError> {
    let (page, sample) = measure_rows(0, || {
        query_timeline_for_case(case_conn, case_root, case_id, offset, limit)
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

pub fn query_timeline_instrumented(
    conn: &Connection,
    offset: u64,
    limit: u32,
) -> Result<InstrumentedPage<TimelineEventDto>, TimelineServiceError> {
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
) -> Result<PageResponse<TimelineEventDto>, TimelineServiceError> {
    ensure_macb_timeline_projected(conn)?;
    let repo = TimelineRepo::new(conn);
    let total = repo.count_filtered(time_start, time_end, event_type)?;
    let events = repo.query_filtered(offset, limit, time_start, time_end, event_type)?;
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

pub fn query_timeline_filtered_for_case(
    case_conn: &Connection,
    case_root: &Path,
    case_id: &domain::CaseId,
    query: TimelineQuery<'_>,
) -> Result<PageResponse<TimelineEventDto>, TimelineServiceError> {
    let offset = query.offset;
    let limit = query.limit;
    let time_start = query.time_start;
    let time_end = query.time_end;
    let event_type = query.event_type;
    let mut total = 0u64;
    let mut events = Vec::new();
    let per_source_limit = offset.saturating_add(limit as u64).min(u32::MAX as u64) as u32;

    for (data_source_id, source_conn) in
        source_db::open_ready_source_connections(case_conn, case_root, case_id)?
    {
        ensure_macb_timeline_projected(&source_conn)?;
        let repo = TimelineRepo::new(&source_conn);
        total = total.saturating_add(repo.count_filtered(time_start, time_end, event_type)?);
        for event in repo.query_filtered(0, per_source_limit, time_start, time_end, event_type)? {
            events.push((data_source_id.clone(), event));
        }
    }

    events.sort_by(|(left_source, left), (right_source, right)| {
        right
            .timestamp
            .cmp(&left.timestamp)
            .then_with(|| left_source.0.cmp(&right_source.0))
            .then_with(|| left.id.0.cmp(&right.id.0))
    });

    let items = events
        .into_iter()
        .skip(offset as usize)
        .take(limit as usize)
        .map(|(data_source_id, event)| timeline_event_to_source_dto(event, &data_source_id))
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
) -> Result<InstrumentedPage<TimelineEventDto>, TimelineServiceError> {
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

pub fn query_timeline_filtered_for_case_instrumented(
    case_conn: &Connection,
    case_root: &Path,
    case_id: &domain::CaseId,
    query: TimelineQuery<'_>,
) -> Result<InstrumentedPage<TimelineEventDto>, TimelineServiceError> {
    let (page, sample) = measure_rows(0, || {
        query_timeline_filtered_for_case(case_conn, case_root, case_id, query)
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

/// Query timeline events in aggregated form, grouped by (event_type, description).
///
/// `offset` and `limit` apply to the number of resulting **clusters**, not raw events.
/// The returned `TimelineAggregatedDto` maps each distinct `event_type` to a
/// `TimelineStripeDto` with all of its clusters and the total event count for that type.
pub fn query_timeline_aggregated(
    conn: &Connection,
    offset: u64,
    limit: u32,
) -> Result<TimelineAggregatedDto, TimelineServiceError> {
    ensure_macb_timeline_projected(conn)?;

    // Query clusters: group by (event_type, description), paginate cluster count
    let cluster_rows: Vec<(String, String, i64, String, String, String)> = {
        let mut stmt = conn.prepare(
            "SELECT event_type, description, COUNT(*) AS cnt,
                        MIN(ts) AS first_ts, MAX(ts) AS last_ts,
                        GROUP_CONCAT(id, ',') AS sample_ids
                 FROM timeline_events
                 GROUP BY event_type, description
                 ORDER BY cnt DESC, event_type ASC, description ASC
                 LIMIT ?1 OFFSET ?2",
        )?;
        let rows = stmt
            .query_map(params![limit, offset], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        rows
    };

    // Collect distinct event_types present in this page
    let mut event_types: Vec<String> = cluster_rows
        .iter()
        .map(|(et, _, _, _, _, _)| et.clone())
        .collect();
    event_types.sort();
    event_types.dedup();

    // Fetch the true total event count per event_type for the types that appear
    let totals = query_totals_by_type(conn, &event_types)?;

    // Build stripes keyed by event_type, seeded with the true total
    let mut stripes_by_type: HashMap<String, TimelineStripeDto> = HashMap::new();
    for (et, total) in &totals {
        stripes_by_type.insert(
            et.clone(),
            TimelineStripeDto {
                clusters: Vec::new(),
                total_events: *total,
            },
        );
    }

    // Populate clusters into their respective stripes
    for (event_type, description, count, first_ts, last_ts, sample_ids_str) in &cluster_rows {
        let sample_event_ids: Vec<String> = sample_ids_str
            .split(',')
            .take(5)
            .map(|s| s.to_string())
            .collect();

        let cluster = TimelineClusterDto {
            event_type: event_type.clone(),
            description: description.clone(),
            count: *count as u64,
            first_ts: first_ts.clone(),
            last_ts: last_ts.clone(),
            sample_event_ids,
        };

        stripes_by_type
            .entry(event_type.clone())
            .or_insert_with(|| TimelineStripeDto {
                clusters: Vec::new(),
                total_events: 0,
            })
            .clusters
            .push(cluster);
    }

    Ok(TimelineAggregatedDto { stripes_by_type })
}

fn query_totals_by_type(
    conn: &Connection,
    event_types: &[String],
) -> Result<Vec<(String, u64)>, TimelineServiceError> {
    if event_types.is_empty() {
        return Ok(Vec::new());
    }

    let placeholders: Vec<String> = (1..=event_types.len()).map(|i| format!("?{i}")).collect();
    let in_clause = placeholders.join(",");
    let sql = format!(
        "SELECT event_type, COUNT(*) AS total
         FROM timeline_events
         WHERE event_type IN ({in_clause})
         GROUP BY event_type"
    );

    let mut stmt = conn.prepare(&sql)?;
    let params_refs: Vec<&dyn rusqlite::types::ToSql> = event_types
        .iter()
        .map(|s| s as &dyn rusqlite::types::ToSql)
        .collect();

    let rows = stmt
        .query_map(params_refs.as_slice(), |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)? as u64))
        })?
        .collect::<Result<Vec<_>, _>>()?;

    Ok(rows)
}

pub fn get_timeline_event_by_id(
    conn: &Connection,
    event_id: &str,
) -> Result<Option<TimelineEventDto>, TimelineServiceError> {
    ensure_macb_timeline_projected(conn)?;
    let repo = TimelineRepo::new(conn);
    let event = repo.find_by_id(event_id)?;
    Ok(event.map(|ev| TimelineEventDto {
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
    }))
}

pub fn get_timeline_event_by_id_for_case(
    case_conn: &Connection,
    case_root: &Path,
    case_id: &domain::CaseId,
    event_id: &str,
) -> Result<Option<TimelineEventDto>, TimelineServiceError> {
    let (data_source_id, local_id) =
        source_db::parse_source_scoped_id("Timeline event id", event_id).map_err(|err| {
            TimelineServiceError::InvalidInput(format!(
                "{err}; source database timeline events require ds:<dataSourceId>:<localId>"
            ))
        })?;
    let source =
        source_db::open_ready_source_by_id(case_conn, case_root, case_id, &data_source_id)?;
    ensure_macb_timeline_projected(&source.connection)?;
    Ok(TimelineRepo::new(&source.connection)
        .find_by_id(&local_id)?
        .map(|event| timeline_event_to_source_dto(event, &data_source_id)))
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

fn timeline_event_to_source_dto(
    ev: domain::TimelineEvent,
    data_source_id: &domain::DataSourceId,
) -> TimelineEventDto {
    TimelineEventDto {
        id: encode_source_scoped_id(data_source_id, &ev.id.0),
        source_object_id: if ev.source_object_id.is_empty() {
            ev.source_object_id
        } else {
            encode_source_scoped_id(data_source_id, &ev.source_object_id)
        },
        event_type: ev.event_type,
        ts: ev.timestamp.to_rfc3339(),
        title: ev.title,
        description: ev.description,
        parser_id: ev.parser_id,
        parser_version: ev.parser_version,
        confidence: ev.confidence,
        source_attribution: ev.source_attribution,
        attrs: ev.attrs,
    }
}

fn ensure_projection_meta_table(conn: &Connection) -> Result<(), TimelineServiceError> {
    Ok(conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS timeline_projection_meta (
            projection_key TEXT PRIMARY KEY NOT NULL,
            status TEXT NOT NULL,
            inserted_count INTEGER NOT NULL DEFAULT 0,
            updated_at TEXT NOT NULL DEFAULT (datetime('now'))
        );",
    )?)
}

fn is_projection_done(conn: &Connection, key: &str) -> Result<bool, TimelineServiceError> {
    let status: Option<String> = conn
        .query_row(
            "SELECT status FROM timeline_projection_meta WHERE projection_key = ?1",
            params![key],
            |row| row.get(0),
        )
        .optional()?;
    Ok(status.as_deref() == Some("done"))
}

fn mark_projection_done(
    conn: &Connection,
    key: &str,
    inserted_count: u64,
) -> Result<(), TimelineServiceError> {
    conn.execute(
        "INSERT INTO timeline_projection_meta (projection_key, status, inserted_count, updated_at)
         VALUES (?1, 'done', ?2, datetime('now'))
         ON CONFLICT(projection_key) DO UPDATE SET
            status = excluded.status,
            inserted_count = excluded.inserted_count,
            updated_at = excluded.updated_at",
        params![key, inserted_count as i64],
    )?;
    Ok(())
}

fn project_macb_timeline_sql(conn: &Connection) -> Result<u64, TimelineServiceError> {
    let tx = conn
        .unchecked_transaction()
        .map_err(|e| TimelineServiceError::Other(format!("Begin MACB timeline projection: {e}")))?;
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
    tx.commit().map_err(|e| {
        TimelineServiceError::Other(format!("Commit MACB timeline projection: {e}"))
    })?;
    Ok(inserted)
}

fn insert_macb_kind_sql(
    conn: &Connection,
    timestamp_column: &str,
    event_type: &str,
    title_prefix: &str,
    description_suffix: &str,
) -> Result<u64, TimelineServiceError> {
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
        .map_err(|e| {
            TimelineServiceError::Other(format!("Insert {event_type} timeline projection: {e}"))
        })
}

/// Write TimelineEvent graph nodes and References edges for all timeline events
/// in the current case. Called after MACB timeline projection inserts new events.
///
/// Returns any non-fatal warnings (e.g., events skipped because their
/// `source_object_id` is empty). Hard failures are returned as `Err`.
fn populate_timeline_event_graph(conn: &Connection) -> Result<Vec<String>, TimelineServiceError> {
    let case_id: String = conn
        .query_row(
            "SELECT DISTINCT case_id FROM timeline_events LIMIT 1",
            [],
            |row| row.get(0),
        )
        .map_err(|e| {
            TimelineServiceError::Other(format!("resolve case_id for timeline graph: {e}"))
        })?;

    let graph_repo = GraphRepo::new(conn);
    let now = Utc::now().to_rfc3339();
    let mut warnings = Vec::new();
    let mut skipped_empty_source: u64 = 0;

    const TIMELINE_GRAPH_BATCH: u32 = 5000;
    const GRAPH_WRITE_CHUNK: usize = 2000;
    let mut offset = 0u64;

    loop {
        let mut stmt = conn
            .prepare(
                "SELECT id, source_object_id, event_type, title, description, confidence, parser_id
                 FROM timeline_events
                 WHERE case_id = ?1
                 LIMIT ?2 OFFSET ?3",
            )
            .map_err(|e| {
                TimelineServiceError::Other(format!("prepare timeline graph query: {e}"))
            })?;

        #[allow(clippy::type_complexity)]
        let rows: Vec<(
            String,
            String,
            String,
            String,
            String,
            Option<f64>,
            Option<String>,
        )> = stmt
            .query_map(
                rusqlite::params![case_id, TIMELINE_GRAPH_BATCH, offset],
                |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                        row.get(5)?,
                        row.get(6)?,
                    ))
                },
            )
            .map_err(|e| {
                TimelineServiceError::Other(format!("query timeline events for graph: {e}"))
            })?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| TimelineServiceError::Other(format!("collect timeline rows: {e}")))?;

        if rows.is_empty() {
            break;
        }

        let row_count = rows.len() as u64;

        let mut nodes = Vec::with_capacity(rows.len());
        let mut edges = Vec::with_capacity(rows.len());

        for (id, source_object_id, event_type, title, _description, confidence, _parser_id) in &rows
        {
            nodes.push(GraphNode {
                id: id.clone(),
                case_id: case_id.clone(),
                node_type: NodeType::TimelineEvent,
                label: title.clone(),
                summary: event_type.clone(),
                tags: Vec::new(),
                created_at: now.clone(),
            });

            if !source_object_id.is_empty() {
                edges.push(GraphEdge {
                    id: format!("references:{}:{}", id, source_object_id),
                    case_id: case_id.clone(),
                    source_id: id.clone(),
                    target_id: source_object_id.clone(),
                    edge_type: EdgeType::References,
                    confidence: *confidence,
                    provenance: Some(format!("timeline.macb:{event_type}")),
                    created_at: now.clone(),
                });
            } else {
                skipped_empty_source += 1;
            }
        }

        for node_chunk in nodes.chunks(GRAPH_WRITE_CHUNK) {
            graph_repo.insert_nodes_batch(node_chunk).map_err(|e| {
                TimelineServiceError::Other(format!("timeline graph node insert: {e}"))
            })?;
        }
        for edge_chunk in edges.chunks(GRAPH_WRITE_CHUNK) {
            graph_repo.insert_edges_batch(edge_chunk).map_err(|e| {
                TimelineServiceError::Other(format!("timeline graph edge insert: {e}"))
            })?;
        }

        offset += row_count;
    }

    if skipped_empty_source > 0 {
        warnings.push(format!(
            "{skipped_empty_source} timeline event(s) skipped because source_object_id was empty; no References edges were created"
        ));
    }

    Ok(warnings)
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

    fn in_memory_case_db_with_source() -> rusqlite::Connection {
        let conn = persistence_sqlite::connection::open_in_memory().unwrap();
        persistence_sqlite::runner::run_all(&conn).unwrap();
        let case = domain::CaseMeta {
            id: domain::CaseId("case-1".to_string()),
            name: "case".to_string(),
            number: None,
            examiner: None,
            notes: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        persistence_sqlite::repositories::case_repo::CaseRepo::new(&conn)
            .create(&case)
            .unwrap();
        let ds = domain::DataSource {
            id: DataSourceId("ds-1".to_string()),
            name: "source".to_string(),
            kind: domain::DataSourceKind::LogicalDirectory,
            source_path: std::path::PathBuf::from("D:/source"),
            imported_at: Utc::now(),
            provenance: domain::DataSourceProvenance::unknown(),
        };
        persistence_sqlite::repositories::datasource_repo::DataSourceRepo::new(&conn)
            .insert(&domain::CaseId("case-1".to_string()), &ds)
            .unwrap();
        conn.execute_batch("UPDATE data_sources SET import_state='ready',platform='linux'")
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
            hidden: false,
            system: false,
            encrypted: false,
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

    #[test]
    fn query_timeline_for_case_reads_source_databases_and_wraps_ids() {
        let tmp = tempfile::TempDir::new().unwrap();
        let case_conn = in_memory_case_db_with_source();
        let ds_id = DataSourceId("ds-1".to_string());
        let source_conn = crate::source_db::open_source_db(tmp.path(), &ds_id).unwrap();
        let event = domain::TimelineEvent {
            id: domain::TimelineEventId("event-1".to_string()),
            source_object_id: "file-1".to_string(),
            event_type: "FILE_CREATED".to_string(),
            timestamp: Utc.with_ymd_and_hms(2026, 1, 1, 12, 0, 0).unwrap(),
            title: "created".to_string(),
            description: "created file".to_string(),
            parser_id: Some("test.parser".to_string()),
            parser_version: None,
            confidence: Some(1.0),
            source_attribution: None,
            attrs: Default::default(),
        };
        TimelineRepo::new(&source_conn)
            .insert_batch_with_case(&[event], "case-1")
            .unwrap();

        let page = query_timeline_for_case(
            &case_conn,
            tmp.path(),
            &domain::CaseId("case-1".to_string()),
            0,
            10,
        )
        .unwrap();

        assert_eq!(page.total, 1);
        assert_eq!(page.items[0].id, "ds:ds-1:event-1");
        assert_eq!(page.items[0].source_object_id, "ds:ds-1:file-1");

        let event = get_timeline_event_by_id_for_case(
            &case_conn,
            tmp.path(),
            &domain::CaseId("case-1".to_string()),
            "ds:ds-1:event-1",
        )
        .unwrap()
        .expect("timeline event");
        assert_eq!(event.event_type, "FILE_CREATED");
    }

    #[test]
    fn get_timeline_event_by_id_for_case_rejects_unscoped_ids() {
        let tmp = tempfile::TempDir::new().unwrap();
        let case_conn = in_memory_case_db_with_source();

        let err = get_timeline_event_by_id_for_case(
            &case_conn,
            tmp.path(),
            &domain::CaseId("case-1".to_string()),
            "event-1",
        )
        .unwrap_err();

        assert!(matches!(err, TimelineServiceError::InvalidInput(_)));
        assert!(err.to_string().contains("ds:<dataSourceId>:<localId>"));
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

    // ── Aggregation tests ──────────────────────────────────────────────

    fn insert_events(conn: &rusqlite::Connection, rows: &[(&str, &str, &str, &str)]) {
        let repo = persistence_sqlite::repositories::timeline_repo::TimelineRepo::new(conn);
        let events: Vec<domain::TimelineEvent> = rows
            .iter()
            .map(|(id, event_type, description, ts)| domain::TimelineEvent {
                id: domain::TimelineEventId(id.to_string()),
                source_object_id: "src-1".to_string(),
                event_type: event_type.to_string(),
                timestamp: chrono::DateTime::parse_from_rfc3339(ts)
                    .unwrap()
                    .with_timezone(&Utc),
                title: format!("{event_type} event"),
                description: description.to_string(),
                parser_id: None,
                parser_version: None,
                confidence: None,
                source_attribution: None,
                attrs: std::collections::BTreeMap::new(),
            })
            .collect();
        repo.insert_batch(&events).unwrap();
    }

    #[test]
    fn aggregate_groups_by_event_type() {
        let conn = in_memory_db_with_timeline();
        insert_events(
            &conn,
            &[
                (
                    "e1",
                    "FILE_CREATED",
                    "File created: /a.txt",
                    "2025-01-01T10:00:00Z",
                ),
                (
                    "e2",
                    "FILE_CREATED",
                    "File created: /b.txt",
                    "2025-01-01T11:00:00Z",
                ),
                (
                    "e3",
                    "FILE_MODIFIED",
                    "File modified: /a.txt",
                    "2025-01-01T12:00:00Z",
                ),
                (
                    "e4",
                    "FILE_MODIFIED",
                    "File modified: /a.txt",
                    "2025-01-01T12:30:00Z",
                ),
                (
                    "e5",
                    "FILE_ACCESSED",
                    "File accessed: /c.txt",
                    "2025-01-01T13:00:00Z",
                ),
            ],
        );

        let result = query_timeline_aggregated(&conn, 0, 50).unwrap();
        let stripes = &result.stripes_by_type;

        // Three distinct event types
        assert_eq!(stripes.len(), 3);
        assert!(stripes.contains_key("FILE_CREATED"));
        assert!(stripes.contains_key("FILE_MODIFIED"));
        assert!(stripes.contains_key("FILE_ACCESSED"));

        // FILE_CREATED stripe: 2 events across 2 clusters, one per description
        let created = &stripes["FILE_CREATED"];
        assert_eq!(created.total_events, 2);
        assert_eq!(created.clusters.len(), 2);
        let descriptions: Vec<&str> = created
            .clusters
            .iter()
            .map(|c| c.description.as_str())
            .collect();
        assert!(descriptions.contains(&"File created: /a.txt"));
        assert!(descriptions.contains(&"File created: /b.txt"));

        // FILE_MODIFIED stripe: 2 events, same description => 1 cluster
        let modified = &stripes["FILE_MODIFIED"];
        assert_eq!(modified.total_events, 2);
        assert_eq!(modified.clusters.len(), 1);
        assert_eq!(modified.clusters[0].description, "File modified: /a.txt");
        assert_eq!(modified.clusters[0].count, 2);

        // FILE_ACCESSED stripe: 1 event, 1 cluster
        let accessed = &stripes["FILE_ACCESSED"];
        assert_eq!(accessed.total_events, 1);
        assert_eq!(accessed.clusters.len(), 1);
        assert_eq!(accessed.clusters[0].count, 1);
    }

    #[test]
    fn cluster_contains_correct_count_and_range() {
        let conn = in_memory_db_with_timeline();
        // Same (event_type, description) across three timestamps
        insert_events(
            &conn,
            &[
                (
                    "e1",
                    "FILE_MODIFIED",
                    "File modified: /shared.txt",
                    "2025-06-01T08:00:00Z",
                ),
                (
                    "e2",
                    "FILE_MODIFIED",
                    "File modified: /shared.txt",
                    "2025-06-02T12:00:00Z",
                ),
                (
                    "e3",
                    "FILE_MODIFIED",
                    "File modified: /shared.txt",
                    "2025-06-03T16:00:00Z",
                ),
            ],
        );

        let result = query_timeline_aggregated(&conn, 0, 10).unwrap();
        let modified = &result.stripes_by_type["FILE_MODIFIED"];
        assert_eq!(modified.total_events, 3);
        assert_eq!(modified.clusters.len(), 1);

        let cluster = &modified.clusters[0];
        assert_eq!(cluster.count, 3);
        // first_ts = MIN(ts), last_ts = MAX(ts)
        assert!(cluster.first_ts.starts_with("2025-06-01T08:00:00"));
        assert!(cluster.last_ts.starts_with("2025-06-03T16:00:00"));
        // Sample IDs: GROUP_CONCAT then split, at most 5
        assert!(!cluster.sample_event_ids.is_empty());
        assert!(cluster.sample_event_ids.len() <= 5);
        // Every sample ID should be among the inserted IDs
        let expected_ids: Vec<&str> = vec!["e1", "e2", "e3"];
        for sid in &cluster.sample_event_ids {
            assert!(expected_ids.contains(&sid.as_str()));
        }
    }

    #[test]
    fn large_timeline_aggregation_is_fast() {
        let conn = in_memory_db_with_timeline();
        let total = 10_000u32;

        // Build 10K events: 4 types, 100 unique descriptions each => ~400 clusters
        let event_types = [
            "FILE_CREATED",
            "FILE_MODIFIED",
            "FILE_ACCESSED",
            "FILE_METADATA_CHANGED",
        ];
        let mut events: Vec<domain::TimelineEvent> = Vec::with_capacity(total as usize);
        for i in 0..total {
            let et = event_types[(i as usize) % event_types.len()];
            let desc_idx = i % 100;
            let desc = format!("Test: /path/{desc_idx}.txt");
            let hour = i % 24;
            let ts = format!("2025-06-01T{hour:02}:00:00Z");
            events.push(domain::TimelineEvent {
                id: domain::TimelineEventId(format!("e{i:05}")),
                source_object_id: "src-1".to_string(),
                event_type: et.to_string(),
                timestamp: chrono::DateTime::parse_from_rfc3339(&ts)
                    .unwrap()
                    .with_timezone(&Utc),
                title: format!("{et} event"),
                description: desc,
                parser_id: None,
                parser_version: None,
                confidence: None,
                source_attribution: None,
                attrs: std::collections::BTreeMap::new(),
            });
        }

        let repo = persistence_sqlite::repositories::timeline_repo::TimelineRepo::new(&conn);
        let started = std::time::Instant::now();
        repo.insert_batch(&events).unwrap();

        // Aggregate with a cluster limit smaller than total clusters
        let result = query_timeline_aggregated(&conn, 0, 20).unwrap();
        let elapsed = started.elapsed();

        // Should return at most 20 clusters (across all types)
        let total_clusters: usize = result
            .stripes_by_type
            .values()
            .map(|s| s.clusters.len())
            .sum();
        assert!(
            total_clusters <= 20,
            "expected ≤ 20 clusters, got {total_clusters}"
        );

        // Each stripe should report its true total, which is much larger than cluster count
        for stripe in result.stripes_by_type.values() {
            assert!(
                stripe.total_events >= stripe.clusters.len() as u64,
                "total_events must be at least the returned cluster count"
            );
        }

        // The aggregation itself should be fast (well under 5 seconds for 10K rows)
        assert!(
            elapsed.as_millis() < 5000,
            "aggregation took {} ms; expected < 5000 ms",
            elapsed.as_millis()
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
            hidden INTEGER NOT NULL DEFAULT 0,
            system INTEGER NOT NULL DEFAULT 0,
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
