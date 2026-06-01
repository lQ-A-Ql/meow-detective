//! Bounded EVTX boot/shutdown candidate extraction.
//!
//! This is intentionally not a general event log platform. It extracts a small
//! set of System.evtx EventLog/User32 records that can be shown as candidates
//! with provenance.

use chrono::{DateTime, Utc};
use evtx::EvtxParser;
use serde_json::Value;

pub const MAX_EVTX_ANALYSIS_BYTES: usize = 64 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EvtxBootEventKind {
    EventLogStarted,
    EventLogStopped,
    UnexpectedShutdown,
    PlannedShutdown,
}

impl EvtxBootEventKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::EventLogStarted => "eventLogStarted",
            Self::EventLogStopped => "eventLogStopped",
            Self::UnexpectedShutdown => "unexpectedShutdown",
            Self::PlannedShutdown => "plannedShutdown",
        }
    }

    fn note(&self) -> &'static str {
        match self {
            Self::EventLogStarted => {
                "EventLog 6005 candidate; indicates the Event Log service started, not a direct boot assertion."
            }
            Self::EventLogStopped => {
                "EventLog 6006 candidate; indicates the Event Log service stopped, not a direct shutdown assertion."
            }
            Self::UnexpectedShutdown => {
                "EventLog 6008 candidate; indicates an unexpected prior shutdown reported by Windows."
            }
            Self::PlannedShutdown => {
                "User32 1074 candidate; indicates a planned shutdown or restart event."
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvtxBootEvent {
    pub timestamp: String,
    pub event_id: u32,
    pub record_id: Option<u64>,
    pub provider: Option<String>,
    pub kind: EvtxBootEventKind,
    pub source_path: String,
    pub note: String,
}

#[derive(Debug, Clone, Default)]
pub struct EvtxBootExtraction {
    pub events: Vec<EvtxBootEvent>,
    pub warnings: Vec<String>,
}

pub fn extract_boot_shutdown_events(bytes: &[u8], source_path: &str) -> EvtxBootExtraction {
    if bytes.len() > MAX_EVTX_ANALYSIS_BYTES {
        return EvtxBootExtraction {
            events: Vec::new(),
            warnings: vec![format!(
                "{source_path} exceeds bounded EVTX parser limit of {MAX_EVTX_ANALYSIS_BYTES} bytes"
            )],
        };
    }

    let mut parser = match EvtxParser::from_buffer(bytes.to_vec()) {
        Ok(parser) => parser,
        Err(err) => {
            return EvtxBootExtraction {
                events: Vec::new(),
                warnings: vec![format!(
                    "{source_path} EVTX parser initialization failed: {err}"
                )],
            };
        }
    };

    let mut extraction = EvtxBootExtraction::default();
    for record in parser.records_json_value() {
        match record {
            Ok(record) => {
                if let Some(event) = boot_event_from_json(
                    &record.data,
                    Some(record.event_record_id),
                    Some(record.timestamp.to_string()),
                    source_path,
                ) {
                    extraction.events.push(event);
                }
            }
            Err(err) => extraction
                .warnings
                .push(format!("{source_path} EVTX record parse error: {err}")),
        }
    }

    if extraction.events.is_empty() && extraction.warnings.is_empty() {
        extraction.warnings.push(format!(
            "{source_path} contains no supported boot/shutdown candidate events"
        ));
    }
    extraction
}

pub fn extract_boot_shutdown_events_from_json_records(
    records: &[Value],
    source_path: &str,
) -> EvtxBootExtraction {
    let mut extraction = EvtxBootExtraction::default();
    for record in records {
        if let Some(event) = boot_event_from_json(record, None, None, source_path) {
            extraction.events.push(event);
        }
    }
    if extraction.events.is_empty() {
        extraction.warnings.push(format!(
            "{source_path} contains no supported boot/shutdown candidate events"
        ));
    }
    extraction
}

fn boot_event_from_json(
    record: &Value,
    fallback_record_id: Option<u64>,
    fallback_timestamp: Option<String>,
    source_path: &str,
) -> Option<EvtxBootEvent> {
    let system = record
        .get("Event")?
        .get("System")
        .or_else(|| record.get("System"))?;
    let event_id = event_id(system.get("EventID")?)?;
    let kind = match event_id {
        6005 => EvtxBootEventKind::EventLogStarted,
        6006 => EvtxBootEventKind::EventLogStopped,
        6008 => EvtxBootEventKind::UnexpectedShutdown,
        1074 => EvtxBootEventKind::PlannedShutdown,
        _ => return None,
    };
    let timestamp = event_timestamp(system)
        .or(fallback_timestamp)
        .unwrap_or_else(|| "unknown".to_string());
    let record_id = event_record_id(system).or(fallback_record_id);
    let provider = provider_name(system);
    let note = kind.note().to_string();

    Some(EvtxBootEvent {
        timestamp,
        event_id,
        record_id,
        provider,
        kind,
        source_path: source_path.to_string(),
        note,
    })
}

fn event_id(value: &Value) -> Option<u32> {
    match value {
        Value::Number(number) => number.as_u64().and_then(|value| u32::try_from(value).ok()),
        Value::String(text) => text.parse().ok(),
        Value::Object(map) => map
            .get("#text")
            .or_else(|| map.get("Text"))
            .or_else(|| map.get("Value"))
            .and_then(event_id),
        _ => None,
    }
}

fn event_record_id(system: &Value) -> Option<u64> {
    match system.get("EventRecordID")? {
        Value::Number(number) => number.as_u64(),
        Value::String(text) => text.parse().ok(),
        _ => None,
    }
}

fn event_timestamp(system: &Value) -> Option<String> {
    let time_created = system.get("TimeCreated")?;
    let raw = match time_created {
        Value::Object(map) => map
            .get("@SystemTime")
            .or_else(|| map.get("SystemTime"))
            .and_then(Value::as_str),
        Value::String(text) => Some(text.as_str()),
        _ => None,
    }?;
    normalize_timestamp(raw)
}

fn normalize_timestamp(raw: &str) -> Option<String> {
    DateTime::parse_from_rfc3339(raw)
        .map(|dt| dt.with_timezone(&Utc).to_rfc3339())
        .ok()
        .or_else(|| {
            DateTime::parse_from_str(raw, "%Y-%m-%d %H:%M:%S%.f %z")
                .map(|dt| dt.with_timezone(&Utc).to_rfc3339())
                .ok()
        })
        .or_else(|| Some(raw.to_string()).filter(|value| !value.trim().is_empty()))
}

fn provider_name(system: &Value) -> Option<String> {
    let provider = system.get("Provider")?;
    match provider {
        Value::Object(map) => map
            .get("@Name")
            .or_else(|| map.get("Name"))
            .and_then(Value::as_str)
            .map(str::to_string),
        Value::String(text) => Some(text.clone()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn extract_eventlog_started_6005_from_json() {
        let extraction = extract_boot_shutdown_events_from_json_records(
            &[json!({
                "Event": {
                    "System": {
                        "Provider": { "@Name": "EventLog" },
                        "EventID": 6005,
                        "EventRecordID": 41,
                        "TimeCreated": { "@SystemTime": "2026-01-01T00:00:00Z" }
                    }
                }
            })],
            "Windows/System32/winevt/Logs/System.evtx",
        );

        assert!(extraction.warnings.is_empty());
        assert_eq!(extraction.events.len(), 1);
        assert_eq!(extraction.events[0].event_id, 6005);
        assert_eq!(
            extraction.events[0].kind,
            EvtxBootEventKind::EventLogStarted
        );
        assert_eq!(extraction.events[0].record_id, Some(41));
        assert_eq!(extraction.events[0].provider.as_deref(), Some("EventLog"));
    }

    #[test]
    fn extract_shutdown_candidates_from_json_string_event_id() {
        let extraction = extract_boot_shutdown_events_from_json_records(
            &[
                json!({"Event":{"System":{"EventID":"6006","TimeCreated":{"SystemTime":"2026-01-01T01:00:00Z"}}}}),
                json!({"Event":{"System":{"EventID":{"#text":"6008"},"TimeCreated":"2026-01-01T02:00:00Z"}}}),
                json!({"Event":{"System":{"EventID":"1074","TimeCreated":{"@SystemTime":"2026-01-01T03:00:00Z"}}}}),
            ],
            "Windows/System32/winevt/Logs/System.evtx",
        );

        let kinds = extraction
            .events
            .iter()
            .map(|event| event.kind.as_str())
            .collect::<Vec<_>>();
        assert_eq!(
            kinds,
            vec!["eventLogStopped", "unexpectedShutdown", "plannedShutdown"]
        );
    }

    #[test]
    fn ignores_unsupported_event_ids() {
        let extraction = extract_boot_shutdown_events_from_json_records(
            &[json!({"Event":{"System":{"EventID":7045}}})],
            "Windows/System32/winevt/Logs/System.evtx",
        );

        assert!(extraction.events.is_empty());
        assert!(extraction.warnings[0].contains("no supported"));
    }

    #[test]
    fn malformed_evtx_returns_warning_not_panic() {
        let extraction = extract_boot_shutdown_events(
            b"not an evtx",
            "Windows/System32/winevt/Logs/System.evtx",
        );

        assert!(extraction.events.is_empty());
        assert!(extraction.warnings[0].contains("parser initialization failed"));
    }

    #[test]
    fn truncated_evtx_magic_returns_warning_not_panic() {
        let extraction =
            extract_boot_shutdown_events(b"ElfFile\0", "Windows/System32/winevt/Logs/System.evtx");

        assert!(extraction.events.is_empty());
        assert!(!extraction.warnings.is_empty());
        assert!(extraction
            .warnings
            .iter()
            .any(|warning| warning.contains("parser") || warning.contains("parse")));
    }

    #[test]
    fn oversized_evtx_returns_not_parsed_warning() {
        let bytes = vec![0u8; MAX_EVTX_ANALYSIS_BYTES + 1];
        let extraction =
            extract_boot_shutdown_events(&bytes, "Windows/System32/winevt/Logs/System.evtx");

        assert!(extraction.events.is_empty());
        assert!(extraction.warnings[0].contains("exceeds bounded EVTX parser limit"));
    }
}
