use super::{export::timeline_event_to_source_dto, TimelineQuery, TimelineServiceError};
use crate::source_db;
use crate::storage_runtime::ReadOnlySourceStorageHandle;
use chrono::{DateTime, Utc};
use domain::{CaseId, DataSourceId, TimelineEvent, TimelineEventId};
use persistence_source::fjall::{ScanDirection, SourceReadSnapshot, TimelineRecord};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use transport::{
    dto::TimelineEventDto,
    paging::{decode_opaque_cursor, encode_opaque_cursor, PageResponse},
};

/// v4 changed `after` from the prefetched head key to the last consumed key,
/// so v3 cursors are rejected instead of silently skipping one row on resume.
const CURSOR_VERSION: u8 = 4;
const CURSOR_KIND: &str = "timeline-fjall";
const FETCH_LIMIT: usize = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct FjallContext {
    time_start: Option<String>,
    time_end: Option<String>,
    event_type: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct FjallSourceCursor {
    source_id: String,
    revision: u64,
    count: u64,
    consumed: u64,
    after: Option<Vec<u8>>,
    snapshot_upper_bound: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct FjallCursor {
    version: u8,
    kind: String,
    case_id: String,
    context: FjallContext,
    total: u64,
    sources: Vec<FjallSourceCursor>,
}

struct FjallSourcePage {
    state: FjallSourceCursor,
    source: ReadOnlySourceStorageHandle,
    snapshot: SourceReadSnapshot,
    head: Option<TimelineRecord>,
    /// Fjall key of the current head record. The cursor's `after` key stays on
    /// the last consumed row so a restored cursor can resume with an exclusive
    /// scan, and this key only advances the position once the head is taken.
    head_after: Option<Vec<u8>>,
    exhausted: bool,
    skip_remaining: u64,
}

pub(super) fn try_query(
    case_conn: &Connection,
    case_root: &std::path::Path,
    case_id: &CaseId,
    query: TimelineQuery<'_>,
) -> Result<Option<PageResponse<TimelineEventDto>>, TimelineServiceError> {
    let ready = source_db::ready_data_sources(case_conn, case_id)?;
    if ready.is_empty() {
        return Ok(Some(PageResponse {
            total: 0,
            items: Vec::new(),
            next_cursor: None,
        }));
    }
    let mut sources = Vec::with_capacity(ready.len());
    for (source, _) in ready {
        let handle = source_db::open_registered_fjall_source_read_only(case_root, &source.id)?;
        let snapshot = handle.snapshot();
        handle.require_projection_complete(&snapshot, &source.id.0)?;
        sources.push((source.id, handle));
    }
    sources.sort_by(|left, right| left.0 .0.cmp(&right.0 .0));
    let context = context_from_query(query)?;
    let (state, pages) = match query.cursor {
        Some(cursor) => resume_cursor(case_id, &context, cursor, sources)?,
        None => capture_cursor(case_id, &context, sources)?,
    };
    Ok(Some(paginate(state, pages, query.offset, query.limit)?))
}

fn capture_cursor(
    case_id: &CaseId,
    context: &FjallContext,
    sources: Vec<(DataSourceId, ReadOnlySourceStorageHandle)>,
) -> Result<(FjallCursor, Vec<FjallSourcePage>), TimelineServiceError> {
    let mut total = 0u64;
    let mut pages = Vec::with_capacity(sources.len());
    for (source_id, source) in sources {
        let snapshot = source.snapshot();
        let revision = source.timeline_cursor_revision(&snapshot)?;
        let count = count_source(&source, &snapshot, &source_id.0, context)?;
        let snapshot_upper_bound = source_upper_bound(&source, &snapshot, &source_id.0, context)?;
        total = total.saturating_add(count);
        pages.push(FjallSourcePage::new(
            source_id,
            source,
            snapshot,
            revision,
            count,
            snapshot_upper_bound,
        ));
    }
    let state = FjallCursor {
        version: CURSOR_VERSION,
        kind: CURSOR_KIND.to_owned(),
        case_id: case_id.0.clone(),
        context: context.clone(),
        total,
        sources: pages.iter().map(|page| page.state.clone()).collect(),
    };
    Ok((state, pages))
}

fn resume_cursor(
    case_id: &CaseId,
    context: &FjallContext,
    encoded: &str,
    sources: Vec<(DataSourceId, ReadOnlySourceStorageHandle)>,
) -> Result<(FjallCursor, Vec<FjallSourcePage>), TimelineServiceError> {
    let state: FjallCursor =
        decode_opaque_cursor(encoded).map_err(|error| invalid_cursor(error.to_string()))?;
    if state.version != CURSOR_VERSION
        || state.kind != CURSOR_KIND
        || state.context != *context
        || (state.case_id != case_id.0 && !state.case_id.is_empty())
    {
        return Err(invalid_cursor("query context does not match"));
    }
    let current_ids = sources
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
    let mut pages = Vec::with_capacity(sources.len());
    for ((source_id, source), source_state) in sources.into_iter().zip(state.sources.iter()) {
        let snapshot = source.snapshot();
        let revision = source.timeline_cursor_revision(&snapshot)?;
        let current_count = count_source(&source, &snapshot, &source_id.0, context)?;
        if revision != source_state.revision {
            let current_upper_bound =
                source_upper_bound(&source, &snapshot, &source_id.0, context)?;
            // Newer rows sort ahead of the frozen rows and are filtered by the upper
            // bound, so ahead-only growth is harmless only until this source has
            // consumed rows; backdated rows sort after the consumed prefix and stay
            // excluded by the frozen count.
            let grew_ahead = current_upper_bound > source_state.snapshot_upper_bound;
            let ahead_shift = grew_ahead && source_state.consumed > 0;
            if current_count <= source_state.count || ahead_shift {
                return Err(invalid_cursor(format!(
                    "timeline snapshot changed for source {}",
                    source_id.0
                )));
            }
        }
        let mut page = FjallSourcePage::from_state(source, snapshot, source_state.clone());
        if revision != source_state.revision {
            page.state.revision = revision;
        }
        pages.push(page);
    }
    Ok((state, pages))
}

fn paginate(
    mut state: FjallCursor,
    mut pages: Vec<FjallSourcePage>,
    offset: u64,
    limit: u32,
) -> Result<PageResponse<TimelineEventDto>, TimelineServiceError> {
    for _ in 0..offset {
        if take_next(&mut pages, &state.context)?.is_none() {
            break;
        }
    }
    let mut items = Vec::with_capacity(limit as usize);
    while items.len() < limit as usize {
        let Some((source_index, record)) = take_next(&mut pages, &state.context)? else {
            break;
        };
        items.push(timeline_event_to_source_dto(
            timeline_record_to_domain(record),
            &DataSourceId(pages[source_index].state.source_id.clone()),
        ));
    }
    let has_more = has_more(&mut pages, &state.context)?;
    state.sources = pages.iter().map(|page| page.state.clone()).collect();
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

fn take_next(
    pages: &mut [FjallSourcePage],
    context: &FjallContext,
) -> Result<Option<(usize, TimelineRecord)>, TimelineServiceError> {
    fill_heads(pages, context)?;
    let Some(index) = pages
        .iter()
        .enumerate()
        .filter_map(|(index, page)| page.head.as_ref().map(|record| (index, page, record)))
        .min_by(|(_, left_page, left), (_, right_page, right)| {
            right
                .timestamp
                .cmp(&left.timestamp)
                .then_with(|| left_page.state.source_id.cmp(&right_page.state.source_id))
                .then_with(|| left.id.cmp(&right.id))
        })
        .map(|(index, _, _)| index)
    else {
        return Ok(None);
    };
    let page = &mut pages[index];
    let record = page.head.take().expect("timeline head selected above");
    if let Some(after) = page.head_after.take() {
        page.state.after = Some(after);
    }
    page.state.consumed = page.state.consumed.saturating_add(1);
    Ok(Some((index, record)))
}

fn fill_heads(
    pages: &mut [FjallSourcePage],
    context: &FjallContext,
) -> Result<(), TimelineServiceError> {
    for page in pages {
        page.ensure_head(context)?;
    }
    Ok(())
}

fn has_more(
    pages: &mut [FjallSourcePage],
    context: &FjallContext,
) -> Result<bool, TimelineServiceError> {
    fill_heads(pages, context)?;
    Ok(pages.iter().any(|page| page.head.is_some()))
}

impl FjallSourcePage {
    fn new(
        source_id: DataSourceId,
        source: ReadOnlySourceStorageHandle,
        snapshot: SourceReadSnapshot,
        revision: u64,
        count: u64,
        snapshot_upper_bound: Option<DateTime<Utc>>,
    ) -> Self {
        Self {
            state: FjallSourceCursor {
                source_id: source_id.0,
                revision,
                count,
                consumed: 0,
                after: None,
                snapshot_upper_bound,
            },
            source,
            snapshot,
            head: None,
            head_after: None,
            exhausted: count == 0,
            skip_remaining: 0,
        }
    }

    fn from_state(
        source: ReadOnlySourceStorageHandle,
        snapshot: SourceReadSnapshot,
        state: FjallSourceCursor,
    ) -> Self {
        // A cursor that saved the Fjall key of its last consumed record resumes
        // with an exclusive scan from that key instead of replaying the consumed
        // prefix. Only legacy or incomplete cursors fall back to counting the
        // records that still have to be skipped.
        let skip_remaining = restored_skip_remaining(state.consumed, state.after.is_some());
        Self {
            exhausted: state.consumed >= state.count,
            state,
            source,
            snapshot,
            head: None,
            head_after: None,
            skip_remaining,
        }
    }

    fn ensure_head(&mut self, context: &FjallContext) -> Result<(), TimelineServiceError> {
        loop {
            if self.state.consumed >= self.state.count {
                self.exhausted = true;
            }
            if self.exhausted || self.head.is_some() {
                return Ok(());
            }
            let page = if let Some(event_type) = context.event_type.as_deref() {
                self.source.timeline_list_by_type(
                    &self.snapshot,
                    &self.state.source_id,
                    event_type,
                    self.state.after.as_deref(),
                    ScanDirection::Forward,
                    FETCH_LIMIT,
                )?
            } else {
                self.source.timeline_list_by_time(
                    &self.snapshot,
                    &self.state.source_id,
                    self.state.after.as_deref(),
                    ScanDirection::Forward,
                    FETCH_LIMIT,
                )?
            };
            let Some(record) = page.records.into_iter().next() else {
                self.exhausted = true;
                return Ok(());
            };
            let next_after = page.next_cursor;
            if next_after.is_none() {
                self.exhausted = true;
            }
            if self.skip_remaining > 0 {
                self.skip_remaining = self.skip_remaining.saturating_sub(1);
                self.state.after = next_after;
                continue;
            }
            if record.timestamp <= self.state.snapshot_upper_bound.unwrap_or(record.timestamp)
                && matches_context(&record, context)?
            {
                self.head_after = next_after;
                self.head = Some(record);
                return Ok(());
            }
            self.state.after = next_after;
        }
    }
}

/// Restored cursors carry the Fjall key of the last consumed record, so nothing
/// has to be replayed; cursors without a saved key keep the legacy replay rule.
pub(crate) fn restored_skip_remaining(consumed: u64, after_present: bool) -> u64 {
    if after_present {
        0
    } else {
        consumed
    }
}

fn count_source(
    source: &ReadOnlySourceStorageHandle,
    snapshot: &SourceReadSnapshot,
    source_id: &str,
    context: &FjallContext,
) -> Result<u64, TimelineServiceError> {
    let start = parse_time_bound(context.time_start.as_deref())?;
    let end = parse_time_bound(context.time_end.as_deref())?;
    let count = if let Some(event_type) = context.event_type.as_deref() {
        source.timeline_count_by_type(snapshot, source_id, event_type, start, end)?
    } else {
        source.timeline_count_by_time(snapshot, source_id, start, end)?
    };
    Ok(count)
}

fn source_upper_bound(
    source: &ReadOnlySourceStorageHandle,
    snapshot: &SourceReadSnapshot,
    source_id: &str,
    context: &FjallContext,
) -> Result<Option<DateTime<Utc>>, TimelineServiceError> {
    let page = if let Some(event_type) = context.event_type.as_deref() {
        source.timeline_list_by_type(
            snapshot,
            source_id,
            event_type,
            None,
            ScanDirection::Forward,
            1,
        )?
    } else {
        source.timeline_list_by_time(snapshot, source_id, None, ScanDirection::Forward, 1)?
    };
    Ok(page.records.first().map(|record| record.timestamp))
}

fn matches_context(
    record: &TimelineRecord,
    context: &FjallContext,
) -> Result<bool, TimelineServiceError> {
    let start = parse_time_bound(context.time_start.as_deref())?;
    let end = parse_time_bound(context.time_end.as_deref())?;
    Ok(start.is_none_or(|value| record.timestamp >= value)
        && end.is_none_or(|value| record.timestamp <= value))
}

fn parse_time_bound(value: Option<&str>) -> Result<Option<DateTime<Utc>>, TimelineServiceError> {
    value
        .map(|value| {
            DateTime::parse_from_rfc3339(value)
                .map(|value| value.with_timezone(&Utc))
                .map_err(|error| TimelineServiceError::InvalidInput(error.to_string()))
        })
        .transpose()
}

fn context_from_query(query: TimelineQuery<'_>) -> Result<FjallContext, TimelineServiceError> {
    let context = FjallContext {
        time_start: query.time_start.map(str::to_owned),
        time_end: query.time_end.map(str::to_owned),
        event_type: query.event_type.map(str::to_owned),
    };
    parse_time_bound(context.time_start.as_deref())?;
    parse_time_bound(context.time_end.as_deref())?;
    Ok(context)
}

pub(crate) fn timeline_record_to_domain(record: TimelineRecord) -> TimelineEvent {
    TimelineEvent {
        id: TimelineEventId(record.id),
        source_object_id: record.source_object_id,
        event_type: record.event_type,
        timestamp: record.timestamp,
        title: record.title,
        description: record.description,
        parser_id: record.parser_id,
        parser_version: record.parser_version,
        confidence: record.confidence,
        source_attribution: record.source_attribution,
        attrs: record.attrs,
    }
}

fn invalid_cursor(reason: impl AsRef<str>) -> TimelineServiceError {
    TimelineServiceError::InvalidInput(format!(
        "invalid or stale timeline cursor: {}",
        reason.as_ref()
    ))
}
