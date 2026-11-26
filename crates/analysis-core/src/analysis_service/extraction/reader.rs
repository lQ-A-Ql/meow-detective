use crate::analysis_service::cancellation::ensure_not_cancelled;
use crate::analysis_service::candidates::EvidenceCandidate;
use evidence_access::file_service::FileServiceError;
use std::io::Read;
use std::sync::atomic::AtomicBool;

const READ_BUFFER_BYTES: usize = 64 * 1024;
const EFS_ANALYSIS_UNSUPPORTED_WARNING: &str =
    "Evidence content is not explicitly clear: it is NTFS EFS-encrypted or its encryption status is unknown; unsupported for analysis and content was not read";

pub(crate) fn encrypted_candidate_warning(candidate: &EvidenceCandidate) -> Option<String> {
    candidate
        .encrypted
        .then(|| EFS_ANALYSIS_UNSUPPORTED_WARNING.to_string())
}

#[derive(Debug)]
pub(super) enum CandidateExtractionError {
    Warning(String),
    CorruptInput(String),
    Cancelled,
}

pub(crate) enum CandidateSource {
    Seekable(Box<dyn evidence_core::ReadSeek>),
    Reader(Box<dyn Read>),
    Bytes(Vec<u8>),
}

pub(crate) type CandidateReadResult = Result<CandidateSource, FileServiceError>;

pub(super) fn corrupt_read(error: &FileServiceError) -> bool {
    matches!(
        error,
        FileServiceError::Io(error)
            if matches!(error.kind(), std::io::ErrorKind::InvalidData | std::io::ErrorKind::UnexpectedEof)
    )
}

pub(super) fn read_candidate_bytes_with_progress(
    candidate: &EvidenceCandidate,
    read_limit: usize,
    cancel_token: &AtomicBool,
    file_reader: &mut impl FnMut(&EvidenceCandidate, usize) -> CandidateReadResult,
    on_read: impl FnMut(usize),
) -> Result<Vec<u8>, CandidateExtractionError> {
    check_candidate_cancelled(cancel_token)?;
    if let Some(warning) = encrypted_candidate_warning(candidate) {
        return Err(CandidateExtractionError::Warning(warning));
    }
    let source = file_reader(candidate, read_limit).map_err(|error| {
        let reason = format!("{} read failed: {error}", candidate.path);
        if corrupt_read(&error) {
            CandidateExtractionError::CorruptInput(reason)
        } else {
            CandidateExtractionError::Warning(reason)
        }
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
            let reason = format!("{} read failed: {error}", candidate.path);
            if matches!(
                error.kind(),
                std::io::ErrorKind::InvalidData | std::io::ErrorKind::UnexpectedEof
            ) {
                CandidateExtractionError::CorruptInput(reason)
            } else {
                CandidateExtractionError::Warning(reason)
            }
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
