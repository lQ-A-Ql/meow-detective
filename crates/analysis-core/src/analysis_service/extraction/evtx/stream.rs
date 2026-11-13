use super::{project_application_event, project_boot_event, project_security_event};
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
use artifacts_windows::{EvtxStructuredEvent, EvtxVisitError};
use std::{
    io::{self, Read, Seek, SeekFrom},
    sync::atomic::{AtomicBool, Ordering},
};

pub(in crate::analysis_service::extraction) fn persist_reader(
    generation: &StreamingGeneration,
    candidate: &EvidenceCandidate,
    reader: Box<dyn evidence_core::ReadSeek>,
    cancel: &AtomicBool,
) -> Result<PersistedExtractionOutcome, AnalysisServiceError> {
    ensure_not_cancelled(cancel)?;
    let mut batch = ConcurrentOutputBatch::new(generation);
    let reader = CancellableReader {
        inner: reader,
        cancel,
    };
    let summary = artifacts_windows::visit_structured_events_from_read_seek(
        reader,
        &candidate.path,
        |event| {
            ensure_not_cancelled(cancel)?;
            let mut outcome = ExtractionOutcome::default();
            match event {
                EvtxStructuredEvent::Boot(event) => {
                    project_boot_event(candidate, &event, &mut outcome)
                }
                EvtxStructuredEvent::Security(event) => {
                    project_security_event(candidate, &event, &mut outcome)
                }
                EvtxStructuredEvent::Application(event) => {
                    project_application_event(candidate, &event, &mut outcome)
                }
            }
            batch.push(outcome);
            if batch.pending_artifact_count() >= 256 {
                batch.flush()?;
            }
            Ok::<(), AnalysisServiceError>(())
        },
    )
    .map_err(|error| {
        if cancel.load(Ordering::Acquire) {
            return AnalysisServiceError::Cancelled;
        }
        match error {
            EvtxVisitError::Parser(error) => AnalysisServiceError::Extraction(error.to_string()),
            EvtxVisitError::Sink(error) => error,
        }
    })?;
    ensure_not_cancelled(cancel)?;
    batch.finish(summary.warnings)
}

struct CancellableReader<'a> {
    inner: Box<dyn evidence_core::ReadSeek>,
    cancel: &'a AtomicBool,
}

impl CancellableReader<'_> {
    fn check_cancelled(&self) -> io::Result<()> {
        if self.cancel.load(Ordering::Acquire) {
            Err(io::Error::new(
                io::ErrorKind::ConnectionAborted,
                "EVTX analysis cancelled",
            ))
        } else {
            Ok(())
        }
    }
}

impl Read for CancellableReader<'_> {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        self.check_cancelled()?;
        let count = self.inner.read(buffer)?;
        self.check_cancelled()?;
        Ok(count)
    }
}

impl Seek for CancellableReader<'_> {
    fn seek(&mut self, from: SeekFrom) -> io::Result<u64> {
        self.check_cancelled()?;
        let position = self.inner.seek(from)?;
        self.check_cancelled()?;
        Ok(position)
    }
}

#[cfg(test)]
#[path = "../../../../tests/unit/analysis_service/extraction/evtx_stream.rs"]
mod tests;
