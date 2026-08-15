use super::records::structured_event_from_json;
use super::types::{EvtxBootExtraction, EvtxStructuredEvent, EvtxStructuredExtraction};
use crate::evtx::capability::supports_evtx_boot_shutdown_path;
use crate::evtx::error::EvtxBootError;
use evtx::{err::EvtxError, EvtxParser, ParserSettings};
use serde_json::Value;
use std::collections::BTreeSet;
use std::io::{Cursor, Read, Seek};

pub const MAX_EVTX_ANALYSIS_BYTES: usize = 16 * 1024 * 1024;
pub(super) const EVTX_FILE_HEADER_SIZE: u64 = 4096;
pub(super) const EVTX_CHUNK_SIZE: u64 = 65536;
const MAX_EVTX_WARNINGS: usize = 64;
const MAX_EVTX_PARSER_THREADS: usize = 4;

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
    extract_structured_events_from_read_seek(Cursor::new(parser_bytes.to_vec()), source_path)
}

/// Parse a complete EVTX stream without copying the whole evidence file into memory.
pub(crate) fn extract_structured_events_from_read_seek<R>(
    reader: R,
    source_path: &str,
) -> Result<EvtxStructuredExtraction, EvtxBootError>
where
    R: Read + Seek,
{
    let mut extraction = EvtxStructuredExtraction::default();
    let summary = visit_structured_events_from_read_seek(reader, source_path, |event| {
        extraction.push(event);
        Ok::<(), std::convert::Infallible>(())
    })
    .map_err(|error| match error {
        EvtxVisitError::Parser(error) => error,
        EvtxVisitError::Sink(error) => match error {},
    })?;
    extraction.warnings = summary.warnings;
    Ok(extraction)
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EvtxVisitSummary {
    pub boot_count: u64,
    pub security_count: u64,
    pub application_count: u64,
    pub warnings: Vec<String>,
}

#[derive(Debug)]
pub enum EvtxVisitError<E> {
    Parser(EvtxBootError),
    Sink(E),
}

pub fn visit_structured_events_from_read_seek<R, F, E>(
    reader: R,
    source_path: &str,
    mut visitor: F,
) -> Result<EvtxVisitSummary, EvtxVisitError<E>>
where
    R: Read + Seek,
    F: FnMut(EvtxStructuredEvent) -> Result<(), E>,
{
    validate_path(source_path).map_err(EvtxVisitError::Parser)?;
    let mut parser = EvtxParser::from_read_seek(reader).map_err(|error| {
        EvtxVisitError::Parser(parser_initialization_error(source_path, &error))
    })?;
    parser = parser
        .with_configuration(ParserSettings::default().num_threads(evtx_parser_thread_count()));

    let mut warnings = EvtxWarningCollector::new(source_path);
    let mut summary = EvtxVisitSummary::default();
    for record in parser.records_json_value() {
        match record {
            Ok(record) => {
                let event = structured_event_from_json(
                    &record.data,
                    Some(record.event_record_id),
                    Some(record.timestamp.to_string()),
                    source_path,
                );
                if let Some(event) = event {
                    summary.record(&event);
                    visitor(event).map_err(EvtxVisitError::Sink)?;
                }
            }
            Err(error) => {
                if let Some(error) = fatal_parser_error(source_path, &error) {
                    return Err(EvtxVisitError::Parser(error));
                }
                warnings.push(format_evtx_warning(source_path, &error));
            }
        }
    }
    summary.warnings = warnings.finish();
    Ok(summary)
}

impl EvtxVisitSummary {
    fn record(&mut self, event: &EvtxStructuredEvent) {
        match event {
            EvtxStructuredEvent::Boot(_) => self.boot_count += 1,
            EvtxStructuredEvent::Security(_) => self.security_count += 1,
            EvtxStructuredEvent::Application(_) => self.application_count += 1,
        }
    }
}

fn evtx_parser_thread_count() -> usize {
    std::thread::available_parallelism()
        .map(usize::from)
        .unwrap_or(1)
        .clamp(1, MAX_EVTX_PARSER_THREADS)
}

pub(crate) fn extract_structured_events_from_json_records(
    records: &[Value],
    source_path: &str,
) -> Result<EvtxStructuredExtraction, EvtxBootError> {
    validate_path(source_path)?;
    let mut extraction = EvtxStructuredExtraction::default();
    for record in records {
        if let Some(event) = structured_event_from_json(record, None, None, source_path) {
            extraction.push(event);
        }
    }
    extraction.warnings = govern_evtx_warnings(source_path, Vec::new());
    Ok(extraction)
}

fn validate_input(bytes: &[u8], source_path: &str) -> Result<(), EvtxBootError> {
    validate_path(source_path)?;
    if bytes.len() > MAX_EVTX_ANALYSIS_BYTES {
        return Err(EvtxBootError::InputTooLarge {
            path: sanitize_evtx_path(source_path),
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
            path: sanitize_evtx_path(source_path),
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

pub(super) fn govern_evtx_warnings(path: &str, raw: Vec<String>) -> Vec<String> {
    let mut collector = EvtxWarningCollector::new(path);
    for message in raw {
        collector.push(message);
    }
    collector.finish()
}

struct EvtxWarningCollector {
    source_path: String,
    sanitized_path: String,
    seen: BTreeSet<String>,
    warnings: Vec<String>,
    capped: bool,
}

impl EvtxWarningCollector {
    fn new(path: &str) -> Self {
        Self {
            source_path: path.to_string(),
            sanitized_path: sanitize_evtx_path(path),
            seen: BTreeSet::new(),
            warnings: Vec::with_capacity(MAX_EVTX_WARNINGS),
            capped: false,
        }
    }

    fn push(&mut self, message: String) {
        if self.capped {
            return;
        }
        let message = message.replace(&self.source_path, &self.sanitized_path);
        let code = evtx_warning_code_for(&message);
        let entry = format!("[{code}] {}: {message}", self.sanitized_path);
        if !self.seen.insert(entry.clone()) {
            return;
        }
        if self.warnings.len() < MAX_EVTX_WARNINGS {
            self.warnings.push(entry);
            return;
        }
        self.warnings.pop();
        self.warnings.push(format!(
            "[EVTX-WARN-CAP] {}: additional EVTX warnings suppressed",
            self.sanitized_path
        ));
        self.capped = true;
    }

    fn finish(self) -> Vec<String> {
        self.warnings
    }
}

fn fatal_parser_error(source_path: &str, error: &EvtxError) -> Option<EvtxBootError> {
    let (operation, source) = error.source_io()?;
    Some(EvtxBootError::SourceIo {
        path: sanitize_evtx_path(source_path),
        operation: operation.to_string(),
        kind: source.kind(),
    })
}

fn parser_initialization_error(source_path: &str, error: &EvtxError) -> EvtxBootError {
    if let Some(error) = fatal_parser_error(source_path, error) {
        return error;
    }
    EvtxBootError::ParserInit {
        path: sanitize_evtx_path(source_path),
        detail: error.to_string(),
    }
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
