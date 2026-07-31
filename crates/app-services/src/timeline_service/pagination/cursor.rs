use std::{cmp::Ordering, collections::VecDeque, path::Path};

use domain::{CaseId, DataSourceId};
use persistence_sqlite::repositories::timeline_repo::{
    TimelineCursorRow, TimelineRepo, TimelineSortKey,
};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use transport::{
    dto::TimelineEventDto,
    paging::{decode_opaque_cursor, encode_opaque_cursor, PageResponse},
};

use super::super::{export::timeline_event_to_source_dto, TimelineQuery, TimelineServiceError};
use crate::source_db;

const CURSOR_PAYLOAD_VERSION: u8 = 2;
const CURSOR_KIND: &str = "timeline";
const MAX_CURSOR_SOURCES: usize = 256;
const MAX_CURSOR_VALUE_LENGTH: usize = 4_096;
const MAX_SOURCE_LOOKAHEAD: usize = 32;
const MAX_PAGE_LOOKAHEAD: usize = 256;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct TimelineCursorContext {
    time_start: Option<String>,
    time_end: Option<String>,
    event_type: Option<String>,
}

impl TimelineCursorContext {
    fn from_query(query: TimelineQuery<'_>) -> Self {
        Self {
            time_start: query.time_start.map(str::to_owned),
            time_end: query.time_end.map(str::to_owned),
            event_type: query.event_type.map(str::to_owned),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct TimelineConsumedKey {
    timestamp: Option<String>,
    event_id: String,
}

impl From<&TimelineSortKey> for TimelineConsumedKey {
    fn from(key: &TimelineSortKey) -> Self {
        Self {
            timestamp: key.timestamp.clone(),
            event_id: key.event_id.clone(),
        }
    }
}

impl TimelineConsumedKey {
    fn to_repo_key(&self) -> TimelineSortKey {
        TimelineSortKey {
            timestamp: self.timestamp.clone(),
            event_id: self.event_id.clone(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct TimelineSourceState {
    source_id: String,
    revision: u64,
    snapshot_high_water: i64,
    count: u64,
    consumed: u64,
    after: Option<TimelineConsumedKey>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct TimelinePageCursor {
    version: u8,
    kind: String,
    case_id: String,
    context: TimelineCursorContext,
    total: u64,
    sources: Vec<TimelineSourceState>,
}

struct TimelineSourcePage {
    state: TimelineSourceState,
    connection: Connection,
    buffer: VecDeque<TimelineCursorRow>,
    exhausted: bool,
}

impl TimelineSourcePage {
    fn ensure_head(
        &mut self,
        context: &TimelineCursorContext,
        batch_size: u32,
    ) -> Result<(), TimelineServiceError> {
        if self.exhausted || !self.buffer.is_empty() {
            return Ok(());
        }
        let after = self
            .state
            .after
            .as_ref()
            .map(TimelineConsumedKey::to_repo_key);
        let rows = TimelineRepo::new(&self.connection).query_filtered_after_at_snapshot(
            after.as_ref(),
            self.state.snapshot_high_water,
            batch_size,
            context.time_start.as_deref(),
            context.time_end.as_deref(),
            context.event_type.as_deref(),
        )?;
        self.exhausted = rows.len() < batch_size as usize;
        self.buffer.extend(rows);
        Ok(())
    }

    fn consume_head(&mut self) -> Result<TimelineCursorRow, TimelineServiceError> {
        let row = self.buffer.pop_front().ok_or_else(|| {
            TimelineServiceError::Other(
                "timeline cursor selected a source without a head row".to_string(),
            )
        })?;
        self.state.after = Some(TimelineConsumedKey::from(&row.sort_key));
        self.state.consumed = self.state.consumed.saturating_add(1);
        Ok(row)
    }
}

pub(super) fn query_cursor_page(
    case_conn: &Connection,
    case_root: &Path,
    case_id: &CaseId,
    query: TimelineQuery<'_>,
) -> Result<PageResponse<TimelineEventDto>, TimelineServiceError> {
    let context = TimelineCursorContext::from_query(query);
    let (state, sources) = match query.cursor {
        Some(cursor) => resume_cursor(case_conn, case_root, case_id, cursor, &context)?,
        None => capture_cursor(case_conn, case_root, case_id, context.clone())?,
    };
    paginate_cursor(state, sources, query.limit)
}

fn capture_cursor(
    case_conn: &Connection,
    case_root: &Path,
    case_id: &CaseId,
    context: TimelineCursorContext,
) -> Result<(TimelinePageCursor, Vec<TimelineSourcePage>), TimelineServiceError> {
    let mut connections =
        source_db::open_ready_source_connections_read_only(case_conn, case_root, case_id)?;
    connections.sort_by(|left, right| left.0 .0.cmp(&right.0 .0));
    let mut total = 0u64;
    let mut sources = Vec::with_capacity(connections.len());
    for (source_id, connection) in connections {
        let repo = TimelineRepo::new(&connection);
        let revision = repo.cursor_revision()?;
        let snapshot_high_water = repo.snapshot_high_water()?;
        let count = repo.count_filtered_at_snapshot(
            snapshot_high_water,
            context.time_start.as_deref(),
            context.time_end.as_deref(),
            context.event_type.as_deref(),
        )?;
        total = total.saturating_add(count);
        sources.push(new_source_page(
            source_id,
            connection,
            revision,
            snapshot_high_water,
            count,
        ));
    }
    let state = TimelinePageCursor {
        version: CURSOR_PAYLOAD_VERSION,
        kind: CURSOR_KIND.to_string(),
        case_id: case_id.0.clone(),
        context,
        total,
        sources: sources.iter().map(|source| source.state.clone()).collect(),
    };
    Ok((state, sources))
}

fn resume_cursor(
    case_conn: &Connection,
    case_root: &Path,
    case_id: &CaseId,
    encoded: &str,
    context: &TimelineCursorContext,
) -> Result<(TimelinePageCursor, Vec<TimelineSourcePage>), TimelineServiceError> {
    let state: TimelinePageCursor =
        decode_opaque_cursor(encoded).map_err(|error| invalid_cursor(error.to_string()))?;
    validate_cursor_state(&state, case_id, context)?;
    let mut connections =
        source_db::open_ready_source_connections_read_only(case_conn, case_root, case_id)?;
    connections.sort_by(|left, right| left.0 .0.cmp(&right.0 .0));
    let current_ids = connections
        .iter()
        .map(|(source_id, _)| source_id.0.as_str())
        .collect::<Vec<_>>();
    let cursor_ids = state
        .sources
        .iter()
        .map(|source| source.source_id.as_str())
        .collect::<Vec<_>>();
    if current_ids != cursor_ids {
        return Err(invalid_cursor("data source set changed"));
    }
    let mut sources = Vec::with_capacity(connections.len());
    for ((source_id, connection), source_state) in
        connections.into_iter().zip(state.sources.iter().cloned())
    {
        let repo = TimelineRepo::new(&connection);
        if repo.cursor_revision()? != source_state.revision {
            return Err(invalid_cursor(format!(
                "timeline snapshot changed for source {}",
                source_id.0
            )));
        }
        let current_count = repo.count_filtered_at_snapshot(
            source_state.snapshot_high_water,
            context.time_start.as_deref(),
            context.time_end.as_deref(),
            context.event_type.as_deref(),
        )?;
        if current_count != source_state.count {
            return Err(invalid_cursor(format!(
                "timeline snapshot changed for source {}",
                source_id.0
            )));
        }
        sources.push(TimelineSourcePage {
            exhausted: source_state.consumed >= source_state.count,
            state: source_state,
            connection,
            buffer: VecDeque::new(),
        });
    }
    Ok((state, sources))
}

fn paginate_cursor(
    mut state: TimelinePageCursor,
    mut sources: Vec<TimelineSourcePage>,
    limit: u32,
) -> Result<PageResponse<TimelineEventDto>, TimelineServiceError> {
    let batch_size = cursor_batch_size(limit, sources.len());
    let mut items = Vec::with_capacity(limit as usize);
    validate_timeline_revisions(&sources)?;
    while items.len() < limit as usize {
        fill_timeline_heads(&mut sources, &state.context, batch_size)?;
        validate_timeline_snapshot(&sources)?;
        let Some(source_index) = next_timeline_source(&sources) else {
            break;
        };
        let row = sources[source_index].consume_head()?;
        items.push(timeline_event_to_source_dto(
            row.event,
            &DataSourceId(sources[source_index].state.source_id.clone()),
        ));
    }
    fill_timeline_heads(&mut sources, &state.context, batch_size)?;
    validate_timeline_snapshot(&sources)?;
    validate_timeline_revisions(&sources)?;
    let has_more = sources.iter().any(|source| !source.buffer.is_empty());
    state.sources = sources.into_iter().map(|source| source.state).collect();
    let next_cursor = has_more
        .then(|| encode_opaque_cursor(&state))
        .transpose()
        .map_err(|error| TimelineServiceError::Other(error.to_string()))?;
    Ok(PageResponse {
        total: state.total,
        items,
        next_cursor,
    })
}

fn validate_timeline_revisions(sources: &[TimelineSourcePage]) -> Result<(), TimelineServiceError> {
    for source in sources {
        if TimelineRepo::new(&source.connection).cursor_revision()? != source.state.revision {
            return Err(invalid_cursor(format!(
                "timeline snapshot changed for source {}",
                source.state.source_id
            )));
        }
    }
    Ok(())
}

fn fill_timeline_heads(
    sources: &mut [TimelineSourcePage],
    context: &TimelineCursorContext,
    batch_size: u32,
) -> Result<(), TimelineServiceError> {
    for source in sources {
        source.ensure_head(context, batch_size)?;
    }
    Ok(())
}

fn validate_timeline_snapshot(sources: &[TimelineSourcePage]) -> Result<(), TimelineServiceError> {
    for source in sources {
        let visible = source
            .state
            .consumed
            .saturating_add(source.buffer.len() as u64);
        if visible > source.state.count || (source.exhausted && visible != source.state.count) {
            return Err(invalid_cursor(format!(
                "timeline snapshot changed for source {}",
                source.state.source_id
            )));
        }
    }
    Ok(())
}

fn next_timeline_source(sources: &[TimelineSourcePage]) -> Option<usize> {
    sources
        .iter()
        .enumerate()
        .filter_map(|(index, source)| source.buffer.front().map(|head| (index, source, head)))
        .min_by(|(_, left_source, left), (_, right_source, right)| {
            compare_timeline_rows(left_source, left, right_source, right)
        })
        .map(|(index, _, _)| index)
}

fn compare_timeline_rows(
    left_source: &TimelineSourcePage,
    left: &TimelineCursorRow,
    right_source: &TimelineSourcePage,
    right: &TimelineCursorRow,
) -> Ordering {
    right
        .sort_key
        .timestamp
        .cmp(&left.sort_key.timestamp)
        .then_with(|| {
            left_source
                .state
                .source_id
                .cmp(&right_source.state.source_id)
        })
        .then_with(|| left.sort_key.event_id.cmp(&right.sort_key.event_id))
}

fn new_source_page(
    source_id: DataSourceId,
    connection: Connection,
    revision: u64,
    snapshot_high_water: i64,
    count: u64,
) -> TimelineSourcePage {
    TimelineSourcePage {
        state: TimelineSourceState {
            source_id: source_id.0,
            revision,
            snapshot_high_water,
            count,
            consumed: 0,
            after: None,
        },
        connection,
        buffer: VecDeque::new(),
        exhausted: count == 0,
    }
}

fn cursor_batch_size(limit: u32, source_count: usize) -> u32 {
    if source_count == 0 {
        return 1;
    }
    let budget = (limit as usize).clamp(1, MAX_PAGE_LOOKAHEAD);
    budget.div_ceil(source_count).clamp(1, MAX_SOURCE_LOOKAHEAD) as u32
}

fn validate_cursor_state(
    state: &TimelinePageCursor,
    case_id: &CaseId,
    context: &TimelineCursorContext,
) -> Result<(), TimelineServiceError> {
    if state.version != CURSOR_PAYLOAD_VERSION
        || state.kind != CURSOR_KIND
        || state.case_id != case_id.0
        || &state.context != context
    {
        return Err(invalid_cursor("query context does not match"));
    }
    if state.sources.len() > MAX_CURSOR_SOURCES {
        return Err(invalid_cursor("source count exceeds the supported bound"));
    }
    let mut previous_source = None;
    let mut total = 0u64;
    for source in &state.sources {
        validate_source_state(source, previous_source)?;
        previous_source = Some(source.source_id.as_str());
        total = total.saturating_add(source.count);
    }
    if total != state.total {
        return Err(invalid_cursor("snapshot total is inconsistent"));
    }
    Ok(())
}

fn validate_source_state(
    source: &TimelineSourceState,
    previous_source: Option<&str>,
) -> Result<(), TimelineServiceError> {
    if source.source_id.is_empty()
        || source.source_id.len() > MAX_CURSOR_VALUE_LENGTH
        || source.snapshot_high_water < 0
        || source.consumed > source.count
        || previous_source.is_some_and(|previous| previous >= source.source_id.as_str())
        || (source.consumed == 0) != source.after.is_none()
    {
        return Err(invalid_cursor("source snapshot state is invalid"));
    }
    if source.after.as_ref().is_some_and(|after| {
        after.event_id.is_empty()
            || after.event_id.len() > MAX_CURSOR_VALUE_LENGTH
            || after
                .timestamp
                .as_ref()
                .is_some_and(|timestamp| timestamp.len() > MAX_CURSOR_VALUE_LENGTH)
    }) {
        return Err(invalid_cursor("consumed sort key is invalid"));
    }
    Ok(())
}

fn invalid_cursor(reason: impl AsRef<str>) -> TimelineServiceError {
    TimelineServiceError::InvalidInput(format!(
        "invalid or stale timeline cursor: {}",
        reason.as_ref()
    ))
}
