use super::{export::timeline_event_to_source_dto, TimelineQuery, TimelineServiceError};
use crate::source_db;
use crate::storage_runtime::{ReadOnlySourceStorageHandle, StorageRuntimeError};
use chrono::{DateTime, Utc};
use domain::{CaseId, DataSourceId, TimelineEvent, TimelineEventId};
use persistence_fjall::{ScanDirection, SourceReadSnapshot, TimelineRecord};
use persistence_sqlite::repositories::timeline_repo::TimelineRepo;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use transport::{
    dto::TimelineEventDto,
    paging::{decode_opaque_cursor, encode_opaque_cursor, PageResponse},
};

const CURSOR_VERSION: u8 = 1;
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
    sqlite_revision: u64,
    count: u64,
    consumed: u64,
    after: Option<Vec<u8>>,
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
        let handle = match source_db::open_registered_fjall_source_read_only(case_root, &source.id)
        {
            Ok(handle) => handle,
            Err(StorageRuntimeError::MissingRoot(_)) => return Ok(None),
            Err(error) => return Err(TimelineServiceError::Other(error.to_string())),
        };
        sources.push((source.id, handle));
    }
    sources.sort_by(|left, right| left.0 .0.cmp(&right.0 .0));
    let context = context_from_query(query)?;
    let (state, pages) = match query.cursor {
        Some(cursor) => resume_cursor(case_conn, case_root, case_id, &context, cursor, sources)?,
        None => capture_cursor(case_conn, case_root, case_id, &context, sources)?,
    };
    Ok(Some(paginate(state, pages, query.offset, query.limit)?))
}

fn capture_cursor(
    case_conn: &Connection,
    case_root: &std::path::Path,
    case_id: &CaseId,
    context: &FjallContext,
    sources: Vec<(DataSourceId, ReadOnlySourceStorageHandle)>,
) -> Result<(FjallCursor, Vec<FjallSourcePage>), TimelineServiceError> {
    let mut total = 0u64;
    let mut pages = Vec::with_capacity(sources.len());
    for (source_id, source) in sources {
        let snapshot = source.snapshot();
        let revision = source.timeline_cursor_revision(&snapshot)?;
        let sqlite_revision = sqlite_timeline_revision(case_conn, case_root, &source_id)?;
        let count = count_source(&source, &snapshot, &source_id.0, context)?;
        total = total.saturating_add(count);
        pages.push(FjallSourcePage::new(
            source_id,
            source,
            snapshot,
            revision,
            sqlite_revision,
            count,
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
    case_conn: &Connection,
    case_root: &std::path::Path,
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
        let sqlite_revision = sqlite_timeline_revision(case_conn, case_root, &source_id)?;
        if revision != source_state.revision {
            return Err(invalid_cursor(format!(
                "timeline snapshot changed for source {}",
                source_id.0
            )));
        }
        if sqlite_revision != source_state.sqlite_revision {
            return Err(invalid_cursor(format!(
                "timeline snapshot changed for source {}",
                source_id.0
            )));
        }
        pages.push(FjallSourcePage::from_state(
            source,
            snapshot,
            source_state.clone(),
        ));
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
        sqlite_revision: u64,
        count: u64,
    ) -> Self {
        Self {
            state: FjallSourceCursor {
                source_id: source_id.0,
                revision,
                sqlite_revision,
                count,
                consumed: 0,
                after: None,
            },
            source,
            snapshot,
            head: None,
            exhausted: count == 0,
            skip_remaining: 0,
        }
    }

    fn from_state(
        source: ReadOnlySourceStorageHandle,
        snapshot: SourceReadSnapshot,
        mut state: FjallSourceCursor,
    ) -> Self {
        let skip_remaining = state.consumed;
        state.after = None;
        Self {
            exhausted: state.consumed >= state.count,
            state,
            source,
            snapshot,
            head: None,
            skip_remaining,
        }
    }

    fn ensure_head(&mut self, context: &FjallContext) -> Result<(), TimelineServiceError> {
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
        self.state.after = page.next_cursor;
        if self.state.after.is_none() {
            self.exhausted = true;
        }
        if self.skip_remaining > 0 {
            self.skip_remaining = self.skip_remaining.saturating_sub(1);
            self.ensure_head(context)?;
            return Ok(());
        }
        if matches_context(&record, context)? {
            self.head = Some(record);
        } else {
            self.ensure_head(context)?;
        }
        Ok(())
    }
}

fn count_source(
    source: &ReadOnlySourceStorageHandle,
    snapshot: &SourceReadSnapshot,
    source_id: &str,
    context: &FjallContext,
) -> Result<u64, TimelineServiceError> {
    let mut after = None;
    let mut count = 0u64;
    loop {
        let page = if let Some(event_type) = context.event_type.as_deref() {
            source.timeline_list_by_type(
                snapshot,
                source_id,
                event_type,
                after.as_deref(),
                ScanDirection::Forward,
                FETCH_LIMIT,
            )?
        } else {
            source.timeline_list_by_time(
                snapshot,
                source_id,
                after.as_deref(),
                ScanDirection::Forward,
                FETCH_LIMIT,
            )?
        };
        let Some(record) = page.records.into_iter().next() else {
            break;
        };
        after = page.next_cursor;
        if matches_context(&record, context)? {
            count = count.saturating_add(1);
        }
        if after.is_none() {
            break;
        }
    }
    Ok(count)
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

fn sqlite_timeline_revision(
    case_conn: &Connection,
    case_root: &std::path::Path,
    source_id: &DataSourceId,
) -> Result<u64, TimelineServiceError> {
    let source = source_db::open_registered_source_db_read_only(case_conn, case_root, source_id)?;
    TimelineRepo::new(&source)
        .cursor_revision()
        .map_err(TimelineServiceError::from)
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
