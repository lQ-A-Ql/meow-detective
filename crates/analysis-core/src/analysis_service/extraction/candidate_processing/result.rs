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
    Blocked(String),
    Warning(String),
    ReplayDiagnostic(Vec<String>),
    ReplayClean,
    ReplayComplete(CompleteAnalysisCandidateScan),
    ExistingV1,
    Failed(AnalysisServiceError),
    Cancelled,
}
