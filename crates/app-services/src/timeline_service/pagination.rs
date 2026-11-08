use persistence_fjall::{ScanDirection, TimelineRecordStore};
use persistence_sqlite::repositories::timeline_repo::{
    TimelineCursorRow, TimelineRepo, TimelineSortKey,
};
use rusqlite::{params, Connection};
use std::collections::{HashMap, VecDeque};
use std::path::Path;
use transport::{
    dto::{TimelineAggregatedDto, TimelineClusterDto, TimelineEventDto, TimelineStripeDto},
    paging::PageResponse,
};

use super::export::timeline_event_to_source_dto;
use super::{TimelineQuery, TimelineServiceError};
use crate::source_db;

mod cursor;

const TIMELINE_MERGE_BATCH_SIZE: u32 = 256;

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
    if query.cursor.is_some() && query.offset != 0 {
        return Err(TimelineServiceError::InvalidInput(
            "offset must be zero when cursor is provided".to_string(),
        ));
    }
    if query.cursor.is_some() || query.offset == 0 {
        if let Some(page) = super::fjall::try_query(case_conn, case_root, case_id, query)? {
            return Ok(page);
        }
        return cursor::query_cursor_page(case_conn, case_root, case_id, query);
    }
    if let Some(page) = super::fjall::try_query(case_conn, case_root, case_id, query)? {
        return Ok(page);
    }
    query_timeline_offset_page(case_conn, case_root, case_id, query)
}

fn query_timeline_offset_page(
    case_conn: &Connection,
    case_root: &Path,
    case_id: &domain::CaseId,
    query: TimelineQuery<'_>,
) -> Result<PageResponse<TimelineEventDto>, TimelineServiceError> {
    let mut total = 0u64;
    let mut sources = Vec::new();

    for (source_id, source_conn) in
        source_db::open_ready_source_connections_read_only(case_conn, case_root, case_id)?
    {
        let repo = TimelineRepo::new(&source_conn);
        total = total.saturating_add(repo.count_filtered(
            query.time_start,
            query.time_end,
            query.event_type,
        )?);
        sources.push(SourceTimelineCursor::new(source_id, source_conn));
    }

    if query.limit == 0 || query.offset >= total {
        return Ok(PageResponse {
            total,
            items: Vec::new(),
            next_cursor: None,
        });
    }

    let scan_end = query
        .offset
        .saturating_add(u64::from(query.limit))
        .min(total);
    let batch_size = query.limit.max(TIMELINE_MERGE_BATCH_SIZE);
    let mut position = 0u64;
    let mut items = Vec::with_capacity(query.limit as usize);
    while position < scan_end {
        for source in &mut sources {
            source.refill(query, batch_size)?;
        }
        let Some(source_index) = next_timeline_source(&sources) else {
            break;
        };
        let source = &mut sources[source_index];
        let event = source.buffer.pop_front().ok_or_else(|| {
            TimelineServiceError::Other(
                "timeline merge cursor selected a source without a buffered event".to_string(),
            )
        })?;
        if position >= query.offset {
            items.push(timeline_event_to_source_dto(event.event, &source.source_id));
        }
        position = position.saturating_add(1);
    }
    Ok(PageResponse {
        total,
        items,
        next_cursor: None,
    })
}

struct SourceTimelineCursor {
    source_id: domain::DataSourceId,
    connection: Connection,
    after: Option<TimelineSortKey>,
    buffer: VecDeque<TimelineCursorRow>,
    exhausted: bool,
}

impl SourceTimelineCursor {
    fn new(source_id: domain::DataSourceId, connection: Connection) -> Self {
        Self {
            source_id,
            connection,
            after: None,
            buffer: VecDeque::new(),
            exhausted: false,
        }
    }

    fn refill(
        &mut self,
        query: TimelineQuery<'_>,
        batch_size: u32,
    ) -> Result<(), TimelineServiceError> {
        if self.exhausted || !self.buffer.is_empty() {
            return Ok(());
        }
        let rows = TimelineRepo::new(&self.connection).query_filtered_after(
            self.after.as_ref(),
            batch_size,
            query.time_start,
            query.time_end,
            query.event_type,
        )?;
        let fetched = rows.len();
        if let Some(last) = rows.last() {
            self.after = Some(last.sort_key.clone());
        }
        self.buffer.extend(rows);
        self.exhausted = fetched < batch_size as usize;
        Ok(())
    }
}

fn next_timeline_source(sources: &[SourceTimelineCursor]) -> Option<usize> {
    let mut selected: Option<(usize, &SourceTimelineCursor, &TimelineCursorRow)> = None;
    for (index, source) in sources.iter().enumerate() {
        let Some(candidate) = source.buffer.front() else {
            continue;
        };
        if selected
            .as_ref()
            .is_none_or(|(_, current_source, current)| {
                timeline_precedes(source, candidate, current_source, current)
            })
        {
            selected = Some((index, source, candidate));
        }
    }
    selected.map(|(index, _, _)| index)
}

fn timeline_precedes(
    left_source: &SourceTimelineCursor,
    left: &TimelineCursorRow,
    right_source: &SourceTimelineCursor,
    right: &TimelineCursorRow,
) -> bool {
    left.sort_key.timestamp > right.sort_key.timestamp
        || (left.sort_key.timestamp == right.sort_key.timestamp
            && (left_source.source_id.0 < right_source.source_id.0
                || (left_source.source_id == right_source.source_id
                    && left.sort_key.event_id < right.sort_key.event_id)))
}

pub fn query_timeline_aggregated(
    conn: &Connection,
    offset: u64,
    limit: u32,
) -> Result<TimelineAggregatedDto, TimelineServiceError> {
    if persistence_sqlite::source_fjall::source_id(conn)?.is_some() {
        return query_native_timeline_aggregated(conn, offset, limit);
    }
    let rows = query_cluster_rows(conn, offset, limit)?;
    let event_types = distinct_event_types(&rows);
    let totals = query_totals_by_type(conn, &event_types)?;
    let mut stripes_by_type = seed_stripes(totals);
    append_clusters(&mut stripes_by_type, rows);
    Ok(TimelineAggregatedDto { stripes_by_type })
}

fn query_native_timeline_aggregated(
    conn: &Connection,
    offset: u64,
    limit: u32,
) -> Result<TimelineAggregatedDto, TimelineServiceError> {
    let source = persistence_sqlite::source_fjall::require_source_store(conn)
        .map_err(TimelineServiceError::Db)?;
    let source_id = persistence_sqlite::source_fjall::source_id(conn)?
        .ok_or_else(|| TimelineServiceError::Other("source identity is missing".into()))?;
    let repository = TimelineRecordStore::from_source_store((*source).clone());
    let snapshot = repository.read_snapshot();
    let mut cursor = None;
    let mut clusters = HashMap::<(String, String), ClusterRow>::new();
    loop {
        let page = repository
            .list_by_source(
                &snapshot,
                &source_id,
                cursor.as_deref(),
                ScanDirection::Forward,
                512,
            )
            .map_err(|error| TimelineServiceError::Other(error.to_string()))?;
        for record in page.records {
            let key = (record.event_type.clone(), record.description.clone());
            let row = clusters.entry(key).or_insert_with(|| ClusterRow {
                event_type: record.event_type.clone(),
                description: record.description.clone(),
                count: 0,
                first_ts: record.timestamp.to_rfc3339(),
                last_ts: record.timestamp.to_rfc3339(),
                sample_ids: String::new(),
            });
            row.count = row.count.saturating_add(1);
            let timestamp = record.timestamp.to_rfc3339();
            if timestamp < row.first_ts {
                row.first_ts = timestamp.clone();
            }
            if timestamp > row.last_ts {
                row.last_ts = timestamp;
            }
            if row
                .sample_ids
                .split(',')
                .filter(|value| !value.is_empty())
                .count()
                < 5
            {
                if !row.sample_ids.is_empty() {
                    row.sample_ids.push(',');
                }
                row.sample_ids.push_str(&record.id);
            }
        }
        cursor = page.next_cursor;
        if cursor.is_none() {
            break;
        }
    }
    let mut rows = clusters.into_values().collect::<Vec<_>>();
    rows.sort_by(|left, right| {
        right
            .count
            .cmp(&left.count)
            .then_with(|| left.event_type.cmp(&right.event_type))
            .then_with(|| left.description.cmp(&right.description))
    });
    let event_types = rows
        .iter()
        .map(|row| row.event_type.clone())
        .collect::<std::collections::BTreeSet<_>>();
    let totals = rows
        .iter()
        .fold(HashMap::<String, u64>::new(), |mut totals, row| {
            *totals.entry(row.event_type.clone()).or_default() += row.count;
            totals
        });
    let mut stripes = totals
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
        .collect::<HashMap<_, _>>();
    for row in rows.into_iter().skip(offset as usize).take(limit as usize) {
        stripes
            .entry(row.event_type.clone())
            .or_insert_with(|| TimelineStripeDto {
                clusters: Vec::new(),
                total_events: 0,
            })
            .clusters
            .push(TimelineClusterDto {
                event_type: row.event_type,
                description: row.description,
                count: row.count,
                first_ts: row.first_ts,
                last_ts: row.last_ts,
                sample_event_ids: row
                    .sample_ids
                    .split(',')
                    .filter(|value| !value.is_empty())
                    .map(str::to_owned)
                    .collect(),
            });
    }
    for event_type in event_types {
        stripes
            .entry(event_type)
            .or_insert_with(|| TimelineStripeDto {
                clusters: Vec::new(),
                total_events: 0,
            });
    }
    Ok(TimelineAggregatedDto {
        stripes_by_type: stripes,
    })
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
