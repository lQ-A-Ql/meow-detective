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

pub(super) fn bounded_clean_evtx_bytes(bytes: &[u8]) -> &[u8] {
    if bytes.len() < EVTX_FILE_HEADER_SIZE as usize + 128 || !bytes.starts_with(b"ElfFile\0") {
        return bytes;
    }
    let chunk_count = u16::from_le_bytes(bytes[42..44].try_into().unwrap_or([0; 2])) as usize;
    let flags = u32::from_le_bytes(bytes[120..124].try_into().unwrap_or([0; 4]));
    if flags & 0x1 != 0 || chunk_count == 0 {
        return bytes;
    }
    let declared_len = (EVTX_FILE_HEADER_SIZE as usize)
        .saturating_add(chunk_count.saturating_mul(EVTX_CHUNK_SIZE as usize));
    if declared_len > EVTX_FILE_HEADER_SIZE as usize && declared_len < bytes.len() {
        &bytes[..declared_len]
    } else {
        bytes
    }
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
