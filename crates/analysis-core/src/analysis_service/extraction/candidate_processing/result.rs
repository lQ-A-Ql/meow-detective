use super::{
    AnalysisCapability, AnalysisServiceError, EvidenceCandidate, PersistedExtractionOutcome,
};
use crate::analysis_service::extraction::PluginExtractFailure;
use persistence_sqlite::repositories::analysis_scan_repo::CompleteAnalysisCandidateScan;

pub(super) struct CandidateCompletion {
    pub(super) candidate: EvidenceCandidate,
    pub(super) capability: AnalysisCapability,
    pub(super) kind: CandidateCompletionKind,
}

pub(super) enum CandidateCompletionKind {
    Persisted {
        observation: PersistedExtractionOutcome,
        plugin_failures: Vec<PluginExtractFailure>,
    },
    Deferred(String),
    InputDiagnostic {
        warning: String,
        diagnostic_counts: transport::dto::AnalysisDiagnosticCountsDto,
    },
    Blocked(String),
    Warning(String),
    ReplayDiagnostic(
        persistence_sqlite::repositories::analysis_scan_repo::DiagnosticAnalysisCandidateScan,
    ),
    ReplayClean,
    ReplayComplete(CompleteAnalysisCandidateScan),
    Failed(AnalysisServiceError),
    Cancelled,
}

impl CandidateCompletionKind {
    pub(super) fn corrupt_input(warning: String) -> Self {
        Self::InputDiagnostic {
            warning,
            diagnostic_counts: transport::dto::AnalysisDiagnosticCountsDto {
                corrupt_inputs: 1,
                ..Default::default()
            },
        }
    }

    pub(super) fn capability_limit(warning: String) -> Self {
        Self::InputDiagnostic {
            warning,
            diagnostic_counts: transport::dto::AnalysisDiagnosticCountsDto {
                capability_limits: 1,
                ..Default::default()
            },
        }
    }
}

pub(super) fn ready_corrupt_input(
    candidate: EvidenceCandidate,
    capability: AnalysisCapability,
    warning: String,
) -> super::PreparedWork<super::PreparedCandidate, CandidateCompletion> {
    super::PreparedWork::Ready(CandidateCompletion {
        candidate,
        capability,
        kind: CandidateCompletionKind::corrupt_input(warning),
    })
}

pub(super) fn ready_read_error(
    candidate: EvidenceCandidate,
    capability: AnalysisCapability,
    error: super::CandidateExtractionError,
) -> Result<super::PreparedWork<super::PreparedCandidate, CandidateCompletion>, AnalysisServiceError>
{
    match error {
        super::CandidateExtractionError::Warning(warning) => {
            Ok(ready_warning(candidate, capability, warning))
        }
        super::CandidateExtractionError::CorruptInput(reason) => {
            Ok(ready_corrupt_input(candidate, capability, reason))
        }
        super::CandidateExtractionError::Cancelled => Err(AnalysisServiceError::Cancelled),
    }
}

pub(super) fn ready_warning(
    candidate: EvidenceCandidate,
    capability: AnalysisCapability,
    warning: String,
) -> super::PreparedWork<super::PreparedCandidate, CandidateCompletion> {
    super::PreparedWork::Ready(CandidateCompletion {
        candidate,
        capability,
        kind: CandidateCompletionKind::Warning(warning),
    })
}
