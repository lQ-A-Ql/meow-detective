use super::ExtractionOutcome;
use crate::analysis_service::candidates::EvidenceCandidate;

pub(super) struct RegistryExtractionContext<'a> {
    pub(super) candidate: &'a EvidenceCandidate,
    pub(super) bytes: &'a [u8],
    pub(super) boot_key: Option<[u8; 16]>,
    pub(super) txlog1: Option<&'a [u8]>,
    pub(super) txlog2: Option<&'a [u8]>,
    pub(super) outcome: &'a mut ExtractionOutcome,
    pub(super) warnings: &'a mut Vec<String>,
}
