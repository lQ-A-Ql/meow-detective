use super::{LinuxLogTimeContext, TextLogParser};
use crate::analysis_service::{
    cancellation::ensure_not_cancelled,
    candidates::EvidenceCandidate,
    extraction::{
        native_output::{ConcurrentOutputBatch, StreamingGeneration},
        state::PersistedExtractionOutcome,
        ExtractionOutcome,
    },
    AnalysisServiceError,
};
use std::io::{BufRead, BufReader, Read};
use std::sync::{
    atomic::{AtomicBool, AtomicU64, Ordering},
    Arc,
};

const MAX_LINE_BYTES: u64 = 1024 * 1024;

pub(super) struct VisitSummary {
    pub(super) warnings: Vec<String>,
    pub(super) source_bytes: u64,
}

pub(in crate::analysis_service::extraction) fn persist_reader(
    generation: &StreamingGeneration,
    candidate: &EvidenceCandidate,
    reader: Box<dyn Read>,
    log_time: &LinuxLogTimeContext,
    cancel: &AtomicBool,
) -> Result<PersistedExtractionOutcome, AnalysisServiceError> {
    let mut batch = ConcurrentOutputBatch::new(generation);
    let mut pending_bytes = 0;
    let summary = visit(reader, candidate, log_time, cancel, |outcome, length| {
        batch.push(outcome);
        pending_bytes += length;
        if batch.pending_artifact_count() >= 256 || pending_bytes >= 512 * 1024 {
            batch.flush()?;
            pending_bytes = 0;
        }
        Ok(())
    })?;
    if summary.source_bytes != candidate.size {
        return Err(AnalysisServiceError::Read(
            "Linux log input length differs from catalog".into(),
        ));
    }
    if !summary.warnings.is_empty() {
        batch.push(ExtractionOutcome {
            diagnostic_counts: transport::dto::AnalysisDiagnosticCountsDto {
                capability_limits: 1,
                ..Default::default()
            },
            ..Default::default()
        });
    }
    batch.finish(summary.warnings)
}

pub(super) fn visit<'a>(
    reader: Box<dyn Read + 'a>,
    candidate: &EvidenceCandidate,
    log_time: &LinuxLogTimeContext,
    cancel: &AtomicBool,
    mut consume: impl FnMut(ExtractionOutcome, usize) -> Result<(), AnalysisServiceError>,
) -> Result<VisitSummary, AnalysisServiceError> {
    let count = Arc::new(AtomicU64::new(0));
    let source = CountingReader {
        inner: reader,
        count: count.clone(),
    };
    let decoded: Box<dyn Read + 'a> = if candidate.path.to_ascii_lowercase().ends_with(".gz") {
        Box::new(flate2::read::MultiGzDecoder::new(source))
    } else {
        Box::new(source)
    };
    let mut input = BufReader::with_capacity(64 * 1024, decoded);
    let scoped_time = log_time.for_candidate(candidate);
    let mut parser = TextLogParser::new(candidate, "linux.text_log", "log", &scoped_time);
    let mut line = Vec::new();
    let mut line_number = 0;
    let mut offset = 0u64;
    loop {
        ensure_not_cancelled(cancel)?;
        line.clear();
        let length = input
            .by_ref()
            .take(MAX_LINE_BYTES + 1)
            .read_until(b'\n', &mut line)
            .map_err(|error| {
                if error.kind() == std::io::ErrorKind::InvalidData
                    || (error.kind() == std::io::ErrorKind::UnexpectedEof
                        && count.load(Ordering::Relaxed) == candidate.size)
                {
                    AnalysisServiceError::CorruptInput(error.to_string())
                } else {
                    AnalysisServiceError::Io(error)
                }
            })?;
        if length == 0 {
            break;
        }
        if length as u64 > MAX_LINE_BYTES {
            return Err(AnalysisServiceError::Unsupported(
                "Linux log line exceeds 1 MiB".into(),
            ));
        }
        line_number += 1;
        consume(parser.parse_bytes(line_number, offset, &line), length)?;
        offset = offset.checked_add(length as u64).ok_or_else(|| {
            AnalysisServiceError::InvalidInput("Linux log offset overflow".into())
        })?;
    }
    ensure_not_cancelled(cancel)?;
    let mut warnings = parser.finish();
    if crate::analysis_service::artifact_builders::is_docker_overlay_path(
        &crate::analysis_service::candidates::normalize_evidence_path(&candidate.path),
    ) {
        warnings.push(format!(
            "{} contains container-scoped Linux log records",
            candidate.path
        ));
    }
    Ok(VisitSummary {
        warnings,
        source_bytes: count.load(Ordering::Relaxed),
    })
}

struct CountingReader<'a> {
    inner: Box<dyn Read + 'a>,
    count: Arc<AtomicU64>,
}

impl Read for CountingReader<'_> {
    fn read(&mut self, output: &mut [u8]) -> std::io::Result<usize> {
        let length = self.inner.read(output)?;
        self.count.fetch_add(length as u64, Ordering::Relaxed);
        Ok(length)
    }
}

#[cfg(test)]
#[path = "../../../../../tests/unit/analysis_service/extraction/linux_text_stream.rs"]
mod tests;
