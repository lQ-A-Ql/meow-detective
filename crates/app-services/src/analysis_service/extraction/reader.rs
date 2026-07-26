use crate::analysis_service::cancellation::ensure_not_cancelled;
use crate::analysis_service::candidates::EvidenceCandidate;
use std::io::{Read, Seek, SeekFrom};
use std::sync::atomic::{AtomicBool, Ordering};

const READ_BUFFER_BYTES: usize = 64 * 1024;
const EFS_ANALYSIS_UNSUPPORTED_WARNING: &str =
    "Evidence content is not explicitly clear: it is NTFS EFS-encrypted or its encryption status is unknown; unsupported for analysis and content was not read";

pub(crate) fn encrypted_candidate_warning(candidate: &EvidenceCandidate) -> Option<String> {
    candidate
        .encrypted
        .then(|| EFS_ANALYSIS_UNSUPPORTED_WARNING.to_string())
}

pub(super) struct CancellableProgressReader<'a> {
    inner: &'a mut dyn evidence_core::ReadSeek,
    cancel_token: &'a AtomicBool,
    on_progress: &'a mut dyn FnMut(usize),
    position: u64,
    high_water: u64,
}

impl<'a> CancellableProgressReader<'a> {
    pub(super) fn new(
        inner: &'a mut dyn evidence_core::ReadSeek,
        cancel_token: &'a AtomicBool,
        on_progress: &'a mut dyn FnMut(usize),
    ) -> Self {
        Self {
            inner,
            cancel_token,
            on_progress,
            position: 0,
            high_water: 0,
        }
    }

    fn reject_cancelled(&self) -> std::io::Result<()> {
        if self.cancel_token.load(Ordering::Relaxed) {
            Err(std::io::Error::new(
                std::io::ErrorKind::Interrupted,
                "EVTX extraction cancelled",
            ))
        } else {
            Ok(())
        }
    }

    fn report_high_water(&mut self) {
        if self.position <= self.high_water {
            return;
        }
        self.high_water = self.position;
        let bytes = usize::try_from(self.high_water).unwrap_or(usize::MAX);
        (self.on_progress)(bytes);
    }
}

impl Read for CancellableProgressReader<'_> {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        self.reject_cancelled()?;
        let read = self.inner.read(buffer)?;
        self.position = self.position.saturating_add(read as u64);
        self.report_high_water();
        Ok(read)
    }
}

impl Seek for CancellableProgressReader<'_> {
    fn seek(&mut self, position: SeekFrom) -> std::io::Result<u64> {
        self.reject_cancelled()?;
        self.position = self.inner.seek(position)?;
        Ok(self.position)
    }
}

#[derive(Debug)]
pub(super) enum CandidateExtractionError {
    Warning(String),
    Cancelled,
}

pub(crate) enum CandidateSource {
    Seekable(Box<dyn evidence_core::ReadSeek>),
    Reader(Box<dyn Read>),
    Bytes(Vec<u8>),
}

pub(super) fn read_candidate_bytes_with_progress(
    candidate: &EvidenceCandidate,
    read_limit: usize,
    cancel_token: &AtomicBool,
    file_reader: &mut impl FnMut(&EvidenceCandidate, usize) -> Result<CandidateSource, String>,
    on_read: impl FnMut(usize),
) -> Result<Vec<u8>, CandidateExtractionError> {
    check_candidate_cancelled(cancel_token)?;
    if let Some(warning) = encrypted_candidate_warning(candidate) {
        return Err(CandidateExtractionError::Warning(warning));
    }
    let source = file_reader(candidate, read_limit).map_err(|error| {
        CandidateExtractionError::Warning(format!("{} read failed: {error}", candidate.path))
    })?;
    read_candidate_source_with_progress(candidate, source, read_limit, cancel_token, on_read)
}

pub(super) fn read_candidate_source_with_progress(
    candidate: &EvidenceCandidate,
    source: CandidateSource,
    read_limit: usize,
    cancel_token: &AtomicBool,
    mut on_read: impl FnMut(usize),
) -> Result<Vec<u8>, CandidateExtractionError> {
    check_candidate_cancelled(cancel_token)?;
    if let Some(warning) = encrypted_candidate_warning(candidate) {
        return Err(CandidateExtractionError::Warning(warning));
    }
    let mut reader = match source {
        CandidateSource::Seekable(reader) => reader as Box<dyn Read>,
        CandidateSource::Reader(reader) => reader,
        CandidateSource::Bytes(mut bytes) => {
            bytes.truncate(read_limit);
            on_read(bytes.len());
            return Ok(bytes);
        }
    };
    let mut bytes = Vec::with_capacity(read_limit.min(READ_BUFFER_BYTES));
    let mut buffer = [0u8; READ_BUFFER_BYTES];
    let mut limited = reader.by_ref().take(read_limit as u64);
    loop {
        check_candidate_cancelled(cancel_token)?;
        let read = limited.read(&mut buffer).map_err(|error| {
            CandidateExtractionError::Warning(format!("{} read failed: {error}", candidate.path))
        })?;
        if read == 0 {
            break;
        }
        bytes.extend_from_slice(&buffer[..read]);
        on_read(bytes.len());
    }
    check_candidate_cancelled(cancel_token)?;
    Ok(bytes)
}

fn check_candidate_cancelled(cancel_token: &AtomicBool) -> Result<(), CandidateExtractionError> {
    ensure_not_cancelled(cancel_token).map_err(|_| CandidateExtractionError::Cancelled)
}
