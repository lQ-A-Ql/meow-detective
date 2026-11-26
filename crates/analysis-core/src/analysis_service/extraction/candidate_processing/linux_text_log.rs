use super::*;
use std::io::Cursor;

pub(super) fn prepare<F>(
    coordinator: &mut CandidateCoordinator<'_, '_, '_, F>,
    candidate: EvidenceCandidate,
    capability: AnalysisCapability,
) -> Result<PreparedWork<PreparedCandidate, CandidateCompletion>, AnalysisServiceError>
where
    F: FnMut(&EvidenceCandidate, usize) -> CandidateReadResult,
{
    let source =
        match (coordinator.file_reader)(&candidate, candidate.size.min(usize::MAX as u64) as usize)
        {
            Ok(source) => source,
            Err(error) => {
                return Ok(super::source_preparation::ready_deferred(
                    candidate,
                    capability,
                    format!("Linux log read failed: {error}"),
                ));
            }
        };
    let reader: Box<dyn std::io::Read> = match source {
        CandidateSource::Seekable(reader) => reader,
        CandidateSource::Reader(reader) => reader,
        CandidateSource::Bytes(bytes) => Box::new(Cursor::new(bytes)),
    };
    let result = super::super::linux::persist_text_log_reader(
        coordinator.context.output,
        &candidate,
        reader,
        coordinator.context.preloads.linux_log_time,
        coordinator.context.cancel_token,
    );
    let kind = match result {
        Ok(observation) => CandidateCompletionKind::Persisted {
            observation,
            plugin_failures: Vec::new(),
        },
        Err(AnalysisServiceError::Cancelled) => return Err(AnalysisServiceError::Cancelled),
        Err(
            AnalysisServiceError::InvalidInput(error) | AnalysisServiceError::CorruptInput(error),
        ) => CandidateCompletionKind::corrupt_input(error),
        Err(AnalysisServiceError::Unsupported(error)) => {
            CandidateCompletionKind::capability_limit(error)
        }
        Err(error @ (AnalysisServiceError::Io(_) | AnalysisServiceError::Read(_))) => {
            CandidateCompletionKind::Deferred(error.to_string())
        }
        Err(error) => CandidateCompletionKind::Failed(error),
    };
    Ok(PreparedWork::Ready(CandidateCompletion {
        candidate,
        capability,
        kind,
    }))
}
