use super::records::structured_event_from_json;
use super::types::{EvtxBootExtraction, EvtxStructuredExtraction};
use crate::evtx::capability::supports_evtx_boot_shutdown_path;
use crate::evtx::error::EvtxBootError;
use evtx::{err::EvtxError, EvtxParser};
use serde_json::Value;
use std::collections::BTreeSet;

pub const MAX_EVTX_ANALYSIS_BYTES: usize = 16 * 1024 * 1024;
pub(super) const EVTX_FILE_HEADER_SIZE: u64 = 4096;
pub(super) const EVTX_CHUNK_SIZE: u64 = 65536;
const MAX_EVTX_WARNINGS: usize = 64;

pub fn extract_boot_shutdown_events(
    bytes: &[u8],
    source_path: &str,
) -> Result<EvtxBootExtraction, EvtxBootError> {
    let structured = extract_structured_events(bytes, source_path)?;
    Ok(EvtxBootExtraction {
        events: structured.boot_events,
        warnings: structured.warnings,
    })
}

pub fn extract_boot_shutdown_events_from_json_records(
    records: &[Value],
    source_path: &str,
) -> Result<EvtxBootExtraction, EvtxBootError> {
    let structured = extract_structured_events_from_json_records(records, source_path)?;
    Ok(EvtxBootExtraction {
        events: structured.boot_events,
        warnings: structured.warnings,
    })
}

pub fn extract_structured_events(
    bytes: &[u8],
    source_path: &str,
) -> Result<EvtxStructuredExtraction, EvtxBootError> {
    validate_input(bytes, source_path)?;
    let parser_bytes = bounded_clean_evtx_bytes(bytes);
    let mut parser = EvtxParser::from_buffer(parser_bytes.to_vec()).map_err(|err| {
        EvtxBootError::ParserInit {
            path: source_path.to_string(),
            detail: err.to_string(),
        }
    })?;

    let mut raw_warnings = Vec::new();
    let mut extraction = EvtxStructuredExtraction::default();
    for record in parser.records_json_value() {
        match record {
            Ok(record) => structured_event_from_json(
                &record.data,
                Some(record.event_record_id),
                Some(record.timestamp.to_string()),
                source_path,
                &mut extraction,
            ),
            Err(err) => raw_warnings.push(format_evtx_warning(source_path, &err)),
        }
    }
    extraction.warnings = govern_evtx_warnings(source_path, raw_warnings);
    Ok(extraction)
}

pub fn extract_structured_events_from_json_records(
    records: &[Value],
    source_path: &str,
) -> Result<EvtxStructuredExtraction, EvtxBootError> {
    validate_path(source_path)?;
    let mut extraction = EvtxStructuredExtraction::default();
    for record in records {
        structured_event_from_json(record, None, None, source_path, &mut extraction);
    }
    extraction.warnings = govern_evtx_warnings(source_path, Vec::new());
    Ok(extraction)
}

fn validate_input(bytes: &[u8], source_path: &str) -> Result<(), EvtxBootError> {
    validate_path(source_path)?;
    if bytes.len() > MAX_EVTX_ANALYSIS_BYTES {
        return Err(EvtxBootError::InputTooLarge {
            path: source_path.to_string(),
            size: bytes.len(),
            max: MAX_EVTX_ANALYSIS_BYTES,
        });
    }
    Ok(())
}

fn validate_path(source_path: &str) -> Result<(), EvtxBootError> {
    if supports_evtx_boot_shutdown_path(source_path) {
        Ok(())
    } else {
        Err(EvtxBootError::UnsupportedPath {
            path: source_path.to_string(),
        })
    }
}

/// Bound the parser input to complete 64KiB chunks.
///
/// The header's declared chunk count is *not* trusted: on images captured
/// before the log service flushed its header update the count lags behind the
/// actual file size (with or without the dirty flag), and slicing there
/// silently discards the newest events. Every complete chunk is kept and the
/// parser's own per-chunk validation decides what is readable; only a
/// trailing partial chunk, which cannot hold complete records, is dropped.
pub(super) fn bounded_clean_evtx_bytes(bytes: &[u8]) -> &[u8] {
    if bytes.len() < EVTX_FILE_HEADER_SIZE as usize || !bytes.starts_with(b"ElfFile\0") {
        return bytes;
    }
    let data_len = bytes.len() - EVTX_FILE_HEADER_SIZE as usize;
    let complete_len = data_len / EVTX_CHUNK_SIZE as usize * EVTX_CHUNK_SIZE as usize;
    &bytes[..EVTX_FILE_HEADER_SIZE as usize + complete_len]
}

/// Diagnostic helper for tail-truncation analysis: return the newest `count`
/// records of *any* kind as `(record_id, event_id, timestamp)` triples,
/// bypassing the curated kind filter entirely.
#[doc(hidden)]
pub fn probe_newest_records(bytes: &[u8], count: usize) -> Vec<(u64, u64, String)> {
    let parser_bytes = bounded_clean_evtx_bytes(bytes);
    let Ok(mut parser) = EvtxParser::from_buffer(parser_bytes.to_vec()) else {
        return Vec::new();
    };
    let mut records = Vec::new();
    for record in parser.records_json_value().flatten() {
        let event_id = record
            .data
            .get("Event")
            .and_then(|event| event.get("System"))
            .and_then(|system| system.get("EventID"))
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0);
        records.push((
            record.event_record_id,
            event_id,
            record.timestamp.to_string(),
        ));
    }
    records.sort_by_key(|(record_id, _, _)| *record_id);
    records.split_off(records.len().saturating_sub(count))
}

pub(super) fn format_evtx_warning(source_path: &str, err: &EvtxError) -> String {
    match err {
        EvtxError::FailedToParseChunk { chunk_id, source } => {
            let offset = EVTX_FILE_HEADER_SIZE + (*chunk_id).saturating_mul(EVTX_CHUNK_SIZE);
            EvtxBootError::ChunkParse {
                path: source_path.to_string(),
                chunk_id: *chunk_id,
                offset,
                detail: source.to_string(),
            }
            .to_string()
        }
        EvtxError::FailedToParseRecord { record_id, source } => EvtxBootError::RecordParse {
            path: source_path.to_string(),
            record_id: Some(*record_id),
            detail: source.to_string(),
        }
        .to_string(),
        other => EvtxBootError::RecordParse {
            path: source_path.to_string(),
            record_id: None,
            detail: other.to_string(),
        }
        .to_string(),
    }
}

fn govern_evtx_warnings(path: &str, raw: Vec<String>) -> Vec<String> {
    let sanitized = sanitize_evtx_path(path);
    let mut seen = BTreeSet::new();
    let mut governed = Vec::with_capacity(raw.len().min(MAX_EVTX_WARNINGS));
    for message in raw {
        let code = evtx_warning_code_for(&message);
        let entry = format!("[{code}] {sanitized}: {message}");
        if !seen.insert(entry.clone()) {
            continue;
        }
        if governed.len() >= MAX_EVTX_WARNINGS {
            let cap = format!("[EVTX-WARN-CAP] {sanitized}: additional EVTX warnings suppressed");
            if seen.insert(cap.clone()) {
                governed.push(cap);
            }
            break;
        }
        governed.push(entry);
    }
    governed
}

fn evtx_warning_code_for(message: &str) -> &'static str {
    if message.contains("parser initialization failed") {
        "EVTX-INIT"
    } else if message.contains("chunk parse warning") {
        "EVTX-CHUNK"
    } else if message.contains("record parse warning") {
        "EVTX-RECORD"
    } else if message.contains("no supported") {
        "EVTX-EMPTY"
    } else if message.contains("exceeds bounded EVTX parser limit") {
        "EVTX-LIMIT"
    } else if message.contains("outside bounded") {
        "EVTX-SCOPE"
    } else {
        "EVTX-WARN"
    }
}

fn sanitize_evtx_path(path: &str) -> String {
    path.replace('\\', "/")
        .rsplit('/')
        .next()
        .unwrap_or(path)
        .to_string()
}
