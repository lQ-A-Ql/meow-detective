use domain::{CaseId, DataSourceId};
use rusqlite::Connection;
use transport::dto::LinuxEvidenceEventDto;

use crate::source_db;

use super::{ClusterServiceError, Result};

pub fn get_linux_evidence_events(
    case_connection: &Connection,
    case_root: &std::path::Path,
    case_id: &CaseId,
    import_set_id: &str,
    offset: u64,
    limit: u32,
) -> Result<Vec<LinuxEvidenceEventDto>> {
    let source_ids = source_ids(case_connection, import_set_id)?;
    let mut events = Vec::new();
    let per_source_limit = offset.saturating_add(u64::from(limit)).min(2_000);
    for source_id in source_ids {
        let source = source_db::open_ready_source_read_only_by_id(
            case_connection,
            case_root,
            case_id,
            &DataSourceId(source_id.clone()),
        )
        .map_err(|error| ClusterServiceError::Db(error.into_db_error()))?;
        let mut statement = source.connection.prepare(
            "SELECT id, source_object_id, event_type, ts, title, description,
                    parser_id, parser_version, confidence, attrs
             FROM timeline_events ORDER BY ts DESC, id ASC LIMIT ?1",
        )?;
        let rows = statement.query_map([per_source_limit as i64], |row| {
            let attrs: String = row.get(9)?;
            Ok(event_from_row(&source_id, row, &attrs))
        })?;
        for row in rows {
            events.push(row?);
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

fn event_from_row(
    data_source_id: &str,
    row: &rusqlite::Row<'_>,
    attrs_json: &str,
) -> LinuxEvidenceEventDto {
    let attrs = serde_json::from_str::<serde_json::Map<String, serde_json::Value>>(attrs_json)
        .unwrap_or_default();
    LinuxEvidenceEventDto {
        event_id: row.get(0).unwrap_or_default(),
        data_source_id: data_source_id.to_string(),
        source_object_id: row.get(1).unwrap_or_default(),
        event_type: row.get(2).unwrap_or_default(),
        event_time: row.get(3).unwrap_or_default(),
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
        parser_id: row.get(6).ok(),
        parser_version: row.get(7).ok(),
        raw_digest: string_attr(&attrs, "rawDigest")
            .or_else(|| string_attr(&attrs, "contentDigest")),
        confidence: row.get(8).ok(),
        completeness: string_attr(&attrs, "completeness").unwrap_or_else(|| "parsed".to_string()),
        title: row.get(4).unwrap_or_default(),
        description: row.get(5).unwrap_or_default(),
    }
}

fn string_attr(attrs: &serde_json::Map<String, serde_json::Value>, key: &str) -> Option<String> {
    attrs
        .get(key)
        .and_then(serde_json::Value::as_str)
        .map(str::to_string)
}
