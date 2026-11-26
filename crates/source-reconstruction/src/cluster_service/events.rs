use domain::{CaseId, DataSourceId};
use rusqlite::Connection;
use rusqlite::OptionalExtension;
use std::cmp::Ordering;
use std::collections::{BinaryHeap, VecDeque};
use transport::dto::LinuxEvidenceEventDto;

use source_kernel::source_db;

use super::{ClusterServiceError, Result};

pub fn get_linux_evidence_events(
    case_connection: &Connection,
    case_root: &std::path::Path,
    case_id: &CaseId,
    import_set_id: &str,
    offset: u64,
    limit: u32,
) -> Result<Vec<LinuxEvidenceEventDto>> {
    let source_ids = source_ids(case_connection, case_id, import_set_id)?;
    if limit == 0 {
        return Ok(Vec::new());
    }
    let mut sources = source_ids
        .into_iter()
        .map(|source_id| EventSource::open(case_root, source_id))
        .collect::<Result<Vec<_>>>()?;
    let mut newest = BinaryHeap::new();
    for (index, source) in sources.iter_mut().enumerate() {
        if let Some(record) = source.next_record()? {
            newest.push(HeapEvent {
                source_index: index,
                record,
            });
        }
    }
    let mut skipped = 0u64;
    let mut events = Vec::with_capacity(limit as usize);
    while let Some(event) = newest.pop() {
        if skipped < offset {
            skipped += 1;
        } else if events.len() < limit as usize {
            events.push(event_from_fjall_record(
                &sources[event.source_index].source_id,
                event.record,
            ));
        } else {
            break;
        }
        if let Some(record) = sources[event.source_index].next_record()? {
            newest.push(HeapEvent {
                source_index: event.source_index,
                record,
            });
        }
    }
    Ok(events)
}

struct EventSource {
    source_id: String,
    fjall: source_kernel::source_db::FjallReadOnlySourceStorageHandle,
    snapshot: persistence_source::fjall::SourceReadSnapshot,
    page: VecDeque<persistence_source::fjall::TimelineRecord>,
    next_cursor: Option<Vec<u8>>,
    exhausted: bool,
}

impl EventSource {
    fn open(case_root: &std::path::Path, source_id: String) -> Result<Self> {
        let fjall = source_db::open_registered_fjall_source_read_only(
            case_root,
            &DataSourceId(source_id.clone()),
        )
        .map_err(|error| {
            ClusterServiceError::Db(persistence_sqlite::DbError::System(error.to_string()))
        })?;
        let snapshot = fjall.snapshot();
        fjall
            .require_projection_complete(&snapshot, &source_id)
            .map_err(|error| {
                ClusterServiceError::Db(persistence_sqlite::DbError::System(error.to_string()))
            })?;
        fjall
            .require_timeline_projection_complete(&snapshot, &source_id)
            .map_err(|error| {
                ClusterServiceError::Db(persistence_sqlite::DbError::System(error.to_string()))
            })?;
        Ok(Self {
            source_id,
            fjall,
            snapshot,
            page: VecDeque::new(),
            next_cursor: None,
            exhausted: false,
        })
    }

    fn next_record(&mut self) -> Result<Option<persistence_source::fjall::TimelineRecord>> {
        if let Some(record) = self.page.pop_front() {
            return Ok(Some(record));
        }
        if self.exhausted {
            return Ok(None);
        }
        let page = self
            .fjall
            .timeline_list_by_time(
                &self.snapshot,
                &self.source_id,
                self.next_cursor.as_deref(),
                persistence_source::fjall::ScanDirection::Forward,
                128,
            )
            .map_err(|error| {
                ClusterServiceError::Db(persistence_sqlite::DbError::System(error.to_string()))
            })?;
        self.next_cursor = page.next_cursor;
        self.exhausted = self.next_cursor.is_none();
        self.page = page.records.into();
        Ok(self.page.pop_front())
    }
}

struct HeapEvent {
    source_index: usize,
    record: persistence_source::fjall::TimelineRecord,
}

impl PartialEq for HeapEvent {
    fn eq(&self, other: &Self) -> bool {
        self.record.timestamp == other.record.timestamp
            && self.record.id == other.record.id
            && self.source_index == other.source_index
    }
}

impl Eq for HeapEvent {}

impl PartialOrd for HeapEvent {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for HeapEvent {
    fn cmp(&self, other: &Self) -> Ordering {
        self.record
            .timestamp
            .cmp(&other.record.timestamp)
            .then_with(|| other.record.id.cmp(&self.record.id))
            .then_with(|| other.source_index.cmp(&self.source_index))
    }
}

fn event_from_fjall_record(
    source_id: &str,
    record: persistence_source::fjall::TimelineRecord,
) -> LinuxEvidenceEventDto {
    let attrs = serde_json::to_value(record.attrs)
        .ok()
        .and_then(|value| value.as_object().cloned())
        .unwrap_or_default();
    LinuxEvidenceEventDto {
        event_id: record.id,
        data_source_id: source_id.to_owned(),
        source_object_id: record.source_object_id,
        event_type: record.event_type,
        event_time: record.timestamp.to_rfc3339(),
        observed_time: string_attr(&attrs, "observedTime"),
        ingest_time: string_attr(&attrs, "ingestTime"),
        timezone: string_attr(&attrs, "timezone").or_else(|| string_attr(&attrs, "tzAssumed")),
        clock_skew_seconds: attrs
            .get("clockSkewSeconds")
            .and_then(serde_json::Value::as_i64),
        native_sequence: string_attr(&attrs, "nativeSequence"),
        actor: string_attr(&attrs, "actor"),
        resource: string_attr(&attrs, "resource"),
        action: string_attr(&attrs, "action"),
        outcome: string_attr(&attrs, "outcome"),
        parser_id: record.parser_id,
        parser_version: record.parser_version,
        raw_digest: string_attr(&attrs, "rawDigest")
            .or_else(|| string_attr(&attrs, "contentDigest")),
        confidence: record.confidence,
        completeness: string_attr(&attrs, "completeness").unwrap_or_else(|| "parsed".to_owned()),
        title: record.title,
        description: record.description,
    }
}

fn source_ids(conn: &Connection, case_id: &CaseId, import_set_id: &str) -> Result<Vec<String>> {
    let owner = conn
        .query_row(
            "SELECT case_id FROM linux_import_sets WHERE id = ?1",
            [import_set_id],
            |row| row.get::<_, String>(0),
        )
        .optional()?;
    if owner.as_deref() != Some(case_id.0.as_str()) {
        return Err(ClusterServiceError::InvalidClusterId);
    }
    let mut statement = conn.prepare(
        "SELECT member.data_source_id, member.import_state, source.case_id
         FROM linux_import_set_members AS member
         LEFT JOIN data_sources AS source ON source.id = member.data_source_id
         WHERE member.import_set_id = ?1 ORDER BY member.member_index",
    )?;
    let mut ids = std::collections::BTreeSet::new();
    let rows = statement.query_map([import_set_id], |row| {
        Ok((
            row.get::<_, Option<String>>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, Option<String>>(2)?,
        ))
    })?;
    for row in rows {
        let (id, state, owner) = row?;
        if !matches!(state.as_str(), "ready" | "ready_metadata")
            || owner.as_deref() != Some(case_id.0.as_str())
        {
            return Err(ClusterServiceError::IncompleteImportSet);
        }
        ids.insert(id.ok_or(ClusterServiceError::IncompleteImportSet)?);
    }
    if ids.is_empty() {
        return Err(ClusterServiceError::IncompleteImportSet);
    }
    Ok(ids.into_iter().collect())
}

fn string_attr(attrs: &serde_json::Map<String, serde_json::Value>, key: &str) -> Option<String> {
    attrs
        .get(key)
        .and_then(serde_json::Value::as_str)
        .map(str::to_string)
}
