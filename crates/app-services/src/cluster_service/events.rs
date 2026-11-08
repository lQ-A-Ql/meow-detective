use domain::{CaseId, DataSourceId};
use rusqlite::Connection;
use transport::dto::LinuxEvidenceEventDto;

use crate::source_db;

use super::{ClusterServiceError, Result};

pub fn get_linux_evidence_events(
    case_connection: &Connection,
    case_root: &std::path::Path,
    _case_id: &CaseId,
    import_set_id: &str,
    offset: u64,
    limit: u32,
) -> Result<Vec<LinuxEvidenceEventDto>> {
    let source_ids = source_ids(case_connection, import_set_id)?;
    let mut events = Vec::new();
    let per_source_limit = offset.saturating_add(u64::from(limit)).min(2_000);
    for source_id in source_ids {
        if let Some(projected) = projected_events(case_root, &source_id, per_source_limit)? {
            events.extend(projected);
        }
    }
    events.sort_by(|left, right| {
        right
            .event_time
            .cmp(&left.event_time)
            .then_with(|| left.event_id.cmp(&right.event_id))
    });
    let start = offset.min(events.len() as u64) as usize;
    let end = start.saturating_add(limit as usize).min(events.len());
    Ok(events[start..end].to_vec())
}

fn projected_events(
    case_root: &std::path::Path,
    source_id: &str,
    limit: u64,
) -> Result<Option<Vec<LinuxEvidenceEventDto>>> {
    let Ok(fjall) = source_db::open_registered_fjall_source_read_only(
        case_root,
        &DataSourceId(source_id.to_owned()),
    ) else {
        return Ok(None);
    };
    let snapshot = fjall.snapshot();
    fjall
        .require_projection_complete(&snapshot, source_id)
        .map_err(|error| {
            ClusterServiceError::Db(persistence_sqlite::DbError::System(error.to_string()))
        })?;
    let page = fjall
        .timeline_list_by_time(
            &snapshot,
            source_id,
            None,
            persistence_fjall::ScanDirection::Forward,
            limit as usize,
        )
        .map_err(|error| {
            ClusterServiceError::Db(persistence_sqlite::DbError::System(error.to_string()))
        })?;
    Ok(Some(
        page.records
            .into_iter()
            .map(|record| event_from_fjall_record(source_id, record))
            .collect(),
    ))
}

fn event_from_fjall_record(
    source_id: &str,
    record: persistence_fjall::TimelineRecord,
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

fn source_ids(conn: &Connection, import_set_id: &str) -> Result<Vec<String>> {
    let mut statement = conn.prepare(
        "SELECT data_source_id FROM linux_import_set_members
         WHERE import_set_id = ?1 AND data_source_id IS NOT NULL
         ORDER BY member_index",
    )?;
    let rows = statement.query_map([import_set_id], |row| row.get(0))?;
    rows.collect::<std::result::Result<Vec<_>, _>>()
        .map_err(Into::into)
}

fn string_attr(attrs: &serde_json::Map<String, serde_json::Value>, key: &str) -> Option<String> {
    attrs
        .get(key)
        .and_then(serde_json::Value::as_str)
        .map(str::to_string)
}
