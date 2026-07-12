use persistence_sqlite::repositories::timeline_repo::TimelineRepo;
use rusqlite::{params, Connection};
use std::collections::HashMap;
use std::path::Path;
use transport::{
    dto::{TimelineAggregatedDto, TimelineClusterDto, TimelineEventDto, TimelineStripeDto},
    paging::PageResponse,
};

use super::export::timeline_event_to_source_dto;
use super::projection::ensure_macb_timeline_projected;
use super::{TimelineQuery, TimelineServiceError};
use crate::source_db;

struct ClusterRow {
    event_type: String,
    description: String,
    count: u64,
    first_ts: String,
    last_ts: String,
    sample_ids: String,
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

pub fn query_timeline_filtered_for_case(
    case_conn: &Connection,
    case_root: &Path,
    case_id: &domain::CaseId,
    query: TimelineQuery<'_>,
) -> Result<PageResponse<TimelineEventDto>, TimelineServiceError> {
    let mut total = 0u64;
    let mut events = Vec::new();
    let per_source_limit = query
        .offset
        .saturating_add(query.limit as u64)
        .min(u32::MAX as u64) as u32;

    for (source_id, source_conn) in
        source_db::open_ready_source_connections(case_conn, case_root, case_id)?
    {
        ensure_macb_timeline_projected(&source_conn)?;
        let repo = TimelineRepo::new(&source_conn);
        total = total.saturating_add(repo.count_filtered(
            query.time_start,
            query.time_end,
            query.event_type,
        )?);
        let source_events = repo.query_filtered(
            0,
            per_source_limit,
            query.time_start,
            query.time_end,
            query.event_type,
        )?;
        events.extend(
            source_events
                .into_iter()
                .map(|event| (source_id.clone(), event)),
        );
    }

    sort_source_events(&mut events);
    let items = events
        .into_iter()
        .skip(query.offset as usize)
        .take(query.limit as usize)
        .map(|(source_id, event)| timeline_event_to_source_dto(event, &source_id))
        .collect();
    Ok(PageResponse { total, items })
}

fn sort_source_events(events: &mut [(domain::DataSourceId, domain::TimelineEvent)]) {
    events.sort_by(|(left_source, left), (right_source, right)| {
        right
            .timestamp
            .cmp(&left.timestamp)
            .then_with(|| left_source.0.cmp(&right_source.0))
            .then_with(|| left.id.0.cmp(&right.id.0))
    });
}

pub fn query_timeline_aggregated(
    conn: &Connection,
    offset: u64,
    limit: u32,
) -> Result<TimelineAggregatedDto, TimelineServiceError> {
    ensure_macb_timeline_projected(conn)?;
    let rows = query_cluster_rows(conn, offset, limit)?;
    let event_types = distinct_event_types(&rows);
    let totals = query_totals_by_type(conn, &event_types)?;
    let mut stripes_by_type = seed_stripes(totals);
    append_clusters(&mut stripes_by_type, rows);
    Ok(TimelineAggregatedDto { stripes_by_type })
}

fn query_cluster_rows(
    conn: &Connection,
    offset: u64,
    limit: u32,
) -> Result<Vec<ClusterRow>, TimelineServiceError> {
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
            Ok(ClusterRow {
                event_type: row.get(0)?,
                description: row.get(1)?,
                count: row.get::<_, i64>(2)? as u64,
                first_ts: row.get(3)?,
                last_ts: row.get(4)?,
                sample_ids: row.get(5)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

fn distinct_event_types(rows: &[ClusterRow]) -> Vec<String> {
    let mut event_types: Vec<String> = rows.iter().map(|row| row.event_type.clone()).collect();
    event_types.sort();
    event_types.dedup();
    event_types
}

fn query_totals_by_type(
    conn: &Connection,
    event_types: &[String],
) -> Result<Vec<(String, u64)>, TimelineServiceError> {
    if event_types.is_empty() {
        return Ok(Vec::new());
    }
    let placeholders: Vec<String> = (1..=event_types.len()).map(|i| format!("?{i}")).collect();
    let sql = format!(
        "SELECT event_type, COUNT(*) AS total
         FROM timeline_events
         WHERE event_type IN ({})
         GROUP BY event_type",
        placeholders.join(",")
    );
    let params: Vec<&dyn rusqlite::types::ToSql> = event_types
        .iter()
        .map(|value| value as &dyn rusqlite::types::ToSql)
        .collect();
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt
        .query_map(params.as_slice(), |row| {
            Ok((row.get(0)?, row.get::<_, i64>(1)? as u64))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

fn seed_stripes(totals: Vec<(String, u64)>) -> HashMap<String, TimelineStripeDto> {
    totals
        .into_iter()
        .map(|(event_type, total_events)| {
            (
                event_type,
                TimelineStripeDto {
                    clusters: Vec::new(),
                    total_events,
                },
            )
        })
        .collect()
}

fn append_clusters(stripes: &mut HashMap<String, TimelineStripeDto>, rows: Vec<ClusterRow>) {
    for row in rows {
        let cluster = TimelineClusterDto {
            event_type: row.event_type.clone(),
            description: row.description,
            count: row.count,
            first_ts: row.first_ts,
            last_ts: row.last_ts,
            sample_event_ids: row
                .sample_ids
                .split(',')
                .take(5)
                .map(str::to_string)
                .collect(),
        };
        stripes
            .entry(row.event_type)
            .or_insert_with(|| TimelineStripeDto {
                clusters: Vec::new(),
                total_events: 0,
            })
            .clusters
            .push(cluster);
    }
}
