use crate::analysis_service::cancellation::ensure_not_cancelled;
use crate::analysis_service::candidates::EvidenceCandidate;
use std::io::Read;
use std::sync::atomic::AtomicBool;

const READ_BUFFER_BYTES: usize = 64 * 1024;

#[derive(Debug)]
pub(super) enum CandidateExtractionError {
    Warning(String),
    Cancelled,
}

pub(super) enum CandidateSource {
    Reader(Box<dyn Read>),
    Bytes(Vec<u8>),
}

pub(super) fn read_candidate_bytes_with_progress(
    candidate: &EvidenceCandidate,
    read_limit: usize,
    cancel_token: &AtomicBool,
    file_reader: &mut impl FnMut(&EvidenceCandidate, usize) -> Result<CandidateSource, String>,
    mut on_read: impl FnMut(usize),
) -> Result<Vec<u8>, CandidateExtractionError> {
    check_candidate_cancelled(cancel_token)?;
    let source = file_reader(candidate, read_limit).map_err(|error| {
        CandidateExtractionError::Warning(format!("{} read failed: {error}", candidate.path))
    })?;
    check_candidate_cancelled(cancel_token)?;
    let mut reader = match source {
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
